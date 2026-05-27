use dongler_core::{
    detect_format, parse_text, to_json, to_latex, to_markdown, Block, ExtractionEngine,
    ExtractionStatus, InputFormat, JsonRenderer, MarkdownRenderer, PlainTextEngine, Renderer,
    Source,
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
