use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use dongler_core::{
    detect_format, load_many, load_path, parse_text, to_json, to_latex, to_markdown, Block,
    DonglerError, ExtractionEngine, ExtractionStatus, InputFormat, JsonRenderer, MarkdownRenderer,
    PlainTextEngine, Renderer, Source,
};

#[test]
fn parse_text_creates_document_ir() {
    let document = parse_text("Hello from Dongler\n\nSecond paragraph").unwrap();

    assert_eq!(document.metadata.format, "text");
    assert_eq!(document.metadata.engine, "plain-text");
    assert_eq!(document.metadata.block_count, 2);
    assert_eq!(document.metadata.word_count, 5);
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.pages[0].number, 1);
}

#[test]
fn plain_text_engine_splits_paragraphs_into_blocks() {
    let engine = PlainTextEngine::default();
    let document = engine
        .extract(&Source::from_text("First paragraph\nstill first\n\nSecond"))
        .unwrap();

    assert_eq!(document.pages[0].blocks.len(), 2);
    match &document.pages[0].blocks[0] {
        Block::Text(block) => assert_eq!(block.text, "First paragraph still first"),
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn markdown_renderer_outputs_paragraph_markdown() {
    let document = parse_text("Hello\n\nfrom Dongler").unwrap();
    let markdown = MarkdownRenderer.render(&document).unwrap();

    assert_eq!(markdown, "Hello\n\nfrom Dongler");
    assert_eq!(
        to_markdown("Hello from Dongler").unwrap(),
        "Hello from Dongler"
    );
}

#[test]
fn json_renderer_outputs_document_json() {
    let document = parse_text("Hello from Dongler").unwrap();
    let rendered = JsonRenderer.render(&document).unwrap();
    let api_rendered = to_json("Hello from Dongler").unwrap();

    assert!(rendered.contains("\"format\": \"text\""));
    assert_eq!(rendered, api_rendered);
}

#[test]
fn latex_renderer_escapes_latex_sensitive_text() {
    let latex = to_latex("Revenue is 100% & cost is $5_000").unwrap();

    assert!(latex.contains("100\\% \\& cost is \\$5\\_000"));
    assert!(latex.contains("\\begin{document}"));
}

#[test]
fn detect_format_maps_known_extensions() {
    assert_eq!(detect_format("sample.txt").unwrap(), "text");
    assert_eq!(detect_format("sample.PDF").unwrap(), "pdf");
    assert_eq!(detect_format("book.xlsx").unwrap(), "excel");
    assert_eq!(detect_format("report.docx").unwrap(), "word");
    assert_eq!(detect_format("page.html").unwrap(), "html");
    assert_eq!(detect_format("scan.png").unwrap(), "image");
    assert_eq!(detect_format("message.eml").unwrap(), "email");
}

#[test]
fn input_format_tracks_current_extraction_support() {
    assert_eq!(
        InputFormat::detect_path("notes.txt")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("paper.pdf")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Planned
    );
}

#[test]
fn document_renders_itself_to_markdown_latex_and_json() {
    let document = parse_text("Revenue is 100% & cost is $5_000").unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        "Revenue is 100% & cost is $5_000"
    );
    assert!(document
        .to_latex()
        .unwrap()
        .contains("100\\% \\& cost is \\$5\\_000"));
    assert!(document.to_json().unwrap().contains("\"format\": \"text\""));
}

#[test]
fn load_path_extracts_supported_text_files_with_source_metadata() {
    let path = write_temp_file("notes.txt", "First paragraph\n\nSecond paragraph");

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "text");
    assert_eq!(
        document.metadata.source.as_deref(),
        Some(path.to_str().unwrap())
    );
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_returns_planned_format_error_for_pdf() {
    let path = write_temp_file("invoice.pdf", "%PDF planned fixture");

    let error = load_path(&path).unwrap_err();

    assert!(matches!(
        error,
        DonglerError::PlannedFormat { ref format } if format == "pdf"
    ));
}

#[test]
fn load_many_returns_per_file_successes_and_errors() {
    let text_path = write_temp_file("batch-notes.txt", "Batch document");
    let pdf_path = write_temp_file("batch-invoice.pdf", "%PDF planned fixture");

    let results = load_many([text_path.clone(), pdf_path.clone()]);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].path, text_path.display().to_string());
    assert!(results[0].ok);
    assert!(results[0].document.is_some());
    assert!(results[0].error.is_none());

    assert_eq!(results[1].path, pdf_path.display().to_string());
    assert!(!results[1].ok);
    assert!(results[1].document.is_none());
    assert!(results[1]
        .error
        .as_deref()
        .unwrap()
        .contains("pdf extraction"));
}

fn write_temp_file(name: &str, contents: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("dongler-test-{nonce}"));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}
