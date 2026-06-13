//! Per-page triage / routing (PRD §4.A): classify each page as born-digital,
//! scanned, or hybrid so the pipeline only OCRs pages that actually need it.
//!
//! The decision uses cheap signals available from the parsed document — text
//! coverage and image area — mirroring what `firecrawl/pdf-inspector` (MIT) does
//! for pure-Rust triage. The hard invariant the pipeline relies on: a page with
//! a real text layer is never routed to OCR.

use crate::geometry::area;
use dongler_core::ir::{Block, Page, Route};

/// A page with at least this many text-layer characters has a usable text layer.
const TEXT_STRONG: usize = 50;
/// Below this, the page is treated as effectively text-less.
const TEXT_SPARSE: usize = 10;
/// An image covering at least this fraction of the page is "dominant".
const IMAGE_DOMINANT: f32 = 0.5;

/// Classify from raw signals. Exposed for direct testing.
///
/// * `text_chars` — total characters recovered from the text layer on the page.
/// * `image_ratio` — fraction of the page area covered by images (0.0–1.0).
pub fn classify(text_chars: usize, image_ratio: f32) -> Route {
    if text_chars >= TEXT_STRONG {
        // Strong text layer. If a big image also dominates, it's a hybrid page
        // (e.g. a scanned figure embedded in a born-digital report).
        if image_ratio >= IMAGE_DOMINANT && text_chars < TEXT_STRONG * 8 {
            Route::Hybrid
        } else {
            Route::BornDigital
        }
    } else if text_chars >= TEXT_SPARSE {
        // Some text plus a dominant image → hybrid; otherwise sparse born-digital.
        if image_ratio >= IMAGE_DOMINANT {
            Route::Hybrid
        } else {
            Route::BornDigital
        }
    } else {
        // Effectively no text. If an image dominates, it must be OCR'd.
        if image_ratio >= IMAGE_DOMINANT {
            Route::Scanned
        } else {
            // Empty / near-empty page with nothing to OCR.
            Route::BornDigital
        }
    }
}

/// Classify a parsed page by computing its text and image signals.
pub fn classify_page(page: &Page) -> Route {
    let text_chars: usize = page
        .blocks
        .iter()
        .map(|block| match block {
            Block::Text(t) => t.text.chars().filter(|c| !c.is_whitespace()).count(),
            Block::Table(t) => t
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.chars().filter(|c| !c.is_whitespace()).count())
                .sum(),
            Block::Figure(_) => 0,
        })
        .sum();

    let page_area = match (page.width, page.height) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => w * h,
        _ => 0.0,
    };
    let image_ratio = if page_area > 0.0 {
        let covered: f32 = page.images.iter().filter_map(|im| im.bbox.map(|b| area(&b))).sum();
        (covered / page_area).clamp(0.0, 1.0)
    } else {
        0.0
    };

    classify(text_chars, image_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_text_no_image_is_born_digital() {
        assert_eq!(classify(800, 0.0), Route::BornDigital);
    }

    #[test]
    fn no_text_full_image_is_scanned() {
        assert_eq!(classify(0, 1.0), Route::Scanned);
    }

    #[test]
    fn some_text_full_image_is_hybrid() {
        assert_eq!(classify(120, 0.95), Route::Hybrid);
    }

    #[test]
    fn empty_page_defaults_to_born_digital() {
        assert_eq!(classify(0, 0.0), Route::BornDigital);
    }

    #[test]
    fn sparse_text_no_image_is_born_digital() {
        assert_eq!(classify(20, 0.1), Route::BornDigital);
    }
}
