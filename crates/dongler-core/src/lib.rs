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
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
    if format.extraction_status() == ExtractionStatus::Planned {
        return Err(DonglerError::planned_format(format.as_str()));
    }

    let source = load_source(format, path)?;
    let mut document = engine_extract(format, &source)?;

    if ocr_fallback_enabled() {
        apply_ocr_fallback(&mut document);
    }
    apply_extract_options(&mut document, &options);
    Ok(document)
}

/// Read a source from disk using the loader appropriate for `format`.
fn load_source(format: InputFormat, path: &Path) -> Result<Source> {
    match format {
        InputFormat::Text => TextSourceLoader.load(path),
        InputFormat::Pdf => PdfSourceLoader.load(path),
        InputFormat::Image => ImageSourceLoader.load(path),
        _ => FormatSourceLoader::new(format).load(path),
    }
}

/// Dispatch an in-memory source to the engine for `format`.
///
/// This is the filesystem-free heart of extraction, shared by the path-based
/// loaders and the byte-based [`extract_bytes`] entry point used by wasm.
fn engine_extract(format: InputFormat, source: &Source) -> Result<Document> {
    match format {
        InputFormat::Text => PlainTextEngine.extract(source),
        InputFormat::Pdf => PdfEngine.extract(source),
        InputFormat::Image => ImageEngine.extract(source),
        InputFormat::Archive => ArchiveEngine.extract(source),
        InputFormat::Word
        | InputFormat::Excel
        | InputFormat::Presentation
        | InputFormat::OpenDocument => OpenXmlEngine.extract(source),
        InputFormat::Html => HtmlEngine.extract(source),
        InputFormat::Email => EmailEngine.extract(source),
        InputFormat::Xml => XmlEngine.extract(source),
        InputFormat::Json => JsonEngine.extract(source),
        InputFormat::Csv => CsvEngine.extract(source),
        InputFormat::LegacyWord
        | InputFormat::LegacyExcel
        | InputFormat::LegacyPresentation
        | InputFormat::LegacyEmail => Err(DonglerError::planned_format(format.as_str())),
    }
}

/// Extract a document from in-memory bytes, detecting the format from
/// `filename` (its extension only — the file is never read from disk).
///
/// This is the primary entry point for environments without a filesystem,
/// such as WebAssembly. OCR fallback is intentionally not applied here because
/// it relies on spawning external processes.
pub fn extract_bytes(bytes: &[u8], filename: &str) -> Result<Document> {
    extract_bytes_with_options(bytes, filename, ExtractOptions::default())
}

pub fn extract_bytes_with_options(
    bytes: &[u8],
    filename: &str,
    options: ExtractOptions,
) -> Result<Document> {
    let format = InputFormat::detect_path(filename)?;
    if format.extraction_status() == ExtractionStatus::Planned {
        return Err(DonglerError::planned_format(format.as_str()));
    }

    let source = Source::from_bytes_for_format(bytes, filename, format)?;
    let mut document = engine_extract(format, &source)?;
    apply_extract_options(&mut document, &options);
    Ok(document)
}

#[derive(Debug, Clone)]
struct OcrFallbackConfig {
    renderer: String,
    engine: String,
    temp_dir: PathBuf,
}

fn ocr_fallback_enabled() -> bool {
    matches!(
        std::env::var("DONGLER_OCR_FALLBACK")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn apply_ocr_fallback(document: &mut Document) {
    if document.metadata.format != "pdf" {
        return;
    }
    let Some(source_path) = document.metadata.source.as_deref().map(PathBuf::from) else {
        return;
    };
    if !source_path.exists() {
        return;
    }
    let config = ocr_fallback_config();
    let mut changed = false;

    for page in &mut document.pages {
        if !page_needs_ocr_fallback(page) {
            continue;
        }

        match ocr_pdf_page(&source_path, page.number, &config) {
            Ok(Some(text)) => {
                insert_ocr_text_block(page, text);
                changed = true;
            }
            Ok(None) => {}
            Err(message) => page.warnings.push(Warning {
                code: "ocr.fallback".to_owned(),
                severity: "warning".to_owned(),
                message,
                source_anchor: Some(SourceAnchor {
                    page_number: page.number,
                    pdf_object_ids: Vec::new(),
                    bbox: page.bbox,
                    extraction_method: "ocr_fallback".to_owned(),
                }),
            }),
        }
    }

    if changed {
        refresh_document_counts(document);
    }
}

fn ocr_fallback_config() -> OcrFallbackConfig {
    OcrFallbackConfig {
        renderer: std::env::var("DONGLER_PDF_RENDERER").unwrap_or_else(|_| "pdftoppm".to_owned()),
        engine: std::env::var("DONGLER_OCR_ENGINE").unwrap_or_else(|_| "tesseract".to_owned()),
        temp_dir: std::env::var("DONGLER_OCR_TEMP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .unwrap_or_else(|_| std::env::temp_dir())
                    .join("target")
                    .join("dongler-ocr")
            }),
    }
}

fn page_needs_ocr_fallback(page: &Page) -> bool {
    !page.images.is_empty()
        && !page.blocks.iter().any(|block| match block {
            Block::Text(text) => !text.text.trim().is_empty(),
            Block::Table(table) => {
                table.headers.iter().any(|value| !value.trim().is_empty())
                    || table
                        .rows
                        .iter()
                        .flatten()
                        .any(|value| !value.trim().is_empty())
            }
            Block::Figure(_) => false,
        })
}

fn ocr_pdf_page(
    source_path: &Path,
    page_number: usize,
    config: &OcrFallbackConfig,
) -> std::result::Result<Option<String>, String> {
    fs::create_dir_all(&config.temp_dir).map_err(|error| {
        format!(
            "could not create OCR temp dir {}: {error}",
            config.temp_dir.display()
        )
    })?;
    let prefix = config.temp_dir.join(format!(
        "page-{}-{}-{}",
        std::process::id(),
        page_number,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let image_path = prefix.with_extension("png");
    let page = page_number.to_string();
    let render_output = Command::new(&config.renderer)
        .args([
            "-f",
            page.as_str(),
            "-l",
            page.as_str(),
            "-r",
            "200",
            "-png",
            "-singlefile",
        ])
        .arg(source_path)
        .arg(&prefix)
        .output()
        .map_err(|error| format!("could not run PDF renderer {}: {error}", config.renderer))?;

    if !render_output.status.success() {
        let stderr = String::from_utf8_lossy(&render_output.stderr);
        return Err(format!(
            "PDF renderer {} failed: {}",
            config.renderer,
            stderr.trim()
        ));
    }

    let ocr_output = Command::new(&config.engine)
        .arg(&image_path)
        .arg("stdout")
        .args(["--psm", "6"])
        .output()
        .map_err(|error| format!("could not run OCR engine {}: {error}", config.engine));
    let _ = fs::remove_file(&image_path);

    let ocr_output = ocr_output?;
    if !ocr_output.status.success() {
        let stderr = String::from_utf8_lossy(&ocr_output.stderr);
        return Err(format!(
            "OCR engine {} failed: {}",
            config.engine,
            stderr.trim()
        ));
    }

    let text = normalize_ocr_text(&String::from_utf8_lossy(&ocr_output.stdout));
    Ok((!text.is_empty()).then_some(text))
}

fn normalize_ocr_text(text: &str) -> String {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn insert_ocr_text_block(page: &mut Page, text: String) {
    let bbox = page.bbox;
    page.blocks.insert(
        0,
        Block::Text(TextBlock {
            text: text.clone(),
            kind: "ocr_text".to_owned(),
            bbox,
            lines: vec![Line {
                text: text.clone(),
                bbox,
                spans: vec![Span {
                    text,
                    bbox,
                    font: None,
                    size: None,
                    bold: false,
                    italic: false,
                }],
            }],
            source_anchors: vec![SourceAnchor {
                page_number: page.number,
                pdf_object_ids: Vec::new(),
                bbox,
                extraction_method: "ocr_fallback".to_owned(),
            }],
            confidence: Some(Confidence {
                score: 0.55,
                calibrated: false,
            }),
        }),
    );
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
