use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use dongler_core::{
    detect_format, load_many, load_path, load_path_with_options, parse_text, to_json, to_latex,
    to_markdown, Block, ExtractOptions, ExtractionEngine, ExtractionStatus, InputFormat,
    JsonRenderer, MarkdownRenderer, PlainTextEngine, Renderer, Source,
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
        ExtractionStatus::Supported
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
fn load_many_returns_per_file_successes_and_errors() {
    let text_path = write_temp_file("batch-notes.txt", "Batch document");
    let pdf_path = write_temp_bytes("batch-invoice.pdf", minimal_text_pdf("Batch PDF"));

    let results = load_many([text_path.clone(), pdf_path.clone()]);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].path, text_path.display().to_string());
    assert!(results[0].ok);
    assert!(results[0].document.is_some());
    assert!(results[0].error.is_none());

    assert_eq!(results[1].path, pdf_path.display().to_string());
    assert!(results[1].ok);
    assert!(results[1].document.is_some());
    assert!(results[1].error.is_none());
}

#[test]
fn load_path_extracts_pdf_text_with_page_geometry_and_source_anchors() {
    let path = write_temp_bytes("paper.pdf", minimal_text_pdf("Hello PDF"));

    let document = load_path(&path).unwrap();

    assert_eq!(document.schema_version, "dongler.ir.v1");
    assert_eq!(document.metadata.format, "pdf");
    assert_eq!(document.metadata.engine, "pdf-native");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.pages[0].width, Some(612.0));
    assert_eq!(document.pages[0].height, Some(792.0));

    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.text, "Hello PDF");
            assert_eq!(block.kind, "paragraph");
            assert!(block.bbox.is_some());
            assert_eq!(block.source_anchors[0].page_number, 1);
            assert!(block.source_anchors[0].bbox.is_some());
            assert_eq!(block.source_anchors[0].extraction_method, "native_pdf");
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_records_pdf_image_xobject_positions() {
    let path = write_temp_bytes("image.pdf", image_pdf());

    let document = load_path(&path).unwrap();
    let page = &document.pages[0];

    assert_eq!(page.images.len(), 1);
    assert_eq!(page.assets.len(), 1);
    assert_eq!(page.images[0].bbox.as_ref().unwrap().x, 200.0);
    assert_eq!(page.images[0].bbox.as_ref().unwrap().y, 300.0);
    assert_eq!(page.images[0].bbox.as_ref().unwrap().width, 100.0);
    assert_eq!(page.images[0].bbox.as_ref().unwrap().height, 50.0);
}

#[test]
fn load_path_extracts_positioned_pdf_rows_as_table_blocks() {
    let path = write_temp_bytes("table.pdf", table_pdf());

    let document = load_path(&path).unwrap();

    let table = document.pages[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected a table block");

    assert_eq!(table.headers, vec!["Name", "Value"]);
    assert_eq!(table.rows, vec![vec!["Alpha".to_owned(), "42".to_owned()]]);
    assert!(table.bbox.is_some());
    assert_eq!(table.source_anchors[0].page_number, 1);
}

#[test]
fn load_path_with_options_can_omit_geometry_and_assets() {
    let path = write_temp_bytes("image-options.pdf", image_pdf());

    let document = load_path_with_options(
        &path,
        ExtractOptions {
            include_geometry: false,
            include_assets: false,
            ..ExtractOptions::default()
        },
    )
    .unwrap();

    assert_eq!(document.pages[0].width, None);
    assert!(document.pages[0].images.is_empty());
    assert!(document.pages[0].assets.is_empty());
}

fn write_temp_file(name: &str, contents: &str) -> PathBuf {
    write_temp_bytes(name, contents.as_bytes().to_vec())
}

fn write_temp_bytes(name: &str, contents: Vec<u8>) -> PathBuf {
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

fn minimal_text_pdf(text: &str) -> Vec<u8> {
    pdf_fixture(&format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    ), &format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET"), "")
}

fn table_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Name) Tj 150 0 Td (Value) Tj -150 -20 Td (Alpha) Tj 150 0 Td (42) Tj ET",
        "",
    )
}

fn image_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im1 6 0 R >> >> /Contents 5 0 R >>",
        "q 100 0 0 50 200 300 cm /Im1 Do Q",
        "6 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 6 >>\nstream\nABCDEF\nendstream\nendobj\n",
    )
}

fn pdf_fixture(page_object: &str, content_stream: &str, extra_objects: &str) -> Vec<u8> {
    let mut pdf = format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n{page_object}\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n{}",
        content_stream.len(),
        content_stream,
        extra_objects
    )
    .into_bytes();
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    pdf
}
