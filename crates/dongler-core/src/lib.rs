pub mod engine;
pub mod error;
pub mod format;
pub mod ir;
pub mod pdf;
pub mod render;
pub mod source;

use std::path::Path;

pub use engine::{ExtractionEngine, PlainTextEngine};
pub use error::{DonglerError, Result};
pub use format::{ExtractionStatus, InputFormat};
pub use ir::{
    Asset, BBox, BatchResult, Block, Confidence, Document, ExtractOptions, FigureBlock,
    ImageObject, Line, Metadata, Page, SourceAnchor, Span, TableBlock, TableCell, TextBlock,
    Warning,
};
pub use pdf::PdfEngine;
pub use render::{JsonRenderer, LatexRenderer, MarkdownRenderer, Renderer};
pub use source::{PdfSourceLoader, Source, SourceLoader, TextSourceLoader};

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
    PlainTextEngine.extract(&Source::from_text(text))
}

pub fn load_path(path: impl AsRef<Path>) -> Result<Document> {
    load_path_with_options(path, ExtractOptions::default())
}

pub fn load_path_with_options(path: impl AsRef<Path>, options: ExtractOptions) -> Result<Document> {
    let path = path.as_ref();
    let format = InputFormat::detect_path(path)?;

    let mut document = match format {
        InputFormat::Text => {
            let source = TextSourceLoader.load(path)?;
            PlainTextEngine.extract(&source)
        }
        InputFormat::Pdf => {
            let source = PdfSourceLoader.load(path)?;
            PdfEngine.extract(&source)
        }
        _ => Err(DonglerError::planned_format(format.as_str())),
    }?;

    apply_extract_options(&mut document, &options);
    Ok(document)
}

fn apply_extract_options(document: &mut Document, options: &ExtractOptions) {
    if !options.include_geometry {
        for page in &mut document.pages {
            page.bbox = None;
            page.width = None;
            page.height = None;
            for block in &mut page.blocks {
                match block {
                    Block::Text(text) => {
                        text.bbox = None;
                        text.lines.clear();
                        for anchor in &mut text.source_anchors {
                            anchor.bbox = None;
                        }
                    }
                    Block::Table(table) => {
                        table.bbox = None;
                        for cell in &mut table.cells {
                            cell.bbox = None;
                        }
                        for anchor in &mut table.source_anchors {
                            anchor.bbox = None;
                        }
                    }
                    Block::Figure(figure) => {
                        figure.bbox = None;
                        for anchor in &mut figure.source_anchors {
                            anchor.bbox = None;
                        }
                    }
                }
            }
            for image in &mut page.images {
                image.bbox = None;
            }
            for asset in &mut page.assets {
                asset.bbox = None;
            }
        }
    }

    if !options.include_assets {
        document.assets.clear();
        for page in &mut document.pages {
            page.assets.clear();
            page.images.clear();
        }
    }
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
