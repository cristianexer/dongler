//! PDF page rasterization via pdfium (PRD §4.B). Renders a page to an RGB image
//! for the layout/OCR/table models. The pdfium dynamic library is resolved at
//! runtime (bundled next to the binary, or the system library); no pdfium binary
//! is needed to *build*. Behind the `ml` feature.

use crate::ml::MlError;
use dongler_core::ir::BBox;
use image::RgbImage;
use pdfium_render::prelude::*;

/// Maps a rendered table-region crop's pixel coordinates (top-left origin, y-down)
/// back to PDF user space (bottom-left origin, y-up). Pure — unit-tested without
/// pdfium.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionTransform {
    /// PDF-space x of the crop's left edge.
    pub region_x_pt: f32,
    /// PDF-space y of the crop's TOP edge (= region.y + region.height).
    pub region_top_pt: f32,
    /// Rendered pixels per PDF point (= dpi / 72).
    pub px_per_pt: f32,
}

impl RegionTransform {
    /// Convert a crop-pixel box `(x, y, w, h)` (top-left origin) into a PDF
    /// user-space [`BBox`] (bottom-left origin, y-up).
    pub fn px_to_pdf(&self, x: f32, y: f32, w: f32, h: f32) -> BBox {
        let pdf_x = self.region_x_pt + x / self.px_per_pt;
        // The crop's top (y=0) is region_top_pt; y grows downward in pixels.
        let pdf_top = self.region_top_pt - y / self.px_per_pt;
        let pdf_h = h / self.px_per_pt;
        BBox {
            x: pdf_x,
            y: pdf_top - pdf_h, // bottom edge
            width: w / self.px_per_pt,
            height: pdf_h,
        }
    }
}

/// Bind to a pdfium library: try one bundled alongside the executable, then the
/// system library.
pub fn bind_pdfium() -> Result<Pdfium, MlError> {
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
        .or_else(|_| Pdfium::bind_to_system_library())?;
    Ok(Pdfium::new(bindings))
}

/// Render page `page_index` (0-based) of a PDF to an RGB image at `dpi`.
pub fn render_page(
    pdfium: &Pdfium,
    pdf_bytes: &[u8],
    page_index: u16,
    dpi: f32,
) -> Result<RgbImage, MlError> {
    let document = pdfium.load_pdf_from_byte_slice(pdf_bytes, None)?;
    let page = document.pages().get(page_index.into())?;
    let config = PdfRenderConfig::new().scale_page_by_factor(dpi / 72.0);
    let bitmap = page.render_with_config(&config)?;
    Ok(bitmap.as_image()?.into_rgb8())
}

/// Render a single table region to a high-DPI RGB crop and return the crop plus a
/// [`RegionTransform`] for mapping the model's cell boxes back to PDF user space.
///
/// `region_pdf` is the table rectangle in PDF user space (y-up). `page_height_pt`
/// is the page height in points (to flip y into pixel space). Renders the whole
/// page once then crops — table regions are small relative to a page, so this is
/// simpler than per-region clip rendering and exact on coordinates.
pub fn render_region(
    pdfium: &Pdfium,
    pdf_bytes: &[u8],
    page_index: u16,
    region_pdf: BBox,
    page_height_pt: f32,
    dpi: f32,
) -> Result<(RgbImage, RegionTransform), MlError> {
    let page = render_page(pdfium, pdf_bytes, page_index, dpi)?;
    let px_per_pt = dpi / 72.0;
    // Region top edge in PDF space, flipped into pixel-space top (y-down).
    let region_top_pt = region_pdf.y + region_pdf.height;
    let top_px = ((page_height_pt - region_top_pt) * px_per_pt).max(0.0);
    let left_px = (region_pdf.x * px_per_pt).max(0.0);
    let w_px = (region_pdf.width * px_per_pt).round() as u32;
    let h_px = (region_pdf.height * px_per_pt).round() as u32;

    let (pw, ph) = (page.width(), page.height());
    let x0 = (left_px.round() as u32).min(pw.saturating_sub(1));
    let y0 = (top_px.round() as u32).min(ph.saturating_sub(1));
    let cw = w_px.max(1).min(pw - x0);
    let ch = h_px.max(1).min(ph - y0);
    let crop = image::imageops::crop_imm(&page, x0, y0, cw, ch).to_image();

    let xform = RegionTransform {
        region_x_pt: x0 as f32 / px_per_pt,
        region_top_pt: page_height_pt - (y0 as f32 / px_per_pt),
        px_per_pt,
    };
    Ok((crop, xform))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_transform_round_trips_pdf_box() {
        // Region at PDF (100, 400) size 200x50, top edge = 450. At 150 DPI,
        // px_per_pt = 150/72 ≈ 2.0833.
        let xform = RegionTransform {
            region_x_pt: 100.0,
            region_top_pt: 450.0,
            px_per_pt: 150.0 / 72.0,
        };
        // A cell at the crop's top-left (0,0) of size = whole region in px.
        let w_px = 200.0 * xform.px_per_pt;
        let h_px = 50.0 * xform.px_per_pt;
        let b = xform.px_to_pdf(0.0, 0.0, w_px, h_px);
        assert!((b.x - 100.0).abs() < 1e-3, "x={}", b.x);
        assert!((b.y - 400.0).abs() < 1e-3, "y={}", b.y); // bottom edge
        assert!((b.width - 200.0).abs() < 1e-3);
        assert!((b.height - 50.0).abs() < 1e-3);
    }

    #[test]
    fn region_transform_maps_inset_cell() {
        let xform = RegionTransform {
            region_x_pt: 0.0,
            region_top_pt: 100.0,
            px_per_pt: 2.0,
        };
        // Cell 10px right, 20px down, 40x10 px → PDF x=5, top=100-10=90, h=5,
        // bottom y=85, width=20.
        let b = xform.px_to_pdf(10.0, 20.0, 40.0, 10.0);
        assert!((b.x - 5.0).abs() < 1e-6);
        assert!((b.y - 85.0).abs() < 1e-6);
        assert!((b.width - 20.0).abs() < 1e-6);
        assert!((b.height - 5.0).abs() < 1e-6);
    }
}
