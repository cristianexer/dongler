//! PDF page rasterization via pdfium (PRD §4.B). Renders a page to an RGB image
//! for the layout/OCR/table models. The pdfium dynamic library is resolved at
//! runtime (bundled next to the binary, or the system library); no pdfium binary
//! is needed to *build*. Behind the `ml` feature.

use crate::ml::MlError;
use image::RgbImage;
use pdfium_render::prelude::*;

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
