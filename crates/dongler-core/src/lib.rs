pub mod engine;
pub mod error;
pub mod format;
pub mod ir;
pub mod render;
pub mod source;

use std::path::Path;

pub use engine::{ExtractionEngine, PlainTextEngine};
pub use error::{DonglerError, Result};
pub use format::{ExtractionStatus, InputFormat};
pub use ir::{BatchResult, Block, Document, Metadata, Page, TableBlock, TextBlock};
pub use render::{JsonRenderer, LatexRenderer, MarkdownRenderer, Renderer};
pub use source::{Source, SourceLoader, TextSourceLoader};

impl Document {
    pub fn to_markdown(&self) -> Result<String> {
        MarkdownRenderer.render(self)
    }

    pub fn to_json(&self) -> Result<String> {
        JsonRenderer.render(self)
    }

    pub fn to_latex(&self) -> Result<String> {
        LatexRenderer.render(self)
    }
}

pub fn parse_text(text: &str) -> Result<Document> {
    PlainTextEngine::default().extract(&Source::from_text(text))
}

pub fn load_path(path: impl AsRef<Path>) -> Result<Document> {
    let path = path.as_ref();
    let format = InputFormat::detect_path(path)?;

    if format.extraction_status() != ExtractionStatus::Supported {
        return Err(DonglerError::planned_format(format.as_str()));
    }

    let source = TextSourceLoader.load(path)?;
    PlainTextEngine::default().extract(&source)
}

pub fn load_many<I, P>(paths: I) -> Vec<BatchResult>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    paths
        .into_iter()
        .map(|path| {
            let path = path.as_ref();
            let path_string = path.display().to_string();

            match load_path(path) {
                Ok(document) => BatchResult {
                    path: path_string,
                    ok: true,
                    document: Some(document),
                    error: None,
                },
                Err(error) => BatchResult {
                    path: path_string,
                    ok: false,
                    document: None,
                    error: Some(error.to_string()),
                },
            }
        })
        .collect()
}

pub fn to_markdown(text: &str) -> Result<String> {
    let document = parse_text(text)?;
    document.to_markdown()
}

pub fn to_json(text: &str) -> Result<String> {
    let document = parse_text(text)?;
    document.to_json()
}

pub fn to_latex(text: &str) -> Result<String> {
    let document = parse_text(text)?;
    document.to_latex()
}

pub fn detect_format(path: &str) -> Result<String> {
    Ok(InputFormat::detect_path(path)?.as_str().to_owned())
}
