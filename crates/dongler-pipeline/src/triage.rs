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

/// An alphabetic token longer than this is almost never a real word — it is a
/// run of words glued together because the producer omitted space characters and
/// gap-based segmentation under-spaced them.
const LONG_TOKEN_CHARS: usize = 18;
/// Need at least this many alphabetic tokens before the long-token ratio is a
/// trustworthy health signal (short pages are too noisy to judge).
const HEALTH_MIN_TOKENS: usize = 25;
/// Above this fraction of glued (over-long) tokens, the text layer is considered
/// garbled — the words are present but un-segmentable, so the page is better read
/// by OCR than trusted verbatim. Clean prose sits near zero; the threshold leaves
/// generous headroom for the occasional URL or chemical name.
const GARBLED_LONG_RATIO: f32 = 0.04;

/// Health of a page's text layer in `[0.0, 1.0]` (1.0 = clean). Driven by the
/// fraction of alphabetic tokens that are implausibly long, the tell-tale of
/// space characters dropped by the producer and not recovered by gap-based
/// segmentation. Pages with too few tokens to judge return 1.0 (trust them).
pub fn text_layer_health(text: &str) -> f32 {
    1.0 - (glued_token_ratio(text) / GARBLED_LONG_RATIO).clamp(0.0, 1.0)
}

/// Fraction of alphabetic tokens that are over-long (glued words).
fn glued_token_ratio(text: &str) -> f32 {
    let tokens = text
        .split_whitespace()
        .filter(|t| t.chars().any(|c| c.is_alphabetic()));
    let mut total = 0usize;
    let mut long = 0usize;
    for token in tokens {
        total += 1;
        if token.chars().count() > LONG_TOKEN_CHARS {
            long += 1;
        }
    }
    if total < HEALTH_MIN_TOKENS {
        return 0.0;
    }
    long as f32 / total as f32
}

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

    // A strong-but-garbled text layer (words present, spacing lost) is read more
    // faithfully by OCR than trusted verbatim — route it to OCR rather than
    // emit glued text. Healthy pages keep the fast deterministic path.
    let route = classify(text_chars, image_ratio);
    if route == Route::BornDigital && text_chars >= TEXT_STRONG {
        let page_text: String = page
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        if text_layer_health(&page_text) < 0.5 {
            return Route::Scanned;
        }
    }
    route
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

    #[test]
    fn clean_prose_is_healthy() {
        let prose = "The Company reports its financial performance using three \
            segments and the impracticability of determining the geographic source \
            of the revenue is disclosed in the notes to the financial statements"
            .repeat(2);
        assert!(text_layer_health(&prose) > 0.9, "clean prose should be healthy");
    }

    #[test]
    fn glued_text_is_unhealthy() {
        // Producer dropped spaces: many tokens are whole phrases glued together.
        let glued = "ofthese significantproductandserviceofferings \
            MicrosoftCloudrevenuewhichincludesAzure ourIntelligentCloudsegmentconsists \
            developersThissegmentprimarilycomprises licensedonpremisesandotherservices \
            commercialcloudpropertieswas geographicsourceoftherevenue"
            .repeat(4);
        assert!(
            text_layer_health(&glued) < 0.5,
            "glued text should be flagged unhealthy, got {}",
            text_layer_health(&glued)
        );
    }

    #[test]
    fn short_text_is_trusted() {
        // Too few tokens to judge — never flag.
        assert_eq!(text_layer_health("a b c reallyreallylongtoken"), 1.0);
    }
}
