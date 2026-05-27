pub mod engine;
pub mod error;
pub mod format;
pub mod ir;
pub mod render;
pub mod source;

pub use engine::{ExtractionEngine, PlainTextEngine};
pub use error::{DonglerError, Result};
pub use format::{ExtractionStatus, InputFormat};
pub use ir::{Block, Document, Metadata, Page, TableBlock, TextBlock};
pub use render::{JsonRenderer, LatexRenderer, MarkdownRenderer, Renderer};
pub use source::{Source, SourceLoader, TextSourceLoader};

pub fn parse_text(text: &str) -> Result<Document> {
    PlainTextEngine::default().extract(&Source::from_text(text))
}

pub fn to_markdown(text: &str) -> Result<String> {
    let document = parse_text(text)?;
    MarkdownRenderer.render(&document)
}

pub fn to_json(text: &str) -> Result<String> {
    let document = parse_text(text)?;
    JsonRenderer.render(&document)
}

pub fn to_latex(text: &str) -> Result<String> {
    let document = parse_text(text)?;
    LatexRenderer.render(&document)
}

pub fn detect_format(path: &str) -> Result<String> {
    Ok(InputFormat::detect_path(path)?.as_str().to_owned())
}
