use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use dongler_core::{
    detect_format, load_many, load_path, load_path_with_options, parse_text, to_json, to_latex,
    to_markdown, Block, ExtractOptions, ExtractionEngine, ExtractionStatus, InputFormat,
    JsonRenderer, MarkdownRenderer, PlainTextEngine, Renderer, Source,
};
use flate2::{write::GzEncoder, Compression};

static OCR_ENV_LOCK: Mutex<()> = Mutex::new(());

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
fn markdown_renderer_drops_non_printable_control_characters() {
    let markdown = to_markdown("safe\0 text \u{8} ok\n\nnext").unwrap();

    assert_eq!(markdown, "safe text ok\n\nnext");
    assert!(!markdown.contains('\0'));
    assert!(!markdown.contains('\u{8}'));
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
fn latex_renderer_drops_non_printable_control_characters() {
    let latex = to_latex("classi\u{2}cation e\u{b}ect \u{96} A–B").unwrap();

    assert!(latex.contains("classication e ect  A-B"));
    assert!(!latex.contains('\u{2}'));
    assert!(!latex.contains('\u{b}'));
    assert!(!latex.contains('\u{96}'));
}

#[test]
fn detect_format_maps_known_extensions() {
    assert_eq!(detect_format("sample.txt").unwrap(), "text");
    assert_eq!(detect_format("README.md").unwrap(), "text");
    assert_eq!(detect_format("sample.PDF").unwrap(), "pdf");
    assert_eq!(detect_format("book.xlsx").unwrap(), "excel");
    assert_eq!(detect_format("report.docx").unwrap(), "word");
    assert_eq!(detect_format("deck.pptx").unwrap(), "presentation");
    assert_eq!(detect_format("notes.odt").unwrap(), "opendocument");
    assert_eq!(detect_format("sheet.ods").unwrap(), "opendocument");
    assert_eq!(detect_format("slides.odp").unwrap(), "opendocument");
    assert_eq!(detect_format("page.html").unwrap(), "html");
    assert_eq!(detect_format("scan.png").unwrap(), "image");
    assert_eq!(detect_format("message.eml").unwrap(), "email");
    assert_eq!(detect_format("annotations.json").unwrap(), "json");
    assert_eq!(detect_format("records.jsonl").unwrap(), "json");
    assert_eq!(detect_format("boxes.csv").unwrap(), "csv");
    assert_eq!(detect_format("rows.tsv").unwrap(), "csv");
    assert_eq!(detect_format("article.nxml").unwrap(), "xml");
    assert_eq!(detect_format("source.tex").unwrap(), "text");
    assert_eq!(detect_format("papers.jsonl.gz").unwrap(), "json");
    assert_eq!(detect_format("article.nxml.gz").unwrap(), "xml");
    assert_eq!(detect_format("source.tar").unwrap(), "archive");
    assert_eq!(detect_format("source.tar.gz").unwrap(), "archive");
    assert_eq!(detect_format("2401.00001.gz").unwrap(), "archive");
    assert_eq!(detect_format("source.tgz").unwrap(), "archive");
    assert_eq!(detect_format("dataset.zip").unwrap(), "archive");
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
    assert_eq!(
        InputFormat::detect_path("scan.png")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("report.docx")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("book.xlsx")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("deck.pptx")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("notes.odt")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("page.html")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("message.eml")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("annotations.json")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("boxes.csv")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("article.xml")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("papers.jsonl.gz")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("source.tar.gz")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("2401.00001.gz")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Supported
    );
    assert_eq!(
        InputFormat::detect_path("legacy.doc")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Planned
    );
    assert_eq!(
        InputFormat::detect_path("legacy.xls")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Planned
    );
    assert_eq!(
        InputFormat::detect_path("legacy.ppt")
            .unwrap()
            .extraction_status(),
        ExtractionStatus::Planned
    );
    assert_eq!(
        InputFormat::detect_path("message.msg")
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
fn load_path_extracts_markdown_headings_and_tables() {
    let path = write_temp_file(
        "readoc.md",
        "# Benchmark Report\n\nIntro paragraph.\n\n| Name | Score |\n| --- | ---: |\n| Alpha | 42 |\n\n## Details\n- first\n- second\n",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "text");
    assert_eq!(document.metadata.engine, "markdown-native");
    assert_eq!(document.metadata.block_count, 5);
    assert_eq!(
        document.to_markdown().unwrap(),
        "# Benchmark Report\n\nIntro paragraph.\n\n| Name | Score |\n| --- | --- |\n| Alpha | 42 |\n\n## Details\n\n- first\n- second"
    );
    let latex = document.to_latex().unwrap();
    assert!(latex.contains("\\section{Benchmark Report}"));
    assert!(latex.contains("\\subsection{Details}"));
    assert!(latex.contains("\\begin{itemize}"));
    assert!(latex.contains("\\item first"));
    assert!(latex.contains("\\begin{tabular}{lr}"));
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "heading_1");
            assert_eq!(block.text, "Benchmark Report");
            assert_eq!(block.source_anchors[0].extraction_method, "markdown_native");
        }
        other => panic!("expected heading block, got {other:?}"),
    }
    match &document.pages[0].blocks[2] {
        Block::Table(table) => {
            assert_eq!(table.headers, vec!["Name", "Score"]);
            assert_eq!(table.rows, vec![vec!["Alpha".to_owned(), "42".to_owned()]]);
            assert_eq!(table.source_anchors[0].extraction_method, "markdown_native");
        }
        other => panic!("expected table block, got {other:?}"),
    }
    match &document.pages[0].blocks[4] {
        Block::Text(block) => {
            assert_eq!(block.kind, "list");
            assert_eq!(block.text, "first\nsecond");
        }
        other => panic!("expected list block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_latex_sections_lists_and_tables() {
    let path = write_temp_file(
        "paper.tex",
        r#"\documentclass{article}
\usepackage{booktabs}
\title{Sample Paper}
\author{Dongler}
\begin{document}
\maketitle
\section{Intro}
This is \textbf{important} and costs 100\% of effort.

\subsection{Findings}
\begin{itemize}
\item first result
\item second \emph{result}
\end{itemize}

\begin{table}
\caption{Scores}
\begin{tabular}{lr}
\toprule
Name & Score \\
\midrule
Alpha & 42 \\
\bottomrule
\end{tabular}
\end{table}
\end{document}
"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "text");
    assert_eq!(document.metadata.engine, "latex-native");
    assert_eq!(document.metadata.title.as_deref(), Some("Sample Paper"));
    assert_eq!(document.metadata.block_count, 6);
    assert_eq!(
        document.to_markdown().unwrap(),
        "# Sample Paper\n\n# Intro\n\nThis is important and costs 100% of effort.\n\n## Findings\n\n- first result\n- second result\n\n| Name | Score |\n| --- | --- |\n| Alpha | 42 |"
    );

    let latex = document.to_latex().unwrap();
    assert!(latex.contains("\\section{Sample Paper}"));
    assert!(latex.contains("\\section{Intro}"));
    assert!(latex.contains("\\subsection{Findings}"));
    assert!(latex.contains("100\\% of effort"));
    assert!(latex.contains("\\begin{itemize}"));
    assert!(latex.contains("\\item second result"));
    assert!(latex.contains("\\begin{tabular}{lr}"));

    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "heading_1");
            assert_eq!(block.text, "Sample Paper");
            assert_eq!(block.source_anchors[0].extraction_method, "latex_native");
        }
        other => panic!("expected title heading block, got {other:?}"),
    }
    match &document.pages[0].blocks[5] {
        Block::Table(table) => {
            assert_eq!(table.caption.as_deref(), Some("Scores"));
            assert_eq!(table.headers, vec!["Name", "Score"]);
            assert_eq!(table.rows, vec![vec!["Alpha".to_owned(), "42".to_owned()]]);
            assert_eq!(table.source_anchors[0].extraction_method, "latex_native");
        }
        other => panic!("expected table block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_docbank_token_label_text_with_boxes() {
    let path = write_temp_file(
        "docbank.txt",
        "Deep\t10\t20\t40\t32\t0\t0\t0\tCMR10\ttitle\n\
         Learning\t45\t20\t100\t32\t0\t0\t0\tCMR10\ttitle\n\
         Works\t10\t60\t45\t72\t0\t0\t0\tCMR10\tparagraph\n\
         well\t50\t60\t75\t72\t0\t0\t0\tCMR10\tparagraph\n",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "text");
    assert_eq!(document.metadata.engine, "plain-text");
    assert_eq!(document.metadata.block_count, 2);
    assert_eq!(document.pages[0].width, Some(100.0));
    assert_eq!(document.pages[0].height, Some(72.0));
    assert_eq!(
        document.to_markdown().unwrap(),
        "Deep Learning\n\nWorks well"
    );
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "title");
            assert_eq!(block.bbox.unwrap().x, 10.0);
            assert_eq!(block.bbox.unwrap().y, 20.0);
            assert_eq!(block.bbox.unwrap().width, 90.0);
            assert_eq!(block.bbox.unwrap().height, 12.0);
            assert_eq!(block.lines.len(), 1);
            assert_eq!(block.lines[0].spans.len(), 2);
            assert_eq!(block.lines[0].spans[1].text, "Learning");
            assert_eq!(
                block.source_anchors[0].extraction_method,
                "docbank_token_labels"
            );
        }
        other => panic!("expected text block, got {other:?}"),
    }
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
fn load_path_classifies_larger_pdf_line_as_heading() {
    let path = write_temp_bytes("heading.pdf", heading_and_body_pdf());

    let document = load_path(&path).unwrap();

    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.text, "Introduction Heading");
            assert_eq!(block.kind, "heading_1");
        }
        other => panic!("expected heading text block, got {other:?}"),
    }
    let body_is_paragraph = document.pages[0].blocks.iter().any(|block| match block {
        Block::Text(text) => text.kind == "paragraph" && text.text.contains("ordinary body"),
        _ => false,
    });
    assert!(body_is_paragraph, "body line should stay a paragraph");

    let markdown = document.to_markdown().unwrap();
    assert!(
        markdown.contains("# Introduction Heading"),
        "markdown should render the heading: {markdown}"
    );
}

#[test]
fn load_path_expands_pdf_unicode_ligatures() {
    let path = write_temp_bytes("ligatures.pdf", ligature_font_pdf());

    let document = load_path(&path).unwrap();

    let text = match &document.pages[0].blocks[0] {
        Block::Text(block) => block.text.clone(),
        other => panic!("expected text block, got {other:?}"),
    };
    assert!(text.contains("file"), "expected expanded ligature in {text:?}");
    assert!(text.contains("flow"), "expected expanded ligature in {text:?}");
    assert!(
        !text.contains('\u{FB01}') && !text.contains('\u{FB02}'),
        "raw ligature codepoints should be expanded: {text:?}"
    );
}

#[test]
fn load_path_detects_bold_and_italic_pdf_fonts() {
    let path = write_temp_bytes("bold-italic.pdf", bold_italic_pdf());

    let document = load_path(&path).unwrap();

    let markdown = document.to_markdown().unwrap();
    assert!(
        markdown.contains("**Important warning**"),
        "expected bold markdown: {markdown}"
    );
    assert!(
        markdown.contains("*Subtle aside note*"),
        "expected italic markdown: {markdown}"
    );

    let mut saw_bold = false;
    let mut saw_italic = false;
    for block in &document.pages[0].blocks {
        if let Block::Text(text) = block {
            for span in text.lines.iter().flat_map(|line| line.spans.iter()) {
                saw_bold |= span.bold;
                saw_italic |= span.italic;
            }
        }
    }
    assert!(saw_bold, "expected a bold span");
    assert!(saw_italic, "expected an italic span");
}

#[test]
fn load_path_applies_pdf_page_rotation_to_geometry() {
    let path = write_temp_bytes("rotated.pdf", rotated_page_pdf(90));

    let document = load_path(&path).unwrap();
    let page = &document.pages[0];

    assert_eq!(page.rotation, Some(90));
    // Display dimensions swap for a 90/270 rotation.
    assert_eq!(page.width, Some(792.0));
    assert_eq!(page.height, Some(612.0));

    let markdown = document.to_markdown().unwrap();
    assert!(markdown.contains("Rotated heading"), "markdown: {markdown}");
    assert!(markdown.contains("Body text below it"), "markdown: {markdown}");

    for block in &page.blocks {
        if let Block::Text(text) = block {
            if let Some(bbox) = text.bbox {
                assert!(
                    bbox.x >= -1.0 && bbox.x <= 792.0 && bbox.y >= -1.0 && bbox.y <= 612.0,
                    "bbox {bbox:?} falls outside the rotated page extent"
                );
            }
        }
    }
}

#[test]
fn load_path_uses_font_ascent_descent_for_glyph_bbox() {
    let path = write_temp_bytes("font-metrics.pdf", font_metrics_pdf());

    let document = load_path(&path).unwrap();

    let block = match &document.pages[0].blocks[0] {
        Block::Text(text) => text,
        other => panic!("expected text block, got {other:?}"),
    };
    let height = block.bbox.unwrap().height;
    // (ascent 900 - descent -300)/1000 * 12pt = 14.4, distinct from the 12.0
    // flat-font-size box the extractor used before font metrics were applied.
    assert!(
        (height - 14.4).abs() < 0.6,
        "bbox height {height} should reflect the font ascent/descent"
    );
}

#[test]
fn load_path_decodes_ascii85_flate_pdf_streams() {
    let path = write_temp_bytes("ascii85-flate.pdf", ascii85_flate_pdf());

    let document = load_path(&path).unwrap();

    let markdown = document.to_markdown().unwrap();
    assert!(markdown.contains("ASCII85 filtered text"));
    assert_eq!(document.metadata.block_count, 1);
}

#[test]
fn load_path_extracts_pdf_with_inherited_page_resources_and_media_box() {
    let path = write_temp_bytes("inherited-page-resources.pdf", inherited_resources_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(document.pages[0].width, Some(300.0));
    assert_eq!(document.pages[0].height, Some(400.0));
    match &document.pages[0].blocks[0] {
        Block::Text(block) => assert_eq!(block.text, "A"),
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_decodes_octal_escaped_pdf_literals() {
    let path = write_temp_bytes("octal-literal.pdf", minimal_text_pdf("\\050Hello\\051"));

    let document = load_path(&path).unwrap();

    match &document.pages[0].blocks[0] {
        Block::Text(block) => assert_eq!(block.text, "(Hello)"),
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_recovers_private_use_pdf_font_ascii() {
    let path = write_temp_bytes("private-use-font.pdf", private_use_font_pdf());

    let document = load_path(&path).unwrap();

    match &document.pages[0].blocks[0] {
        Block::Text(block) => assert_eq!(block.text, "FDA125-316B0"),
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_decodes_to_unicode_bfrange_destination_arrays() {
    let path = write_temp_bytes("bfrange-array-font.pdf", bfrange_array_font_pdf());

    let document = load_path(&path).unwrap();

    match &document.pages[0].blocks[0] {
        Block::Text(block) => assert_eq!(block.text, "Average"),
        other => panic!("expected text block, got {other:?}"),
    }
    assert_eq!(document.to_markdown().unwrap(), "Average");
}

#[test]
fn load_path_decodes_pdf_encoding_differences() {
    let path = write_temp_bytes("encoding-differences.pdf", encoding_differences_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(document.to_markdown().unwrap(), "affine Φ Σ Ω don't");
}

#[test]
fn load_path_applies_pdf_text_matrix_scale_to_span_boxes() {
    let path = write_temp_bytes("scaled-text-matrix.pdf", scaled_text_matrix_pdf());

    let document = load_path(&path).unwrap();
    let Block::Text(block) = &document.pages[0].blocks[0] else {
        panic!("expected text block");
    };

    let span = &block.lines[0].spans[0];
    let bbox = span.bbox.unwrap();
    assert_eq!(span.text, "Scaled");
    assert!(
        bbox.width > 60.0,
        "expected scaled text width, got {bbox:?}"
    );
    assert!(
        bbox.height > 20.0,
        "expected scaled text height, got {bbox:?}"
    );
}

#[test]
fn load_path_applies_pdf_character_spacing_to_text_advance() {
    let path = write_temp_bytes("character-spacing.pdf", character_spacing_pdf());

    let document = load_path(&path).unwrap();
    let Block::Text(block) = &document.pages[0].blocks[0] else {
        panic!("expected text block");
    };

    let spans = &block.lines[0].spans;
    assert_eq!(block.text, "AB");
    assert!(spans[1].bbox.unwrap().x - spans[0].bbox.unwrap().x > 12.0);
}

#[test]
fn load_path_uses_pdf_font_widths_for_text_advance() {
    let path = write_temp_bytes("font-widths.pdf", font_widths_pdf());

    let document = load_path(&path).unwrap();
    let Block::Text(block) = &document.pages[0].blocks[0] else {
        panic!("expected text block");
    };

    let spans = &block.lines[0].spans;
    assert_eq!(block.text, "AB");
    assert!(
        spans[1].bbox.unwrap().x - spans[0].bbox.unwrap().x > 10.0,
        "{spans:?}"
    );
}

#[test]
fn load_path_applies_pdf_text_rise_to_span_box() {
    let path = write_temp_bytes("text-rise.pdf", text_rise_pdf());

    let document = load_path(&path).unwrap();
    let Block::Text(block) = &document.pages[0].blocks[0] else {
        panic!("expected text block");
    };

    let spans = &block.lines[0].spans;
    assert_eq!(block.text, "base super");
    assert!(spans[1].bbox.unwrap().y > spans[0].bbox.unwrap().y);
}

#[test]
fn load_path_reconstructs_pdf_superscripts_and_subscripts_from_geometry() {
    let path = write_temp_bytes("script-geometry.pdf", script_geometry_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(document.to_markdown().unwrap(), "x^2 + y_i = z");
}

#[test]
fn load_path_does_not_treat_offset_numeric_table_cells_as_scripts() {
    let path = write_temp_bytes("offset-numeric-cells.pdf", offset_numeric_cells_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(document.to_markdown().unwrap(), "10.615 -11.607 0.918");
}

#[test]
fn load_path_collapses_fragmented_pdf_word_glyphs() {
    let path = write_temp_bytes("fragmented-word.pdf", fragmented_word_pdf());

    let document = load_path(&path).unwrap();

    match &document.pages[0].blocks[0] {
        Block::Text(block) => assert_eq!(block.text, "attention listener's human-human"),
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_reads_cidfont_w_widths_for_word_spacing() {
    let path = write_temp_bytes("cidfont-w-widths.pdf", cidfont_w_widths_pdf());

    let document = load_path(&path).unwrap();

    // Correct only when the descendant CIDFont `/W` widths are parsed: the two
    // 1000-em glyphs abut, so they read as a single word rather than "A B".
    assert_eq!(document.to_markdown().unwrap(), "AB");
}

#[test]
fn load_path_repairs_pdf_word_piece_spacing_and_punctuation() {
    let path = write_temp_bytes("word-piece-spacing.pdf", word_piece_spacing_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        "coordinating listener's visual foci work, describe proposed"
    );
}

#[test]
fn load_path_repairs_fragmented_words_seen_in_column_pdfs() {
    let path = write_temp_bytes("column-word-fragments.pdf", column_word_fragments_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        "Participants in a conversation coordinate with one another.\n\nThe model centers on production demands."
    );
}

#[test]
fn load_path_repairs_pdf_hyphen_split_codes() {
    let path = write_temp_bytes("hyphen-split-code.pdf", hyphen_split_code_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(document.to_markdown().unwrap(), "FDA125-316B2");
}

#[test]
fn load_path_repairs_pdf_math_subscript_spacing() {
    let path = write_temp_bytes("math-subscript-spacing.pdf", math_subscript_spacing_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        r"n = m_1 + m_2 + \cdots + m_\ell"
    );
}

#[test]
fn load_path_repairs_pdf_math_tuple_ellipsis_subscripts() {
    let path = write_temp_bytes("math-tuple-ellipsis.pdf", math_tuple_ellipsis_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        r"( v, x_1),\ldots, ( v, x_s)"
    );
}

#[test]
fn load_path_repairs_pdf_control_glyph_math_text() {
    let path = write_temp_bytes("control-glyph-math.pdf", control_glyph_math_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        r"sufficient fine-tuning floating 0 \neq \lambda_i \in \mathbb{F}_q and \Lambda = \lambda_1"
    );
}

#[test]
fn load_path_repairs_pdf_combining_overlay_not_equal_math_text() {
    let path = write_temp_bytes(
        "combining-not-equal.pdf",
        utf16be_text_pdf("0 \u{338} = λ_i ∈ Fq"),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        r"0 \neq \lambda_i \in \mathbb{F}_q"
    );
}

#[test]
fn load_path_sanitizes_nonprinting_pdf_controls_before_ir_text() {
    let path = write_temp_bytes("nonprinting-controls.pdf", nonprinting_controls_pdf());

    let document = load_path(&path).unwrap();
    let block_text = document.pages[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .unwrap();

    assert_eq!(block_text, "safe text ok fine after");
    assert!(
        !block_text
            .chars()
            .any(|character| character.is_control() && character != '\n'),
        "block text contained nonprinting controls: {block_text:?}"
    );
    let span_text = match &document.pages[0].blocks[0] {
        Block::Text(text) => text.lines[0].spans[0].text.as_str(),
        _ => panic!("expected text block"),
    };
    assert_eq!(span_text, "safe text ok fine after");
    assert!(
        !span_text
            .chars()
            .any(|character| character.is_control() && character != '\n'),
        "span text contained nonprinting controls: {span_text:?}"
    );
}

#[test]
fn load_path_repairs_windows_1252_pdf_control_punctuation() {
    let path = write_temp_bytes("windows-1252-controls.pdf", windows_1252_controls_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        "Women's group – before \"quoted\" ... done"
    );
}

#[test]
fn load_path_repairs_pdf_math_arrows_and_greek_symbols() {
    let path = write_temp_bytes("math-arrows.pdf", math_arrows_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        r"\cdot: G \times X \to X and \tau ( g \cdot x) = g \cdot \tau ( x)"
    );
}

#[test]
fn load_path_repairs_pdf_uppercase_greek_math_symbols() {
    let path = write_temp_bytes("uppercase-greek-math.pdf", uppercase_greek_math_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        r"\Phi: \Sigma \to \Omega and \sum_i x_i"
    );
}

#[test]
fn load_path_does_not_split_single_column_pdf_math_at_column_band() {
    let path = write_temp_bytes("single-column-math-band.pdf", single_column_math_band_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        r"For some integer \ell \geq 1, let 0 \neq \lambda_i \in \mathbb{F}_q and m_i \geq 1"
    );
}

#[test]
fn load_path_does_not_split_repeated_body_math_at_column_band() {
    let path = write_temp_bytes("repeated-body-math-band.pdf", repeated_body_math_band_pdf());

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            r"Fix a basis i = 1, \ldots, smooth connected divisor",
            r"One can choose a = \lambda, validation data sample"
        ]
    );
}

#[test]
fn load_path_merges_wrapped_pdf_paragraph_lines() {
    let path = write_temp_bytes("wrapped-paragraph.pdf", wrapped_paragraph_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.block_count, 1);
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(
                block.text,
                "This paragraph has a hyphenated continuation and keeps flowing"
            );
            assert_eq!(
                document.to_markdown().unwrap(),
                "This paragraph has a hyphenated continuation and keeps flowing"
            );
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_merges_hyphenated_pdf_continuation_that_looks_like_heading() {
    let path = write_temp_bytes(
        "hyphenated-heading-continuation.pdf",
        hyphenated_heading_continuation_pdf(),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(
        document.to_markdown().unwrap(),
        "The broken underline: appears inside a paragraph"
    );
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
fn load_path_renders_image_only_pdf_as_figure_markdown() {
    let _guard = OCR_ENV_LOCK.lock().unwrap();
    clear_ocr_env();
    let path = write_temp_bytes("image-only.pdf", image_pdf());

    let document = load_path(&path).unwrap();
    let markdown = document.to_markdown().unwrap();

    assert!(matches!(document.pages[0].blocks[0], Block::Figure(_)));
    assert!(markdown.contains("!["));
    assert!(markdown.contains("image-1-Im1"));
}

#[test]
#[cfg(unix)]
fn load_path_can_ocr_image_only_pdf_when_fallback_is_enabled() {
    let _guard = OCR_ENV_LOCK.lock().unwrap();
    let path = write_temp_bytes("ocr-image-only.pdf", image_pdf());
    let harness = fake_ocr_harness();

    std::env::set_var("DONGLER_OCR_FALLBACK", "1");
    std::env::set_var("DONGLER_PDF_RENDERER", &harness.renderer);
    std::env::set_var("DONGLER_OCR_ENGINE", &harness.ocr);
    std::env::set_var("DONGLER_OCR_TEMP_DIR", &harness.temp_dir);

    let document = load_path(&path).unwrap();
    clear_ocr_env();

    let markdown = document.to_markdown().unwrap();
    assert!(markdown.contains("recognized OCR text"));
    assert!(markdown.contains("![Image image-1-Im1](image-1-Im1)"));
    assert!(matches!(document.pages[0].blocks[0], Block::Text(_)));
    assert!(matches!(document.pages[0].blocks[1], Block::Figure(_)));
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
fn load_path_preserves_pdf_text_around_detected_tables() {
    let path = write_temp_bytes("table-with-title.pdf", table_with_surrounding_text_pdf());

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.block_count, 3);
    assert_eq!(
        document.to_markdown().unwrap(),
        "Results Summary\n\n| Name | Value |\n| --- | --- |\n| Alpha | 42 |\n\nSource note"
    );
    match &document.pages[0].blocks[0] {
        Block::Text(block) => assert_eq!(block.text, "Results Summary"),
        other => panic!("expected text block before table, got {other:?}"),
    }
    match &document.pages[0].blocks[1] {
        Block::Table(table) => {
            assert_eq!(table.headers, vec!["Name", "Value"]);
            assert_eq!(table.rows, vec![vec!["Alpha".to_owned(), "42".to_owned()]]);
        }
        other => panic!("expected table block, got {other:?}"),
    }
    match &document.pages[0].blocks[2] {
        Block::Text(block) => assert_eq!(block.text, "Source note"),
        other => panic!("expected text block after table, got {other:?}"),
    }
}

#[test]
fn load_path_keeps_multi_section_statement_as_one_table() {
    // A statement with section-header rows ("Operating activities:",
    // "Operating expenses:") interleaved between data rows must extract as a
    // single table spanning all sections — not fragment at each header.
    let path = write_temp_bytes("multi-section-statement.pdf", multi_section_statement_pdf());
    let document = load_path(&path).unwrap();

    let table = document.pages[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected a single table spanning all sections");

    // The period header (years) is promoted to the header row; the statement
    // *title* above it stays out of the header (not pulled into the label column).
    assert_eq!(table.headers, vec!["".to_owned(), "2024".to_owned(), "2023".to_owned()]);
    assert!(
        table.rows.iter().all(|row| row[0] != "CONSOLIDATED STATEMENTS OF OPERATIONS"),
        "statement title leaked into a table row"
    );

    let labels: Vec<&str> = table.rows.iter().map(|row| row[0].as_str()).collect();
    assert!(labels.contains(&"Operating activities:"), "missing first section header: {labels:?}");
    assert!(labels.contains(&"Operating expenses:"), "missing later section header: {labels:?}");
    // Both a top and a bottom data row are present, so the table did not fragment.
    let net_sales = table.rows.iter().find(|row| row[0] == "Net sales").expect("Net sales row");
    assert_eq!(net_sales[1], "391,011");
    let net_income = table.rows.iter().find(|row| row[0] == "Net income").expect("Net income row");
    assert_eq!(net_income[1], "93,736");
    assert_eq!(net_income[2], "96,995");
    // A section header is a label-only row (its numeric columns are empty).
    let section = table.rows.iter().find(|row| row[0] == "Operating expenses:").unwrap();
    assert!(section[1..].iter().all(|cell| cell.is_empty()));
}

#[test]
fn load_path_merges_wrapped_row_label_into_one_row() {
    // A long row label that wrapped onto a previous line ("… beginning of" /
    // "period 12,345 …") must merge back into the figure row, and a section header
    // above an item must NOT be swallowed.
    let path = write_temp_bytes("wrapped-label.pdf", wrapped_label_statement_pdf());
    let document = load_path(&path).unwrap();

    let table = document.pages[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected a single statement table");

    let labels: Vec<&str> = table.rows.iter().map(|row| row[0].as_str()).collect();
    assert!(
        labels.contains(&"Cash and cash equivalents and restricted cash, beginning of period"),
        "wrapped label was not merged: {labels:?}"
    );
    // The wrap continuation is not left as its own stray row.
    assert!(!labels.iter().any(|label| *label == "period"), "stray wrap tail row: {labels:?}");
    // The section header keeps its own label-only row (not merged into an item).
    let operating = table.rows.iter().find(|row| row[0] == "Operating activities:").unwrap();
    assert!(operating[1..].iter().all(|cell| cell.is_empty()));
    let net_income = table.rows.iter().find(|row| row[0] == "Net income").expect("Net income row");
    assert_eq!(net_income[1], "5,000");
}

#[test]
fn load_path_extracts_pdf_table_from_implied_word_alignment() {
    let path = write_temp_bytes("implied-alignment-table.pdf", implied_alignment_table_pdf());

    let document = load_path(&path).unwrap();

    let table = document.pages[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected a table block");

    assert_eq!(
        table.headers,
        vec![
            "",
            "Total No",
            "Private doctor only",
            "Local council only",
            "Department of Health only",
            "More than one",
            "None",
        ]
    );
    assert_eq!(
        table.rows,
        vec![
            vec![
                "Sydney".to_owned(),
                "160".to_owned(),
                "108 (68)".to_owned(),
                "11 (7)".to_owned(),
                "15 (9)".to_owned(),
                "25 (16)".to_owned(),
                "1 (0-6)".to_owned(),
            ],
            vec![
                "Elsewhere".to_owned(),
                "44".to_owned(),
                "28 (65)".to_owned(),
                "1 (2)".to_owned(),
                "9 (20)".to_owned(),
                "4 (9)".to_owned(),
                "2 (5)".to_owned(),
            ],
        ]
    );
}

#[test]
fn load_path_extracts_pdf_table_from_ruled_grid_lines() {
    let path = write_temp_bytes("ruled-grid-table.pdf", ruled_grid_table_pdf());

    let document = load_path(&path).unwrap();

    let table = document.pages[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected ruled grid table");

    assert_eq!(table.headers, vec!["Description", "Result"]);
    assert_eq!(table.rows, vec![vec!["Alpha".to_owned(), "42".to_owned()]]);
}

#[test]
fn load_path_marks_merged_header_cell_as_column_span() {
    let path = write_temp_bytes("merged-header-grid.pdf", merged_header_grid_pdf());

    let document = load_path(&path).unwrap();

    let table = document.pages[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected ruled grid table with a merged header");

    // The rectangular grid is preserved for renderers.
    assert_eq!(
        table.headers,
        vec![
            "Item".to_owned(),
            "Measured Values Total".to_owned(),
            String::new(),
        ]
    );

    // The grouped header spans the two measurement columns...
    let span_cell = table
        .cells
        .iter()
        .find(|cell| cell.text == "Measured Values Total")
        .expect("expected the grouped header cell");
    assert_eq!(span_cell.column, 1);
    assert_eq!(span_cell.col_span, 2);
    assert_eq!(span_cell.row_span, 1);
    assert!(span_cell.is_header);

    // ...and its spanned-over continuation position is omitted from `cells`.
    assert!(!table
        .cells
        .iter()
        .any(|cell| cell.row == 0 && cell.column == 2));

    // Ordinary cells keep a span of 1.
    let width_cell = table
        .cells
        .iter()
        .find(|cell| cell.text == "Width")
        .expect("expected the Width sub-header");
    assert_eq!(width_cell.col_span, 1);
}

#[test]
fn load_path_does_not_treat_unlabeled_ruled_columns_as_table() {
    let path = write_temp_bytes("unlabeled-ruled-columns.pdf", unlabeled_ruled_columns_pdf());

    let document = load_path(&path).unwrap();

    assert!(document.pages[0]
        .blocks
        .iter()
        .all(|block| !matches!(block, Block::Table(_))));
    assert!(document.to_markdown().unwrap().contains("Left heading"));
}

#[test]
fn load_path_extracts_unlabeled_multirow_ruled_grid_as_table() {
    let path = write_temp_bytes(
        "unlabeled-multirow-ruled-grid.pdf",
        unlabeled_multirow_ruled_grid_pdf(),
    );

    let document = load_path(&path).unwrap();

    let table = document.pages[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected unlabeled ruled grid table");

    assert_eq!(table.headers, vec!["Class", "Explanation"]);
    assert_eq!(
        table.rows,
        vec![
            vec![
                "Marine".to_owned(),
                "Cargo and hull Large events".to_owned()
            ],
            vec!["Property".to_owned(), "Direct risks".to_owned()],
            vec!["Cyber".to_owned(), "Ransomware cover".to_owned()]
        ]
    );
}

#[test]
fn load_path_does_not_treat_numeric_multicolumn_body_as_implied_table() {
    let path = write_temp_bytes(
        "numeric-multicolumn-body.pdf",
        numeric_multicolumn_body_pdf(),
    );

    let document = load_path(&path).unwrap();

    assert!(document.pages[0]
        .blocks
        .iter()
        .all(|block| !matches!(block, Block::Table(_))));
    assert_eq!(
        document.to_markdown().unwrap(),
        "Left body 2015\n\nLeft continuation 2016\n\nRight body 3.9 kg\n\nRight continuation 5.0 kg"
    );
}

#[test]
fn load_path_keeps_column_order_around_detected_pdf_table() {
    let path = write_temp_bytes(
        "table-with-following-columns.pdf",
        table_with_following_columns_pdf(),
    );

    let document = load_path(&path).unwrap();
    let markdown = document.to_markdown().unwrap();

    assert!(markdown.contains("| Name | Value |"));
    assert!(
        markdown.contains("Left one\n\nLeft two\n\nRight one\n\nRight two"),
        "{markdown}"
    );
}

#[test]
fn load_path_orders_pdf_columns_before_interleaved_rows() {
    let path = write_temp_bytes("two-columns.pdf", two_column_pdf());

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec!["Left one", "Left two", "Right one", "Right two"]
    );
}

#[test]
fn load_path_splits_same_baseline_pdf_columns_before_ordering() {
    let path = write_temp_bytes("same-baseline-columns.pdf", same_baseline_columns_pdf());

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec!["Left one", "Left two", "Right one", "Right two"]
    );
}

#[test]
fn load_path_splits_same_baseline_pdf_columns_with_moderate_gutter() {
    let path = write_temp_bytes("moderate-gutter-columns.pdf", moderate_gutter_columns_pdf());

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            "Left column content here",
            "Left column continues",
            "Right one",
            "Right two"
        ]
    );
}

#[test]
fn load_path_splits_pdf_columns_when_estimated_width_overlaps_gutter() {
    let path = write_temp_bytes(
        "overlapping-gutter-columns.pdf",
        overlapping_gutter_columns_pdf(),
    );

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            "Left abstract phrase with listeners visual focus",
            "Left continuation",
            "Right body starts",
            "Right follows"
        ]
    );
}

#[test]
fn load_path_splits_pdf_columns_at_tight_right_column_band() {
    let path = write_temp_bytes("tight-band-columns.pdf", tight_band_columns_pdf());

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            "demand is checked at the beginning of every left column continues",
            "gagement. Beyond the number of words right column continues"
        ]
    );
}

#[test]
fn load_path_splits_pdf_columns_when_left_column_contains_math() {
    let path = write_temp_bytes("math-left-column-band.pdf", math_left_column_band_pdf());

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            r"1: T = \lambda",
            "2: i = arg max",
            "Right prose begins additional words",
            "Right prose continues additional words"
        ]
    );
}

#[test]
fn load_path_splits_pdf_algorithm_columns_with_single_right_run() {
    let path = write_temp_bytes(
        "algorithm-single-right-run.pdf",
        algorithm_single_right_run_pdf(),
    );

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            r"1: T = \lambda",
            "2: while l \\leq N do",
            "a correlation-aware selection mechanism",
            "resolve coherence conflicts"
        ]
    );
}

#[test]
fn load_path_keeps_right_column_formula_base_out_of_left_paragraph() {
    let path = write_temp_bytes(
        "formula-base-before-tight-band.pdf",
        formula_base_before_tight_band_pdf(),
    );

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        !text_blocks.iter().any(|text| text.contains("reduces H")),
        "{text_blocks:?}"
    );
    assert!(
        text_blocks
            .iter()
            .any(|text| text.contains("HT is formed by row vectors")),
        "{text_blocks:?}"
    );
}

#[test]
fn load_path_keeps_pdf_front_matter_and_footnote_out_of_column_body() {
    let path = write_temp_bytes(
        "front-matter-footnote-columns.pdf",
        front_matter_footnote_columns_pdf(),
    );

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            "Sample Title",
            "Left Author",
            "Right Author",
            "Abstract",
            "Left body turns to meet",
            "Left body continues",
            "the gaze of speaker right body follows",
            "Footnote text"
        ]
    );
}

#[test]
fn load_path_keeps_full_width_pdf_title_before_ordered_columns() {
    let path = write_temp_bytes("title-with-columns.pdf", title_with_columns_pdf());

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            "Centered Title",
            "Left one",
            "Left two",
            "Right one",
            "Right two"
        ]
    );
}

#[test]
fn load_path_keeps_wide_title_before_staggered_pdf_columns() {
    let path = write_temp_bytes(
        "wide-title-staggered-columns.pdf",
        wide_title_staggered_columns_pdf(),
    );

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            "A Wide Centered Title Spanning Both Columns",
            "Left paragraph hyphenated continuation",
            "Right first",
            "Right second"
        ]
    );
}

#[test]
fn load_path_assigns_wide_staggered_lines_to_pdf_columns() {
    let path = write_temp_bytes(
        "wide-staggered-column-lines.pdf",
        wide_staggered_column_lines_pdf(),
    );

    let document = load_path(&path).unwrap();

    let text_blocks = document.pages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        text_blocks,
        vec![
            "Left seed one",
            "Left seed two",
            "Left lower paragraph has a long hyphenated continuation keeps flowing as text",
            "Right seed one",
            "Right seed two",
            "Right lower first",
            "Right lower second"
        ]
    );
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

#[test]
fn load_path_with_options_can_suppress_repeated_headers_and_footers() {
    let path = write_temp_bytes("headers-footers.pdf", repeated_headers_footers_pdf());

    let document = load_path_with_options(
        &path,
        ExtractOptions {
            suppress_headers_footers: true,
            ..ExtractOptions::default()
        },
    )
    .unwrap();

    let text_blocks = document
        .pages
        .iter()
        .flat_map(|page| page.blocks.iter())
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(text_blocks, vec!["Body one", "Body two", "Body three"]);
    assert_eq!(document.metadata.block_count, 3);
    assert_eq!(document.metadata.word_count, 6);
}

#[test]
fn load_path_extracts_image_dimensions_as_page_asset() {
    let path = write_temp_bytes("scan.png", png_fixture(2, 3));

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "image");
    assert_eq!(document.metadata.engine, "image-native");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.pages[0].width, Some(2.0));
    assert_eq!(document.pages[0].height, Some(3.0));
    assert_eq!(document.metadata.block_count, 1);
    assert_eq!(document.pages[0].blocks.len(), 1);
    assert_eq!(document.pages[0].images.len(), 1);
    assert_eq!(document.pages[0].assets.len(), 1);
    assert_eq!(document.assets.len(), 1);
    match &document.pages[0].blocks[0] {
        Block::Figure(block) => {
            assert_eq!(block.image_ref.as_deref(), Some("image-1"));
            assert_eq!(block.bbox.unwrap().width, 2.0);
            assert_eq!(block.bbox.unwrap().height, 3.0);
            assert_eq!(block.source_anchors[0].extraction_method, "image_native");
        }
        other => panic!("expected figure block, got {other:?}"),
    }
    assert_eq!(document.to_markdown().unwrap(), "![scan.png](image-1)");
}

#[test]
fn load_path_extracts_little_endian_tiff_dimensions_as_page_asset() {
    let path = write_temp_bytes("scan.tif", tiff_fixture(640, 480, true));

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "image");
    assert_eq!(document.metadata.engine, "image-native");
    assert_eq!(document.pages[0].width, Some(640.0));
    assert_eq!(document.pages[0].height, Some(480.0));
    assert_eq!(document.pages[0].images[0].width, Some(640));
    assert_eq!(document.pages[0].images[0].height, Some(480));
}

#[test]
fn load_path_extracts_big_endian_tiff_dimensions_as_page_asset() {
    let path = write_temp_bytes("scan.tiff", tiff_fixture(300, 200, false));

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "image");
    assert_eq!(document.metadata.engine, "image-native");
    assert_eq!(document.pages[0].width, Some(300.0));
    assert_eq!(document.pages[0].height, Some(200.0));
    assert_eq!(document.pages[0].images[0].width, Some(300));
    assert_eq!(document.pages[0].images[0].height, Some(200));
}

#[test]
fn load_path_extracts_html_text_blocks() {
    let path = write_temp_file(
        "page.html",
        "<!doctype html><title>Ignored</title><h1>Quarterly Report</h1><p>Revenue &amp; margin</p><script>noise()</script>",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "html");
    assert_eq!(document.metadata.engine, "html-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "Quarterly Report\n\nRevenue & margin"
    );
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_extracts_hocr_html_ocr_lines_with_boxes() {
    let path = write_temp_file(
        "page.hocr.html",
        r#"<!doctype html>
<html>
  <body>
    <div class="ocr_page" title="bbox 0 0 640 480">
      <span class="ocr_line" title="bbox 10 20 190 44">
        <span class="ocrx_word" title="bbox 10 20 80 44; x_wconf 97">Historic</span>
        <span class="ocrx_word" title="bbox 100 20 190 44; x_wconf 96">Document</span>
      </span>
    </div>
  </body>
</html>"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "html");
    assert_eq!(document.metadata.engine, "html-native");
    assert_eq!(document.pages[0].width, Some(640.0));
    assert_eq!(document.pages[0].height, Some(480.0));
    assert_eq!(document.to_markdown().unwrap(), "Historic Document");
    assert_eq!(document.metadata.block_count, 1);

    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "ocr_line");
            assert_eq!(block.bbox.unwrap().x, 10.0);
            assert_eq!(block.bbox.unwrap().width, 180.0);
            assert_eq!(block.source_anchors[0].bbox.unwrap().height, 24.0);
            assert_eq!(block.source_anchors[0].extraction_method, "html_native");
            assert_eq!(block.lines[0].spans[1].bbox.unwrap().x, 100.0);
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_email_subject_and_body() {
    let path = write_temp_file(
        "message.eml",
        "From: sender@example.com\r\nSubject: Launch Update\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nFirst line\r\n\r\nSecond line",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "email");
    assert_eq!(document.metadata.engine, "email-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "Launch Update\n\nFirst line\n\nSecond line"
    );
    assert_eq!(document.metadata.block_count, 3);
}

#[test]
fn load_path_extracts_omnidocbench_json_pages_with_geometry() {
    let path = write_temp_file(
        "omnidocbench.json",
        r#"[
            {
                "page_info": {"page_no": 0, "width": 200, "height": 300, "image_path": "page.png"},
                "layout_dets": [
                    {
                        "category_type": "text_block",
                        "ignore": true,
                        "order": 1,
                        "poly": [0, 0, 10, 0, 10, 10, 0, 10],
                        "text": "Ignored"
                    },
                    {
                        "category_type": "title",
                        "ignore": false,
                        "order": 2,
                        "poly": [10, 20, 110, 20, 110, 40, 10, 40],
                        "text": "Demo Title"
                    },
                    {
                        "category_type": "table",
                        "ignore": false,
                        "order": 3,
                        "poly": [10, 50, 190, 50, 190, 100, 10, 100],
                        "html": "<table><tr><th>Name</th><th>Value</th></tr><tr><td>Alpha</td><td>42</td></tr></table>"
                    },
                    {
                        "category_type": "equation_isolated",
                        "ignore": false,
                        "order": 4,
                        "poly": [10, 110, 100, 110, 100, 140, 10, 140],
                        "latex": "$$x=1$$"
                    }
                ]
            }
        ]"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "json");
    assert_eq!(document.metadata.engine, "json-native");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.pages[0].number, 1);
    assert_eq!(document.pages[0].width, Some(200.0));
    assert_eq!(document.pages[0].height, Some(300.0));
    assert_eq!(document.metadata.block_count, 3);
    assert_eq!(
        document.to_markdown().unwrap(),
        "Demo Title\n\n| Name | Value |\n| --- | --- |\n| Alpha | 42 |\n\n$$x=1$$"
    );

    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "title");
            assert_eq!(block.bbox.unwrap().width, 100.0);
            assert_eq!(block.source_anchors[0].page_number, 1);
            assert_eq!(block.source_anchors[0].extraction_method, "json_native");
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_jsonl_text_records() {
    let path = write_temp_file(
        "records.jsonl",
        r#"{"title":"First record","body_text":[{"section":"Intro","text":"Body paragraph"}]}
{"text":"Second record"}"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "json");
    assert_eq!(document.metadata.engine, "json-native");
    assert_eq!(document.pages.len(), 2);
    assert_eq!(
        document.to_markdown().unwrap(),
        "First record\n\nBody paragraph\n\nSecond record"
    );
}

#[test]
fn load_path_extracts_gzipped_jsonl_text_records() {
    let path = write_temp_bytes(
        "papers.jsonl.gz",
        gzip_bytes(r#"{"title":"Compressed","abstract":"JSONL text"}"#),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "json");
    assert_eq!(document.metadata.engine, "json-native");
    assert_eq!(document.to_markdown().unwrap(), "Compressed\n\nJSONL text");
}

#[test]
fn load_path_extracts_pubtabnet_jsonl_table_structure() {
    let path = write_temp_file(
        "pubtabnet.jsonl",
        r#"{"filename":"table.png","html":{"cell":[{"tokens":["Name"],"bbox":[1,2,30,12]},{"tokens":["Value"],"bbox":[40,2,90,12]},{"tokens":["Alpha"],"bbox":[1,20,30,30]},{"tokens":["42"],"bbox":[40,20,90,30]}],"structure":{"tokens":["<thead>","<tr>","<td>","</td>","<td>","</td>","</tr>","</thead>","<tbody>","<tr>","<td>","</td>","<td>","</td>","</tr>","</tbody>"]}}}"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "json");
    assert_eq!(document.metadata.engine, "json-native");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(
        document.to_markdown().unwrap(),
        "| Name | Value |\n| --- | --- |\n| Alpha | 42 |"
    );
    match &document.pages[0].blocks[0] {
        Block::Table(table) => {
            assert_eq!(table.headers, vec!["Name", "Value"]);
            assert_eq!(table.rows, vec![vec!["Alpha".to_owned(), "42".to_owned()]]);
            assert_eq!(table.bbox.unwrap().x, 1.0);
            assert_eq!(table.bbox.unwrap().y, 2.0);
            assert_eq!(table.bbox.unwrap().width, 89.0);
            assert_eq!(table.bbox.unwrap().height, 28.0);
            assert_eq!(table.cells.len(), 4);
            assert_eq!(table.cells[0].text, "Name");
            assert!(table.cells[0].is_header);
            assert_eq!(table.cells[2].row, 1);
            assert_eq!(table.cells[2].bbox.unwrap().y, 20.0);
            assert_eq!(table.source_anchors[0].bbox.unwrap().width, 89.0);
            assert_eq!(table.source_anchors[0].extraction_method, "json_native");
        }
        other => panic!("expected table block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_word_json_annotations_with_boxes() {
    let path = write_temp_file(
        "pubtables-words.json",
        r#"{
            "image_width": 640,
            "image_height": 480,
            "words": [
                {"text": "Patient", "bbox": [10, 20, 70, 36]},
                {"text": "Value", "bbox": [90, 20, 135, 36]}
            ]
        }"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "json");
    assert_eq!(document.metadata.engine, "json-native");
    assert_eq!(document.pages[0].width, Some(640.0));
    assert_eq!(document.pages[0].height, Some(480.0));
    assert_eq!(document.to_markdown().unwrap(), "Patient\n\nValue");
    assert_eq!(document.metadata.block_count, 2);

    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "word");
            assert_eq!(block.bbox.unwrap().x, 10.0);
            assert_eq!(block.bbox.unwrap().width, 60.0);
            assert_eq!(block.source_anchors[0].bbox.unwrap().height, 16.0);
            assert_eq!(block.source_anchors[0].extraction_method, "json_native");
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_grid_cells_json_table_structure() {
    let path = write_temp_file(
        "grid-table.json",
        r#"{
            "table_bbox": [5, 6, 205, 106],
            "cells": [
                [
                    {"tokens": ["Name"], "bbox": [10, 20, 60, 40]},
                    {"tokens": ["Value"], "bbox": [70, 20, 130, 40]}
                ],
                [
                    {"tokens": ["Alpha"], "bbox": [10, 45, 60, 65]},
                    {"tokens": ["42"], "bbox": [70, 45, 130, 65]}
                ]
            ]
        }"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "json");
    assert_eq!(document.metadata.engine, "json-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "| Name | Value |\n| --- | --- |\n| Alpha | 42 |"
    );
    match &document.pages[0].blocks[0] {
        Block::Table(table) => {
            assert_eq!(table.headers, vec!["Name", "Value"]);
            assert_eq!(table.rows, vec![vec!["Alpha".to_owned(), "42".to_owned()]]);
            assert_eq!(table.bbox.unwrap().width, 200.0);
            assert_eq!(table.cells.len(), 4);
            assert!(table.cells[0].is_header);
            assert_eq!(table.cells[3].bbox.unwrap().x, 70.0);
        }
        other => panic!("expected table block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_funsd_json_form_blocks_with_boxes() {
    let path = write_temp_file(
        "form.json",
        r#"{
            "form": [
                {"box": [10, 20, 110, 40], "text": "Date:", "label": "question", "id": 0},
                {"box": [120, 20, 170, 40], "text": "2026-05-27", "label": "answer", "id": 1}
            ]
        }"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "json");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.metadata.block_count, 2);
    assert_eq!(document.to_markdown().unwrap(), "Date:\n\n2026-05-27");
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "question");
            assert_eq!(block.bbox.unwrap().width, 100.0);
            assert_eq!(block.source_anchors[0].bbox.unwrap().height, 20.0);
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_coco_json_layout_annotations() {
    let path = write_temp_file(
        "coco.json",
        r#"{
            "images": [{"id": 1, "file_name": "page.png", "width": 100, "height": 200}],
            "categories": [{"id": 2, "name": "title"}],
            "annotations": [{"id": 10, "image_id": 1, "category_id": 2, "bbox": [10, 20, 30, 40]}]
        }"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "json");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.pages[0].width, Some(100.0));
    assert_eq!(document.pages[0].height, Some(200.0));
    assert_eq!(document.to_markdown().unwrap(), "title");
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "title");
            assert_eq!(block.bbox.unwrap().x, 10.0);
            assert_eq!(block.bbox.unwrap().y, 20.0);
            assert_eq!(block.bbox.unwrap().width, 30.0);
            assert_eq!(block.bbox.unwrap().height, 40.0);
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_sroie_csv_ocr_boxes_with_embedded_commas() {
    let path = write_temp_file(
        "receipt.csv",
        "1,2,11,2,11,12,1,12,NO.2, JALAN TEST\n20,30,50,30,50,45,20,45,TOTAL RM 12.30\n",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "csv");
    assert_eq!(document.metadata.engine, "csv-native");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.metadata.block_count, 2);
    assert_eq!(
        document.to_markdown().unwrap(),
        "NO.2, JALAN TEST\n\nTOTAL RM 12.30"
    );
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "row");
            assert_eq!(block.bbox.unwrap().x, 1.0);
            assert_eq!(block.bbox.unwrap().y, 2.0);
            assert_eq!(block.bbox.unwrap().width, 10.0);
            assert_eq!(block.bbox.unwrap().height, 10.0);
            assert_eq!(block.source_anchors[0].extraction_method, "csv_native");
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_tesseract_tsv_ocr_lines_with_boxes() {
    let path = write_temp_file(
        "ocr.tsv",
        "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
         1\t1\t0\t0\t0\t0\t0\t0\t640\t480\t-1\t\n\
         5\t1\t1\t1\t1\t1\t10\t20\t70\t24\t97\tHistoric\n\
         5\t1\t1\t1\t1\t2\t100\t20\t90\t24\t96\tDocument\n",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "csv");
    assert_eq!(document.metadata.engine, "csv-native");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.metadata.block_count, 1);
    assert_eq!(document.pages[0].width, Some(190.0));
    assert_eq!(document.pages[0].height, Some(44.0));
    assert_eq!(document.to_markdown().unwrap(), "Historic Document");
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "ocr_line");
            assert_eq!(block.bbox.unwrap().x, 10.0);
            assert_eq!(block.bbox.unwrap().y, 20.0);
            assert_eq!(block.bbox.unwrap().width, 180.0);
            assert_eq!(block.bbox.unwrap().height, 24.0);
            assert_eq!(block.lines.len(), 1);
            assert_eq!(block.lines[0].text, "Historic Document");
            assert_eq!(block.lines[0].spans.len(), 2);
            assert_eq!(block.lines[0].spans[1].text, "Document");
            assert_eq!(block.lines[0].spans[1].bbox.unwrap().x, 100.0);
            assert_eq!(block.source_anchors[0].extraction_method, "csv_native");
            assert!((block.confidence.as_ref().unwrap().score - 0.965).abs() < 0.0001);
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_ckorzen_tsv_feature_boxes() {
    let path = write_temp_file(
        "expected.tsv",
        "feature\tstart line\tend line\tbounding boxes\ttext\n\
         title\t15\t16\t(1;[302.199280;84.528076;308.052277;92.232506])\tQuantum Hall Solitons\n",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "csv");
    assert_eq!(document.metadata.engine, "csv-native");
    assert_eq!(document.metadata.block_count, 1);
    assert_eq!(document.to_markdown().unwrap(), "Quantum Hall Solitons");
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "title");
            assert!((block.bbox.unwrap().x - 302.19928).abs() < 0.0001);
            assert!((block.bbox.unwrap().y - 84.528076).abs() < 0.0001);
            assert!((block.bbox.unwrap().width - 5.852997).abs() < 0.0001);
            assert!((block.bbox.unwrap().height - 7.70443).abs() < 0.0001);
            assert_eq!(block.source_anchors[0].page_number, 1);
            assert_eq!(block.source_anchors[0].extraction_method, "csv_native");
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_xml_article_text_blocks() {
    let path = write_temp_file(
        "article.nxml",
        "<article><front><title-group><article-title>Native XML</article-title></title-group></front><body><sec><p>Alpha &amp; beta</p></sec></body></article>",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "xml");
    assert_eq!(document.metadata.engine, "xml-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "Native XML\n\nAlpha & beta"
    );
}

#[test]
fn load_path_extracts_pascal_voc_xml_layout_boxes() {
    let path = write_temp_file(
        "annotation.xml",
        "<annotation><size><width>800</width><height>600</height></size><object><name>table</name><bndbox><xmin>10</xmin><ymin>20</ymin><xmax>210</xmax><ymax>120</ymax></bndbox></object></annotation>",
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "xml");
    assert_eq!(document.metadata.engine, "xml-native");
    assert_eq!(document.pages[0].width, Some(800.0));
    assert_eq!(document.pages[0].height, Some(600.0));
    assert_eq!(document.to_markdown().unwrap(), "table");
    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "table");
            assert_eq!(block.bbox.unwrap().x, 10.0);
            assert_eq!(block.bbox.unwrap().y, 20.0);
            assert_eq!(block.bbox.unwrap().width, 200.0);
            assert_eq!(block.bbox.unwrap().height, 100.0);
            assert_eq!(block.source_anchors[0].extraction_method, "xml_native");
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_alto_xml_ocr_lines_with_boxes() {
    let path = write_temp_file(
        "alto.xml",
        r#"<?xml version="1.0"?>
<alto xmlns="http://www.loc.gov/standards/alto/ns-v4#">
  <Layout>
    <Page WIDTH="640" HEIGHT="480">
      <PrintSpace>
        <TextBlock>
          <TextLine HPOS="10" VPOS="20" WIDTH="180" HEIGHT="24">
            <String CONTENT="Historic" HPOS="10" VPOS="20" WIDTH="80" HEIGHT="24" WC="0.99"/>
            <SP WIDTH="10"/>
            <String CONTENT="Document" HPOS="100" VPOS="20" WIDTH="90" HEIGHT="24" WC="0.98"/>
          </TextLine>
        </TextBlock>
      </PrintSpace>
    </Page>
  </Layout>
</alto>"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "xml");
    assert_eq!(document.metadata.engine, "xml-native");
    assert_eq!(document.pages[0].width, Some(640.0));
    assert_eq!(document.pages[0].height, Some(480.0));
    assert_eq!(document.to_markdown().unwrap(), "Historic Document");
    assert_eq!(document.metadata.block_count, 1);

    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "ocr_line");
            assert_eq!(block.bbox.unwrap().x, 10.0);
            assert_eq!(block.bbox.unwrap().width, 180.0);
            assert_eq!(block.source_anchors[0].bbox.unwrap().height, 24.0);
            assert_eq!(block.source_anchors[0].extraction_method, "xml_native");
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_page_xml_ocr_lines_with_boxes() {
    let path = write_temp_file(
        "pagexml.xml",
        r#"<?xml version="1.0"?>
<PcGts xmlns="http://schema.primaresearch.org/PAGE/gts/pagecontent/2019-07-15">
  <Page imageWidth="640" imageHeight="480">
    <TextRegion id="r1">
      <TextLine id="l1">
        <Coords points="10,20 190,20 190,44 10,44"/>
        <Word id="w1">
          <Coords points="10,20 80,20 80,44 10,44"/>
          <TextEquiv conf="0.97"><Unicode>Historic</Unicode></TextEquiv>
        </Word>
        <Word id="w2">
          <Coords points="100,20 190,20 190,44 100,44"/>
          <TextEquiv conf="0.96"><Unicode>Document</Unicode></TextEquiv>
        </Word>
        <TextEquiv><Unicode>Historic Document</Unicode></TextEquiv>
      </TextLine>
    </TextRegion>
  </Page>
</PcGts>"#,
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "xml");
    assert_eq!(document.metadata.engine, "xml-native");
    assert_eq!(document.pages[0].width, Some(640.0));
    assert_eq!(document.pages[0].height, Some(480.0));
    assert_eq!(document.to_markdown().unwrap(), "Historic Document");
    assert_eq!(document.metadata.block_count, 1);

    match &document.pages[0].blocks[0] {
        Block::Text(block) => {
            assert_eq!(block.kind, "ocr_line");
            assert_eq!(block.bbox.unwrap().x, 10.0);
            assert_eq!(block.bbox.unwrap().width, 180.0);
            assert_eq!(block.lines[0].spans[1].bbox.unwrap().x, 100.0);
            assert_eq!(block.source_anchors[0].extraction_method, "xml_native");
        }
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn load_path_extracts_docx_paragraphs_from_openxml() {
    let path = write_temp_bytes(
        "report.docx",
        zip_fixture(&[(
            "word/document.xml",
            r#"<?xml version="1.0"?><w:document xmlns:w="w"><w:body><w:p><w:r><w:t>First paragraph</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r><w:r><w:t> paragraph</w:t></w:r></w:p></w:body></w:document>"#,
        )]),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "word");
    assert_eq!(document.metadata.engine, "openxml-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "First paragraph\n\nSecond paragraph"
    );
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_extracts_xlsx_shared_string_rows_from_openxml() {
    let path = write_temp_bytes(
        "book.xlsx",
        zip_fixture(&[
            (
                "xl/sharedStrings.xml",
                r#"<?xml version="1.0"?><sst><si><t>Name</t></si><si><t>Value</t></si><si><t>Alpha</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<?xml version="1.0"?><worksheet><sheetData><row><c t="s"><v>0</v></c><c t="s"><v>1</v></c></row><row><c t="s"><v>2</v></c><c><v>42</v></c></row></sheetData></worksheet>"#,
            ),
        ]),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "excel");
    assert_eq!(document.metadata.engine, "openxml-native");
    assert_eq!(document.to_markdown().unwrap(), "Name Value\n\nAlpha 42");
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_extracts_pptx_slide_text_from_openxml() {
    let path = write_temp_bytes(
        "deck.pptx",
        zip_fixture(&[
            (
                "ppt/slides/slide2.xml",
                r#"<?xml version="1.0"?><p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Revenue</a:t></a:r><a:r><a:t> Growth</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<?xml version="1.0"?><p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Quarterly Update</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
            ),
        ]),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "presentation");
    assert_eq!(document.metadata.engine, "openxml-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "Quarterly Update\n\nRevenue Growth"
    );
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_extracts_odt_paragraphs_from_opendocument() {
    let path = write_temp_bytes(
        "notes.odt",
        zip_fixture(&[(
            "content.xml",
            r#"<?xml version="1.0"?><office:document-content xmlns:office="office" xmlns:text="text"><office:body><office:text><text:p>First paragraph</text:p><text:p>Second &amp; third</text:p></office:text></office:body></office:document-content>"#,
        )]),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "opendocument");
    assert_eq!(document.metadata.engine, "openxml-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "First paragraph\n\nSecond & third"
    );
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_extracts_ods_rows_from_opendocument() {
    let path = write_temp_bytes(
        "sheet.ods",
        zip_fixture(&[(
            "content.xml",
            r#"<?xml version="1.0"?><office:document-content xmlns:office="office" xmlns:table="table" xmlns:text="text"><office:body><office:spreadsheet><table:table><table:table-row><table:table-cell><text:p>Name</text:p></table:table-cell><table:table-cell><text:p>Value</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell><text:p>Alpha</text:p></table:table-cell><table:table-cell><text:p>42</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#,
        )]),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "opendocument");
    assert_eq!(document.metadata.engine, "openxml-native");
    assert_eq!(document.to_markdown().unwrap(), "Name Value\n\nAlpha 42");
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_extracts_odp_slide_text_from_opendocument() {
    let path = write_temp_bytes(
        "slides.odp",
        zip_fixture(&[(
            "content.xml",
            r#"<?xml version="1.0"?><office:document-content xmlns:office="office" xmlns:draw="draw" xmlns:text="text"><office:body><office:presentation><draw:page><text:p>Slide One</text:p><text:p>Bullet A</text:p></draw:page><draw:page><text:p>Slide Two</text:p></draw:page></office:presentation></office:body></office:document-content>"#,
        )]),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "opendocument");
    assert_eq!(document.metadata.engine, "openxml-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "Slide One\n\nBullet A\n\nSlide Two"
    );
    assert_eq!(document.metadata.block_count, 3);
}

#[test]
fn load_path_extracts_tar_source_package_text() {
    let path = write_temp_bytes(
        "source.tar",
        tar_fixture(&[
            ("paper/main.tex", "A theorem from TeX.\n\nSecond paragraph."),
            ("paper/readme.md", "Ignored readme"),
            ("paper/figure.png", "not text"),
        ]),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "archive");
    assert_eq!(document.metadata.engine, "archive-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "A theorem from TeX.\n\nSecond paragraph.\n\nIgnored readme"
    );
    assert_eq!(document.metadata.block_count, 3);
}

#[test]
fn load_path_extracts_gzipped_tar_xml_source_package_text() {
    let path = write_temp_bytes(
        "pmc.tar.gz",
        gzip_raw_bytes(&tar_fixture(&[(
            "article.nxml",
            "<article><front><article-title>PMC Article</article-title></front><body><p>Body text</p></body></article>",
        )])),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "archive");
    assert_eq!(document.metadata.engine, "archive-native");
    assert_eq!(document.to_markdown().unwrap(), "PMC Article\n\nBody text");
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_extracts_gzipped_single_source_text() {
    let path = write_temp_bytes(
        "2401.00001.gz",
        gzip_raw_bytes(b"Bare arXiv source text.\n\nSecond paragraph."),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "archive");
    assert_eq!(document.metadata.engine, "archive-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "Bare arXiv source text.\n\nSecond paragraph."
    );
    assert_eq!(document.metadata.block_count, 2);
}

#[test]
fn load_path_extracts_zip_source_package_text() {
    let path = write_temp_bytes(
        "dataset.zip",
        zip_fixture(&[
            (
                "records.jsonl",
                r#"{"title":"Zipped record","abstract":"Zip text"}"#,
            ),
            (
                "article.nxml",
                "<article><front><article-title>Zip XML</article-title></front><body><p>XML body</p></body></article>",
            ),
            ("image.png", "not text"),
        ]),
    );

    let document = load_path(&path).unwrap();

    assert_eq!(document.metadata.format, "archive");
    assert_eq!(document.metadata.engine, "archive-native");
    assert_eq!(
        document.to_markdown().unwrap(),
        "Zipped record\n\nZip text\n\nZip XML\n\nXML body"
    );
    assert_eq!(document.metadata.block_count, 4);
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

fn clear_ocr_env() {
    for key in [
        "DONGLER_OCR_FALLBACK",
        "DONGLER_PDF_RENDERER",
        "DONGLER_OCR_ENGINE",
        "DONGLER_OCR_TEMP_DIR",
    ] {
        std::env::remove_var(key);
    }
}

#[cfg(unix)]
struct FakeOcrHarness {
    renderer: PathBuf,
    ocr: PathBuf,
    temp_dir: PathBuf,
}

#[cfg(unix)]
fn fake_ocr_harness() -> FakeOcrHarness {
    use std::os::unix::fs::PermissionsExt;

    let root = write_temp_bytes("harness-marker", Vec::new())
        .parent()
        .unwrap()
        .to_owned();
    let renderer = root.join("fake-pdftoppm");
    let ocr = root.join("fake-tesseract");
    let temp_dir = root.join("ocr-temp");
    fs::create_dir_all(&temp_dir).unwrap();
    fs::write(
        &renderer,
        "#!/bin/sh\nlast=\"\"\nfor arg in \"$@\"; do last=\"$arg\"; done\nprintf 'fake image' > \"${last}.png\"\n",
    )
    .unwrap();
    fs::write(
        &ocr,
        "#!/bin/sh\nprintf 'recognized OCR text\\nsecond OCR line\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&renderer, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&ocr, fs::Permissions::from_mode(0o755)).unwrap();

    FakeOcrHarness {
        renderer,
        ocr,
        temp_dir,
    }
}

fn gzip_bytes(contents: &str) -> Vec<u8> {
    gzip_raw_bytes(contents.as_bytes())
}

fn gzip_raw_bytes(contents: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(contents).unwrap();
    encoder.finish().unwrap()
}

fn minimal_text_pdf(text: &str) -> Vec<u8> {
    pdf_fixture(&format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    ), &format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET"), "")
}

fn ascii85_flate_pdf() -> Vec<u8> {
    let encoded_stream =
        "<~Garg^;:'MC<%p.,#Y@rK2Zb0*KocuP%EDjh:JV?sKs]'oP165U'SV_\"M@rX;O=URe&-,ML%L)~>";
    let mut pdf = format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n5 0 obj\n<< /Filter [ /ASCII85Decode /FlateDecode ] /Length {} >>\nstream\n{}\nendstream\nendobj\n",
        encoded_stream.len(),
        encoded_stream
    )
    .into_bytes();
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    pdf
}

fn utf16be_text_pdf(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xfe, 0xff];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    let literal = bytes
        .into_iter()
        .map(|byte| format!("\\{byte:03o}"))
        .collect::<String>();
    minimal_text_pdf(&literal)
}

fn table_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Name) Tj 150 0 Td (Value) Tj -150 -20 Td (Alpha) Tj 150 0 Td (42) Tj ET",
        "",
    )
}

fn fragmented_word_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 1 0 0 1 72 720 Tm (a) Tj (t) Tj (t) Tj (e) Tj (n) Tj (t) Tj (i) Tj (o) Tj (n) Tj ( listener) Tj (') Tj (s human) Tj (-) Tj (human) Tj ET",
        "",
    )
}

fn private_use_font_pdf() -> Vec<u8> {
    let cmap = "/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CMapType 2 def
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
12 beginbfrange
<0011><0011><f0f3>
<0014><0014><f0f0>
<0015><0015><f0ef>
<0016><0016><f0ee>
<0017><0017><f0ed>
<0019><0019><f0eb>
<001a><001a><f0ea>
<0025><0025><f0df>
<0026><0026><f0de>
<0028><0028><f0dc>
<002a><002a><f0da>
endbfrange
endcmap
CMapName currentdict /CMap defineresource pop
end
end";
    let content_stream =
        "BT /F1 12 Tf 72 720 Td <002A00280025001500160019001100170015001A00260014> Tj ET";
    let mut pdf = format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type0 /BaseFont /PrivateUse /Encoding /Identity-H /ToUnicode 6 0 R >>\nendobj\n5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n6 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
        content_stream.len(),
        content_stream,
        cmap.len(),
        cmap
    )
    .into_bytes();
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    pdf
}

fn bfrange_array_font_pdf() -> Vec<u8> {
    let cmap = "/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CMapType 2 def
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
1 beginbfrange
<0000> <0005> [<0041> <0076> <00650072> <0061> <0067> <0065>]
endbfrange
endcmap
CMapName currentdict /CMap defineresource pop
end
end";
    let content_stream = "BT /F1 12 Tf 72 720 Td <000000010002000300040005> Tj ET";
    let mut pdf = format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type0 /BaseFont /ArrayMap /Encoding /Identity-H /ToUnicode 6 0 R >>\nendobj\n5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n6 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
        content_stream.len(),
        content_stream,
        cmap.len(),
        cmap
    )
    .into_bytes();
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    pdf
}

fn heading_and_body_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 24 Tf 72 720 Td (Introduction Heading) Tj /F1 12 Tf 0 -50 Td (This is ordinary body paragraph text that clearly forms the bulk of the page content.) Tj ET",
        "",
    )
}

/// A Type0/Identity-H font whose glyph widths live only in the descendant
/// CIDFont `/W` array (CID 1 and 2 each 1000/1000 em). The two glyphs "A" and "B"
/// are positioned 12pt apart — exactly the advance of a 1000-em glyph at size 12 —
/// so they abut and read "AB" only when the `/W` widths are parsed. Without them,
/// the fallback width is narrower, leaving a gap that reads as "A B".
fn cidfont_w_widths_pdf() -> Vec<u8> {
    let cmap = "/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CMapType 2 def
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
2 beginbfchar
<0001> <0041>
<0002> <0042>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end";
    let content_stream = "BT /F1 12 Tf 72 720 Td <0001> Tj 12 0 Td <0002> Tj ET";
    let mut pdf = format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type0 /BaseFont /CIDTest /Encoding /Identity-H /DescendantFonts [7 0 R] /ToUnicode 6 0 R >>\nendobj\n5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n6 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n7 0 obj\n<< /Type /Font /Subtype /CIDFontType2 /BaseFont /CIDTest /DW 1000 /W [1 [1000 1000]] >>\nendobj\n",
        content_stream.len(),
        content_stream,
        cmap.len(),
        cmap
    )
    .into_bytes();
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    pdf
}

fn ligature_font_pdf() -> Vec<u8> {
    let cmap = "/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CMapType 2 def
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
7 beginbfchar
<0001><FB01>
<0002><006C>
<0003><0065>
<0004><FB02>
<0005><006F>
<0006><0077>
<0007><0020>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end";
    let content_stream =
        "BT /F1 12 Tf 72 720 Td <0001000200030007000400050006> Tj ET";
    let mut pdf = format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type0 /BaseFont /Ligature /Encoding /Identity-H /ToUnicode 6 0 R >>\nendobj\n5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n6 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
        content_stream.len(),
        content_stream,
        cmap.len(),
        cmap
    )
    .into_bytes();
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    pdf
}

fn font_metrics_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 6 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Metrics) Tj ET",
        "6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Custom /FontDescriptor 7 0 R >>\nendobj\n7 0 obj\n<< /Type /FontDescriptor /FontName /Custom /Flags 32 /Ascent 900 /Descent -300 >>\nendobj\n",
    )
}

fn rotated_page_pdf(rotate: i32) -> Vec<u8> {
    pdf_fixture(
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate {rotate} /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
        ),
        "BT /F1 12 Tf 72 720 Td (Rotated heading) Tj 0 -24 Td (Body text below it) Tj ET",
        "",
    )
}

fn bold_italic_pdf() -> Vec<u8> {
    let content_stream = "BT /F1 12 Tf 72 720 Td (Important warning) Tj /F2 12 Tf 0 -20 Td (Subtle aside note) Tj ET";
    let mut pdf = format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 6 0 R >> >> /Contents 5 0 R >>\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>\nendobj\n5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique >>\nendobj\n",
        content_stream.len(),
        content_stream
    )
    .into_bytes();
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    pdf
}

fn encoding_differences_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 6 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (a\\016ne \\010 \\006 \\012 don\\047t) Tj ET",
        "6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Custom /Encoding 7 0 R >>\nendobj\n7 0 obj\n<< /Type /Encoding /BaseEncoding /WinAnsiEncoding /Differences [ 6 /Sigma 8 /Phi 10 /Omega 14 /ffi 39 /quoteright ] >>\nendobj\n",
    )
}

fn scaled_text_matrix_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 2 0 0 2 72 720 Tm (Scaled) Tj ET",
        "",
    )
}

fn character_spacing_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 10 Tc 72 720 Td (A) Tj (B) Tj ET",
        "",
    )
}

fn font_widths_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 6 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (A) Tj (B) Tj ET",
        "6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Custom /FirstChar 65 /LastChar 66 /Widths [1000 200] >>\nendobj\n",
    )
}

fn text_rise_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (base) Tj 2 Ts ( super) Tj ET",
        "",
    )
}

fn script_geometry_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (x) Tj 3 Ts /F1 8 Tf (2) Tj 0 Ts /F1 12 Tf ( + y) Tj -3 Ts /F1 8 Tf (i) Tj 0 Ts /F1 12 Tf ( = z) Tj ET",
        "",
    )
}

fn offset_numeric_cells_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (10.615) Tj -3 Ts /F1 8 Tf (-11.607) Tj 0 Ts /F1 12 Tf ( 0.918) Tj ET",
        "",
    )
}

fn word_piece_spacing_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (coordina) Tj ( ting) Tj ( listener) Tj (’) Tj (s) Tj ( visual) Tj ( foc) Tj ( i) Tj ( work) Tj ( ,) Tj ( de) Tj ( scribe) Tj ( pro) Tj ( posed) Tj ET",
        "",
    )
}

fn column_word_fragments_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Participants in a c onversatio n coordinate with one an other.) Tj 0 -20 Td (The model ce nters on prod uction de mands.) Tj ET",
        "",
    )
}

fn hyphen_split_code_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (FDA125-) Tj ( 316B2) Tj ET",
        "",
    )
}

fn math_subscript_spacing_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (n = m) Tj ( 1) Tj ( + m) Tj ( 2) Tj ( + · · · + mℓ) Tj ET",
        "",
    )
}

fn math_tuple_ellipsis_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (( v, x 1),..., ( v, xs)) Tj ET",
        "",
    )
}

fn control_glyph_math_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (suf\\002cient \\002ne-tuning \\003oating 0 6 = λ_i ∈ Fq and \\003 = λ_1) Tj ET",
        "",
    )
}

fn nonprinting_controls_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (safe\\000 text \\024 ok \\002ne \\031after) Tj ET",
        "",
    )
}

fn windows_1252_controls_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Women\\222s group \\226 before \\223quoted\\224 \\205 done) Tj ET",
        "",
    )
}

fn math_arrows_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (·: G × X → X and τ \\050 g · x\\051 = g · τ \\050 x\\051) Tj ET",
        "",
    )
}

fn uppercase_greek_math_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Φ: Σ → Ω and ∑_i x_i) Tj ET",
        "",
    )
}

fn single_column_math_band_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (For some integer ℓ ≥ 1, let 0 6 = λ) Tj 234 0 Td (_i ∈ Fq and m_i ≥ 1) Tj ET",
        "",
    )
}

fn repeated_body_math_band_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Fix a basis) Tj 178 0 Td (i = 1, ...,) Tj 56 0 Td (smooth connected) Tj (divisor) Tj ET BT /F1 12 Tf 72 700 Td (One can choose) Tj 178 0 Td (a = λ,) Tj 56 0 Td (validation data) Tj (sample) Tj ET",
        "",
    )
}

fn wrapped_paragraph_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (This paragraph has a hyphen-) Tj 0 -14 Td (ated continuation and keeps) Tj 0 -14 Td (flowing) Tj ET",
        "",
    )
}

fn hyphenated_heading_continuation_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (The bro-) Tj 0 -14 Td (ken underline:) Tj 0 -14 Td (appears inside a paragraph) Tj ET",
        "",
    )
}

fn table_with_surrounding_text_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 750 Td (Results Summary) Tj 0 -30 Td (Name) Tj 150 0 Td (Value) Tj -150 -20 Td (Alpha) Tj 150 0 Td (42) Tj -150 -20 Td (Source note) Tj ET",
        "",
    )
}

fn wrapped_label_statement_pdf() -> Vec<u8> {
    let mut ops = Vec::new();
    // The opening row's label wraps: a long first line with no figures, then a
    // short tail ("period") that carries the figures, hanging-indented.
    for (text, x, y) in [
        ("STATEMENTS OF CASH FLOWS", 90.0, 760.0),
        ("2024", 337.8, 742.0),
        ("2023", 437.8, 742.0),
        ("Cash and cash equivalents and restricted cash, beginning of", 90.0, 726.0),
        ("period", 98.0, 712.0),
        ("12,345", 329.7, 712.0),
        ("11,000", 429.7, 712.0),
        ("Operating activities:", 90.0, 696.0),
        ("Net income", 98.0, 680.0),
        ("5,000", 335.3, 680.0),
        ("4,800", 435.3, 680.0),
        ("Depreciation", 98.0, 664.0),
        ("1,200", 335.3, 664.0),
        ("1,150", 435.3, 664.0),
        ("Deferred taxes", 98.0, 648.0),
        ("300", 343.3, 648.0),
        ("250", 443.3, 648.0),
        ("Inventories", 98.0, 632.0),
        ("(400)", 336.7, 632.0),
        ("150", 443.3, 632.0),
        ("Accounts payable", 98.0, 616.0),
        ("600", 343.3, 616.0),
        ("(200)", 436.7, 616.0),
        ("Investing activities:", 90.0, 600.0),
        ("Capital expenditures", 98.0, 584.0),
        ("(2,000)", 328.6, 584.0),
        ("(1,800)", 428.6, 584.0),
        ("Acquisitions", 98.0, 568.0),
        ("(500)", 336.7, 568.0),
        ("(300)", 436.7, 568.0),
        ("Net income", 98.0, 552.0),
        ("5,000", 335.3, 552.0),
        ("4,800", 435.3, 552.0),
        ("See notes.", 90.0, 536.0),
    ] {
        ops.push(format!("BT /F1 10 Tf 1 0 0 1 {x} {y} Tm ({text}) Tj ET"));
    }
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        &ops.join("\n"),
        "",
    )
}

fn multi_section_statement_pdf() -> Vec<u8> {
    let mut ops = Vec::new();
    // Realistic Helvetica (size 10) layout: right-aligned figures in two columns,
    // section-header rows interleaved between data rows.
    for (text, x, y) in [
        ("CONSOLIDATED STATEMENTS OF OPERATIONS", 90.0, 760.0),
        ("2024", 337.8, 742.0),
        ("2023", 437.8, 742.0),
        ("Operating activities:", 90.0, 726.0),
        ("Net sales", 90.0, 710.0),
        ("391,011", 324.1, 710.0),
        ("383,285", 424.1, 710.0),
        ("Cost of sales", 90.0, 694.0),
        ("210,352", 324.1, 694.0),
        ("214,137", 424.1, 694.0),
        ("Gross margin", 90.0, 678.0),
        ("180,659", 324.1, 678.0),
        ("169,148", 424.1, 678.0),
        ("Operating expenses:", 90.0, 662.0),
        ("Research and development", 90.0, 646.0),
        ("31,370", 329.7, 646.0),
        ("29,915", 429.7, 646.0),
        ("Selling and administrative", 90.0, 630.0),
        ("26,097", 329.7, 630.0),
        ("24,932", 429.7, 630.0),
        ("Total operating expenses", 90.0, 614.0),
        ("57,467", 329.7, 614.0),
        ("54,847", 429.7, 614.0),
        ("Operating income", 90.0, 598.0),
        ("123,192", 324.1, 598.0),
        ("114,301", 424.1, 598.0),
        ("Other income", 90.0, 582.0),
        ("269", 343.3, 582.0),
        ("(565)", 436.7, 582.0),
        ("Provision for income taxes", 90.0, 566.0),
        ("29,749", 329.7, 566.0),
        ("16,741", 429.7, 566.0),
        ("Net income", 90.0, 550.0),
        ("93,736", 329.7, 550.0),
        ("96,995", 429.7, 550.0),
        ("See accompanying notes.", 90.0, 526.0),
    ] {
        ops.push(format!("BT /F1 10 Tf 1 0 0 1 {x} {y} Tm ({text}) Tj ET"));
    }
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        &ops.join("\n"),
        "",
    )
}

fn implied_alignment_table_pdf() -> Vec<u8> {
    let mut ops = Vec::new();
    for (text, x, y) in [
        // Coordinates use realistic Helvetica (size 8) advances: tokens within a
        // cell are separated by a visible space and numeric columns are
        // right-aligned, the way a real renderer lays out this table. (Earlier
        // coordinates placed separate words at a zero gap, relying on the old
        // always-space assembly.)
        ("Table 1 Providers of immunisations", 80.0, 774.0),
        ("Total", 208.0, 758.0),
        ("No", 208.0, 751.0),
        ("Private", 250.0, 745.0),
        ("doctor", 278.5, 745.0),
        ("only", 250.0, 738.0),
        ("Local", 330.0, 745.0),
        ("council", 353.2, 745.0),
        ("only", 330.0, 738.0),
        ("Department", 412.0, 745.0),
        ("of", 456.0, 745.0),
        ("Health", 412.0, 738.0),
        ("only", 437.9, 738.0),
        ("More", 492.0, 745.0),
        ("than", 513.5, 745.0),
        ("one", 492.0, 738.0),
        ("None", 545.0, 745.0),
        ("Sydney", 80.0, 724.0),
        ("160", 212.7, 724.0),
        ("108", 258.4, 724.0),
        ("(68)", 275.8, 724.0),
        ("11", 337.3, 724.0),
        ("(7)", 350.2, 724.0),
        ("15", 417.3, 724.0),
        ("(9)", 430.2, 724.0),
        ("25", 492.9, 724.0),
        ("(16)", 505.8, 724.0),
        ("1", 539.7, 724.0),
        ("(0-6)", 548.1, 724.0),
        ("Elsewhere", 80.0, 714.0),
        ("44", 217.1, 714.0),
        ("28", 262.9, 714.0),
        ("(65)", 275.8, 714.0),
        ("1", 341.8, 714.0),
        ("(2)", 350.2, 714.0),
        ("9", 417.3, 714.0),
        ("(20)", 425.8, 714.0),
        ("4", 501.8, 714.0),
        ("(9)", 510.2, 714.0),
        ("2", 546.8, 714.0),
        ("(5)", 555.2, 714.0),
        ("Body paragraph after table", 80.0, 688.0),
    ] {
        ops.push(format!("BT /F1 8 Tf 1 0 0 1 {x} {y} Tm ({text}) Tj ET"));
    }
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        &ops.join("\n"),
        "",
    )
}

fn ruled_grid_table_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 772 Td (Table 1 Results) Tj ET
72 700 220 60 re S
182 700 m 182 760 l S
72 730 m 292 730 l S
BT /F1 12 Tf 90 742 Td (Description) Tj 104 0 Td (Result) Tj ET
BT /F1 12 Tf 110 712 Td (Alpha) Tj 126 0 Td (42) Tj ET",
        "",
    )
}

fn merged_header_grid_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 782 Td (Table 4 Overview) Tj ET
72 680 300 90 re S
172 680 m 172 770 l S
272 680 m 272 770 l S
72 710 m 372 710 l S
72 740 m 372 740 l S
BT /F1 12 Tf 95 750 Td (Item) Tj ET
BT /F1 12 Tf 180 750 Td (Measured Values Total) Tj ET
BT /F1 12 Tf 185 720 Td (Width) Tj ET
BT /F1 12 Tf 285 720 Td (Height) Tj ET
BT /F1 12 Tf 110 690 Td (A) Tj ET
BT /F1 12 Tf 200 690 Td (10) Tj ET
BT /F1 12 Tf 300 690 Td (20) Tj ET",
        "",
    )
}

fn unlabeled_ruled_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "72 700 220 60 re S
182 700 m 182 760 l S
72 730 m 292 730 l S
BT /F1 12 Tf 90 742 Td (Left heading) Tj 104 0 Td (Right heading) Tj ET
BT /F1 12 Tf 110 712 Td (Left body) Tj 104 0 Td (Right body) Tj ET",
        "",
    )
}

fn unlabeled_multirow_ruled_grid_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "72 620 420 100 re S
212 620 m 212 720 l S
72 695 m 492 695 l S
72 670 m 492 670 l S
72 645 m 492 645 l S
BT /F1 10 Tf 90 704 Td (Class) Tj 150 0 Td (Explanation) Tj ET
BT /F1 10 Tf 90 679 Td (Marine) Tj 150 0 Td (Cargo and hull) Tj ET
BT /F1 10 Tf 240 674 Td (Large events) Tj ET
BT /F1 10 Tf 90 654 Td (Property) Tj 150 0 Td (Direct risks) Tj ET
BT /F1 10 Tf 90 629 Td (Cyber) Tj 150 0 Td (Ransomware cover) Tj ET",
        "",
    )
}

fn numeric_multicolumn_body_pdf() -> Vec<u8> {
    let mut ops = Vec::new();
    for (text, x, y) in [
        ("Left body", 72.0, 720.0),
        ("2015", 170.0, 720.0),
        ("Right body", 330.0, 720.0),
        ("3.9 kg", 455.0, 720.0),
        ("Left continuation", 72.0, 700.0),
        ("2016", 170.0, 700.0),
        ("Right", 330.0, 700.0),
        ("continuation", 365.0, 700.0),
        ("5.0 kg", 455.0, 700.0),
    ] {
        ops.push(format!("BT /F1 12 Tf 1 0 0 1 {x} {y} Tm ({text}) Tj ET"));
    }
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        &ops.join("\n"),
        "",
    )
}

fn table_with_following_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 760 Td (Table 1) Tj 0 -20 Td (Name) Tj 150 0 Td (Value) Tj 120 0 Td (Count) Tj -270 -20 Td (Alpha) Tj 150 0 Td (42) Tj 120 0 Td (7) Tj -270 -20 Td (Beta) Tj 150 0 Td (43) Tj 120 0 Td (8) Tj -270 -70 Td (Left one) Tj 234 0 Td (Right one) Tj -234 -20 Td (Left two) Tj 234 0 Td (Right two) Tj ET",
        "",
    )
}

fn two_column_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Left one) Tj 0 -20 Td (Left two) Tj 258 10 Td (Right one) Tj 0 -20 Td (Right two) Tj ET",
        "",
    )
}

fn same_baseline_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Left one) Tj 234 0 Td (Right one) Tj -234 -20 Td (Left two) Tj 234 0 Td (Right two) Tj ET",
        "",
    )
}

fn moderate_gutter_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Left column content here) Tj 168 0 Td (Right one) Tj -168 -20 Td (Left column continues) Tj 168 0 Td (Right two) Tj ET",
        "",
    )
}

fn overlapping_gutter_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Left abstract phrase with listeners visual focus) Tj 234 0 Td (Right body starts) Tj -234 -20 Td (Left continuation) Tj 234 0 Td (Right follows) Tj ET",
        "",
    )
}

fn tight_band_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (demand is checked at) Tj ( the beginning) Tj ( of every) Tj 234 0 Td (gagement.) Tj ( Beyond the number of words) Tj ET BT /F1 12 Tf 72 700 Td (left column) Tj ( continues) Tj 234 0 Td (right column) Tj ( continues) Tj ET",
        "",
    )
}

fn math_left_column_band_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (1: T) Tj 20 0 Td (= λ) Tj 214 0 Td (Right prose begins) Tj ( additional words) Tj ET BT /F1 12 Tf 72 700 Td (2: i) Tj 20 0 Td (= arg max) Tj 214 0 Td (Right prose continues) Tj ( additional words) Tj ET",
        "",
    )
}

fn algorithm_single_right_run_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (1: T) Tj 20 0 Td (= λ) Tj 214 0 Td (a correlation-aware selection mechanism) Tj ET BT /F1 12 Tf 72 700 Td (2: while) Tj 40 0 Td ( l ≤ N do) Tj 174 0 Td (resolve coherence conflicts) Tj ET",
        "",
    )
}

fn formula_base_before_tight_band_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 740 Td (lower bound pruning strategies, MILP significantly reduces) Tj 234 0 Td (H) Tj 9 0 Td (T) Tj 18 0 Td (is formed by row vectors) Tj ET BT /F1 12 Tf 72 720 Td (1: T) Tj 20 0 Td (= λ) Tj 214 0 Td (right prose starts here) Tj ET BT /F1 12 Tf 72 700 Td (2: while) Tj 40 0 Td (l ≤ N do) Tj 194 0 Td (right prose continues here) Tj ET",
        "",
    )
}

fn front_matter_footnote_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 180 750 Td (Sample Title) Tj ET BT /F1 12 Tf 72 700 Td (Left Author) Tj 234 0 Td (Right Author) Tj ET BT /F1 12 Tf 160 650 Td (Abstract) Tj ET BT /F1 12 Tf 72 610 Td (Left body turns to meet) Tj 234 0 Td (the gaze of speaker) Tj ET BT /F1 12 Tf 72 590 Td (Left body continues) Tj 241 0 Td (right body follows) Tj ET BT /F1 9 Tf 72 72 Td (Footnote text) Tj ET",
        "",
    )
}

fn title_with_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 180 750 Td (Centered Title) Tj -108 -30 Td (Left one) Tj 234 0 Td (Right one) Tj -234 -20 Td (Left two) Tj 234 0 Td (Right two) Tj ET",
        "",
    )
}

fn wide_title_staggered_columns_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 120 750 Td (A Wide Centered Title Spanning Both Columns) Tj -48 -30 Td (Left paragraph hyphen-) Tj 234 -4 Td (Right first) Tj -234 -16 Td (ated continuation) Tj 234 -4 Td (Right second) Tj ET",
        "",
    )
}

fn wide_staggered_column_lines_pdf() -> Vec<u8> {
    pdf_fixture(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "BT /F1 12 Tf 72 720 Td (Left seed one) Tj 234 -4 Td (Right seed one) Tj -234 -16 Td (Left seed two) Tj 234 -4 Td (Right seed two) Tj -234 -46 Td (Left lower paragraph has a long hyphen-) Tj 234 -4 Td (Right lower first) Tj -234 -16 Td (ated continuation keeps flowing as text) Tj 234 -4 Td (Right lower second) Tj ET",
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

fn inherited_resources_pdf() -> Vec<u8> {
    let cmap = "/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CMapType 2 def
1 beginbfchar
<01> <0041>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end";
    let content_stream = "BT /F1 12 Tf 72 320 Td <01> Tj ET";
    let mut pdf = format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type0 /BaseFont /Test /ToUnicode 6 0 R >>\nendobj\n5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n6 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
        content_stream.len(),
        content_stream,
        cmap.len(),
        cmap
    )
    .into_bytes();
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    pdf
}

fn repeated_headers_footers_pdf() -> Vec<u8> {
    let page_objects = [
        (
            3,
            5,
            "BT /F1 12 Tf 72 760 Td (Report Header) Tj 0 -360 Td (Body one) Tj 0 -360 Td (Page 1) Tj ET",
        ),
        (
            6,
            7,
            "BT /F1 12 Tf 72 760 Td (Report Header) Tj 0 -360 Td (Body two) Tj 0 -360 Td (Page 2) Tj ET",
        ),
        (
            8,
            9,
            "BT /F1 12 Tf 72 760 Td (Report Header) Tj 0 -360 Td (Body three) Tj 0 -360 Td (Page 3) Tj ET",
        ),
    ];
    let mut pdf = "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R 6 0 R 8 0 R] /Count 3 >>\nendobj\n4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_owned();

    for (page_id, content_id, content_stream) in page_objects {
        pdf.push_str(&format!(
            "{page_id} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents {content_id} 0 R >>\nendobj\n{content_id} 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            content_stream.len(),
            content_stream
        ));
    }

    let mut bytes = pdf.into_bytes();
    bytes.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    bytes
}

fn png_fixture(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0]);
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes
}

fn tiff_fixture(width: u32, height: u32, little_endian: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if little_endian {
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        tiff_entry(&mut bytes, 256, width, true);
        tiff_entry(&mut bytes, 257, height, true);
        bytes.extend_from_slice(&0u32.to_le_bytes());
    } else {
        bytes.extend_from_slice(b"MM");
        bytes.extend_from_slice(&42u16.to_be_bytes());
        bytes.extend_from_slice(&8u32.to_be_bytes());
        bytes.extend_from_slice(&2u16.to_be_bytes());
        tiff_entry(&mut bytes, 256, width, false);
        tiff_entry(&mut bytes, 257, height, false);
        bytes.extend_from_slice(&0u32.to_be_bytes());
    }
    bytes
}

fn tiff_entry(bytes: &mut Vec<u8>, tag: u16, value: u32, little_endian: bool) {
    if little_endian {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    } else {
        bytes.extend_from_slice(&tag.to_be_bytes());
        bytes.extend_from_slice(&4u16.to_be_bytes());
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&value.to_be_bytes());
    }
}

fn tar_fixture(files: &[(&str, &str)]) -> Vec<u8> {
    let mut archive = Vec::new();
    for (name, contents) in files {
        let bytes = contents.as_bytes();
        let mut header = [0u8; 512];
        write_tar_bytes(&mut header[0..100], name.as_bytes());
        write_tar_octal(&mut header[100..108], 0o644);
        write_tar_octal(&mut header[108..116], 0);
        write_tar_octal(&mut header[116..124], 0);
        write_tar_octal(&mut header[124..136], bytes.len() as u64);
        write_tar_octal(&mut header[136..148], 0);
        for byte in &mut header[148..156] {
            *byte = b' ';
        }
        header[156] = b'0';
        write_tar_bytes(&mut header[257..263], b"ustar\0");
        write_tar_bytes(&mut header[263..265], b"00");
        let checksum = header.iter().map(|byte| u32::from(*byte)).sum::<u32>();
        write_tar_checksum(&mut header[148..156], checksum);
        archive.extend_from_slice(&header);
        archive.extend_from_slice(bytes);
        let padding = (512 - bytes.len() % 512) % 512;
        archive.extend(std::iter::repeat(0).take(padding));
    }
    archive.extend_from_slice(&[0u8; 1024]);
    archive
}

fn write_tar_bytes(target: &mut [u8], value: &[u8]) {
    let len = target.len().min(value.len());
    target[..len].copy_from_slice(&value[..len]);
}

fn write_tar_octal(target: &mut [u8], value: u64) {
    let text = format!("{value:0width$o}\0", width = target.len() - 1);
    write_tar_bytes(target, text.as_bytes());
}

fn write_tar_checksum(target: &mut [u8], value: u32) {
    let text = format!("{value:06o}\0 ",);
    write_tar_bytes(target, text.as_bytes());
}

fn zip_fixture(files: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = Vec::new();
    let mut central = Vec::new();

    for (name, contents) in files {
        let local_offset = zip.len() as u32;
        let name_bytes = name.as_bytes();
        let content_bytes = contents.as_bytes();
        let crc = crc32(content_bytes);

        zip.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        zip.extend_from_slice(&20u16.to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        zip.extend_from_slice(&crc.to_le_bytes());
        zip.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        zip.extend_from_slice(name_bytes);
        zip.extend_from_slice(content_bytes);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
        central.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&local_offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }

    let central_offset = zip.len() as u32;
    zip.extend_from_slice(&central);
    zip.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&(files.len() as u16).to_le_bytes());
    zip.extend_from_slice(&(files.len() as u16).to_le_bytes());
    zip.extend_from_slice(&(central.len() as u32).to_le_bytes());
    zip.extend_from_slice(&central_offset.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
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
