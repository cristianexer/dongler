//! The born-digital text source abstraction (PRD §4.C).
//!
//! A `TextProvider` turns PDF bytes into a parsed [`Document`] using only the
//! embedded text layer — no OCR. The default is [`DonglerCoreProvider`] (the
//! salvaged Rust parser); a pdfium-backed provider is the planned fallback and
//! the E0 head-to-head comparison target. Keeping this behind a trait is what
//! lets the parser "earn its place with data" per the PRD.

use dongler_core::{extract_bytes_with_options, Document, ExtractOptions, Result};

/// Extracts born-digital text into the IR.
pub trait TextProvider: Send + Sync {
    /// Stable identifier used in provenance and eval ledgers.
    fn name(&self) -> &'static str;

    /// Parse `bytes` (a document whose type is hinted by `filename`) into a
    /// [`Document`] using only the text layer.
    fn extract(&self, bytes: &[u8], filename: &str, options: &ExtractOptions)
        -> Result<Document>;
}

/// The default provider, backed by `dongler-core`'s custom parser.
#[derive(Debug, Default, Clone, Copy)]
pub struct DonglerCoreProvider;

impl TextProvider for DonglerCoreProvider {
    fn name(&self) -> &'static str {
        "dongler-core"
    }

    fn extract(
        &self,
        bytes: &[u8],
        filename: &str,
        options: &ExtractOptions,
    ) -> Result<Document> {
        extract_bytes_with_options(bytes, filename, options.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal single-line PDF (same structure as the core test fixtures).
    fn minimal_pdf(text: &str) -> Vec<u8> {
        let content = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
              /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
        );
        pdf.extend_from_slice(
            b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        );
        pdf.extend_from_slice(
            format!(
                "5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
                content.len(),
                content
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
        pdf
    }

    #[test]
    fn core_provider_extracts_text() {
        let provider = DonglerCoreProvider;
        assert_eq!(provider.name(), "dongler-core");
        let doc = provider
            .extract(&minimal_pdf("Hello Provider"), "x.pdf", &ExtractOptions::default())
            .expect("extract");
        let text: String = doc
            .pages
            .iter()
            .flat_map(|p| p.blocks.iter())
            .filter_map(|b| match b {
                dongler_core::ir::Block::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Hello Provider"), "got: {text:?}");
    }
}
