pub mod archive;
pub mod csv;
pub mod engine;
pub mod error;
pub mod format;
pub mod image;
pub mod ir;
pub mod json;
pub mod openxml;
pub mod pdf;
pub mod render;
pub mod source;
pub mod textual;

use std::collections::{HashMap, HashSet};
use std::path::Path;

pub use archive::ArchiveEngine;
pub use csv::CsvEngine;
pub use engine::{ExtractionEngine, PlainTextEngine};
pub use error::{DonglerError, Result};
pub use format::{ExtractionStatus, InputFormat};
pub use image::ImageEngine;
pub use ir::{
    Asset, BBox, BatchResult, Block, Confidence, Document, ExtractOptions, FigureBlock,
    ImageObject, Line, Metadata, Page, SourceAnchor, Span, TableBlock, TableCell, TextBlock,
    Warning,
};
pub use json::JsonEngine;
pub use openxml::OpenXmlEngine;
pub use pdf::PdfEngine;
pub use render::{JsonRenderer, LatexRenderer, MarkdownRenderer, Renderer};
pub use source::{
    FormatSourceLoader, ImageSourceLoader, PdfSourceLoader, Source, SourceLoader, TextSourceLoader,
};
pub use textual::{EmailEngine, HtmlEngine, XmlEngine};

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
        InputFormat::Image => {
            let source = ImageSourceLoader.load(path)?;
            ImageEngine.extract(&source)
        }
        InputFormat::Archive => {
            let source = FormatSourceLoader::new(format).load(path)?;
            ArchiveEngine.extract(&source)
        }
        InputFormat::Word
        | InputFormat::Excel
        | InputFormat::Presentation
        | InputFormat::OpenDocument => {
            let source = FormatSourceLoader::new(format).load(path)?;
            OpenXmlEngine.extract(&source)
        }
        InputFormat::Html => {
            let source = FormatSourceLoader::new(format).load(path)?;
            HtmlEngine.extract(&source)
        }
        InputFormat::Email => {
            let source = FormatSourceLoader::new(format).load(path)?;
            EmailEngine.extract(&source)
        }
        InputFormat::Xml => {
            let source = FormatSourceLoader::new(format).load(path)?;
            XmlEngine.extract(&source)
        }
        InputFormat::Json => {
            let source = FormatSourceLoader::new(format).load(path)?;
            JsonEngine.extract(&source)
        }
        InputFormat::Csv => {
            let source = FormatSourceLoader::new(format).load(path)?;
            CsvEngine.extract(&source)
        }
        InputFormat::LegacyWord
        | InputFormat::LegacyExcel
        | InputFormat::LegacyPresentation
        | InputFormat::LegacyEmail => Err(DonglerError::planned_format(format.as_str())),
    }?;

    apply_extract_options(&mut document, &options);
    Ok(document)
}

fn apply_extract_options(document: &mut Document, options: &ExtractOptions) {
    if options.suppress_headers_footers {
        suppress_repeated_headers_footers(document);
    }

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

fn suppress_repeated_headers_footers(document: &mut Document) {
    if document.pages.len() < 2 {
        return;
    }

    let mut occurrences = HashMap::new();
    for page in &document.pages {
        let mut seen_on_page = HashSet::new();
        for block in &page.blocks {
            if let Some(key) = header_footer_key(page.height, block) {
                seen_on_page.insert(key);
            }
        }
        for key in seen_on_page {
            *occurrences.entry(key).or_insert(0usize) += 1;
        }
    }

    let minimum_pages = 2.max((document.pages.len() + 1) / 2);
    let repeated = occurrences
        .into_iter()
        .filter_map(|(key, count)| (count >= minimum_pages).then_some(key))
        .collect::<HashSet<_>>();
    if repeated.is_empty() {
        return;
    }

    for page in &mut document.pages {
        let page_height = page.height;
        page.blocks.retain(|block| {
            header_footer_key(page_height, block)
                .map(|key| !repeated.contains(&key))
                .unwrap_or(true)
        });
    }
    refresh_document_counts(document);
}

fn header_footer_key(page_height: Option<f32>, block: &Block) -> Option<String> {
    let height = page_height?;
    if height <= 0.0 {
        return None;
    }

    let bbox = block_bbox(block)?;
    let center_y = bbox.y + bbox.height / 2.0;
    let margin = (height * 0.12).max(48.0);
    let band = if center_y >= height - margin {
        "top"
    } else if center_y <= margin {
        "bottom"
    } else {
        return None;
    };

    let text = normalize_repeated_margin_text(&block_text(block));
    (!text.is_empty()).then(|| format!("{band}:{text}"))
}

fn block_bbox(block: &Block) -> Option<BBox> {
    match block {
        Block::Text(text) => text.bbox,
        Block::Table(table) => table.bbox,
        Block::Figure(figure) => figure.bbox,
    }
}

fn normalize_repeated_margin_text(text: &str) -> String {
    let mut output = String::new();
    let mut last_was_space = true;
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_digit() {
            if !output.ends_with('#') {
                output.push('#');
            }
            last_was_space = false;
        } else if character.is_whitespace() {
            if !last_was_space {
                output.push(' ');
                last_was_space = true;
            }
        } else {
            output.push(character);
            last_was_space = false;
        }
    }
    output.trim().to_owned()
}

fn refresh_document_counts(document: &mut Document) {
    let mut character_count = 0;
    let mut word_count = 0;
    let mut block_count = 0;

    for page in &document.pages {
        for block in &page.blocks {
            let text = block_text(block);
            character_count += text.chars().count();
            word_count += text.split_whitespace().count();
            block_count += 1;
        }
    }

    document.metadata.character_count = character_count;
    document.metadata.word_count = word_count;
    document.metadata.block_count = block_count;
}

fn block_text(block: &Block) -> String {
    match block {
        Block::Text(text) => text.text.clone(),
        Block::Table(table) => {
            let mut rows = Vec::new();
            if !table.headers.is_empty() {
                rows.push(table.headers.join(" "));
            }
            rows.extend(table.rows.iter().map(|row| row.join(" ")));
            rows.join("\n")
        }
        Block::Figure(figure) => figure.caption.clone().unwrap_or_default(),
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
