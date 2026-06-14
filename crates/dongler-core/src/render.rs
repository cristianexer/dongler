use crate::error::Result;
use crate::ir::{Block, BlockKind, Document, FigureBlock, TableBlock, TextBlock};

pub trait Renderer {
    fn render(&self, document: &Document) -> Result<String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownRenderer;

impl Renderer for MarkdownRenderer {
    fn render(&self, document: &Document) -> Result<String> {
        let mut rendered_blocks = Vec::new();

        for page in &document.pages {
            for block in &page.blocks {
                match block {
                    Block::Text(text) => {
                        // Page furniture (running headers/footers) is kept in the
                        // IR but excluded from default Markdown, per PRD §4.G.
                        if BlockKind::parse(&text.kind).is_page_furniture() {
                            continue;
                        }
                        rendered_blocks.push(render_markdown_text(text));
                    }
                    Block::Table(table) => rendered_blocks.push(render_markdown_table(table)),
                    Block::Figure(figure) => {
                        rendered_blocks.push(render_markdown_figure(figure));
                    }
                }
            }
        }

        Ok(rendered_blocks.join("\n\n"))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct JsonRenderer;

impl Renderer for JsonRenderer {
    fn render(&self, document: &Document) -> Result<String> {
        Ok(serde_json::to_string_pretty(document)?)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LatexRenderer;

impl Renderer for LatexRenderer {
    fn render(&self, document: &Document) -> Result<String> {
        let mut output = String::from(
            "\\documentclass{article}\n\\usepackage{longtable}\n\\begin{document}\n\n",
        );

        for page in &document.pages {
            for block in &page.blocks {
                match block {
                    Block::Text(text) => {
                        output.push_str(&render_latex_text(text));
                        output.push_str("\n\n");
                    }
                    Block::Table(table) => {
                        output.push_str(&render_latex_table(table));
                        output.push_str("\n\n");
                    }
                    Block::Figure(figure) => {
                        output.push_str(&render_latex_figure(figure));
                        output.push_str("\n\n");
                    }
                }
            }
        }

        output.push_str("\\end{document}\n");
        Ok(output)
    }
}

fn render_markdown_text(text: &TextBlock) -> String {
    if let Some(level) = heading_level(&text.kind) {
        return format!(
            "{} {}",
            "#".repeat(level),
            sanitize_markdown_text(&text.text)
        );
    }
    if text.kind == "list" {
        return text
            .text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| format!("- {}", sanitize_markdown_text(line.trim())))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let body = sanitize_markdown_text(&text.text);
    let (bold, italic) = block_emphasis(text);
    emphasize_markdown(&body, bold, italic)
}

/// Whether every non-blank span of a block is bold and/or italic, so the whole
/// block can be wrapped in emphasis markers without losing the cleaned text.
fn block_emphasis(block: &TextBlock) -> (bool, bool) {
    let mut any = false;
    let mut bold = true;
    let mut italic = true;
    for span in block.lines.iter().flat_map(|line| line.spans.iter()) {
        if span.text.trim().is_empty() {
            continue;
        }
        any = true;
        bold &= span.bold;
        italic &= span.italic;
    }
    if any {
        (bold, italic)
    } else {
        (false, false)
    }
}

fn emphasize_markdown(text: &str, bold: bool, italic: bool) -> String {
    let marker = match (bold, italic) {
        (true, true) => "***",
        (true, false) => "**",
        (false, true) => "*",
        (false, false) => return text.to_owned(),
    };
    if text.is_empty() {
        return text.to_owned();
    }
    format!("{marker}{text}{marker}")
}

fn emphasize_latex(text: &str, bold: bool, italic: bool) -> String {
    match (bold, italic) {
        (true, true) => format!("\\textbf{{\\textit{{{text}}}}}"),
        (true, false) => format!("\\textbf{{{text}}}"),
        (false, true) => format!("\\textit{{{text}}}"),
        (false, false) => text.to_owned(),
    }
}

fn render_markdown_table(table: &TableBlock) -> String {
    // PRD default: embed HTML tables so row/col spans survive. Prefer a
    // pre-rendered html string, then synthesize HTML from spanning cells; fall
    // back to a GFM pipe table for simple (span-free) tables.
    if let Some(html) = &table.html {
        let html = html.trim();
        if !html.is_empty() {
            return html.to_owned();
        }
    }
    if table.cells.iter().any(|c| c.col_span > 1 || c.row_span > 1) {
        if let Some(html) = render_html_table_from_cells(table) {
            return html;
        }
    }

    let width = table
        .headers
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or_default());

    if width == 0 {
        return String::new();
    }

    let headers = normalize_row(&table.headers, width);
    let separators = vec!["---".to_owned(); width];
    let rows = table
        .rows
        .iter()
        .map(|row| normalize_row(row, width))
        .collect::<Vec<_>>();

    let mut lines = Vec::with_capacity(rows.len() + 2);
    lines.push(markdown_row(&headers));
    lines.push(markdown_row(&separators));
    lines.extend(rows.iter().map(|row| markdown_row(row)));
    lines.join("\n")
}

/// Build an HTML `<table>` from explicit cells, preserving `colspan`/`rowspan`.
/// Spanned-over grid positions are omitted from `cells`, so each cell is emitted
/// once with its span attributes. Returns `None` if there are no cells.
fn render_html_table_from_cells(table: &TableBlock) -> Option<String> {
    html_table_from_cells(&table.cells, table.caption.as_deref())
}

/// Build an HTML `<table>` from explicit cells (with optional caption), preserving
/// `colspan`/`rowspan`. Public so pipeline stages that produce cells directly
/// (e.g. the SLANet table path) render identically to the Markdown renderer.
/// Returns `None` if `cells` is empty.
pub fn html_table_from_cells(cells: &[crate::ir::TableCell], caption: Option<&str>) -> Option<String> {
    if cells.is_empty() {
        return None;
    }
    let max_row = cells.iter().map(|c| c.row).max()?;
    let mut rows: Vec<Vec<&crate::ir::TableCell>> = vec![Vec::new(); max_row + 1];
    for cell in cells {
        if cell.row < rows.len() {
            rows[cell.row].push(cell);
        }
    }
    for row in &mut rows {
        row.sort_by_key(|c| c.column);
    }

    let mut html = String::from("<table>\n");
    if let Some(caption) = caption {
        let caption = caption.trim();
        if !caption.is_empty() {
            html.push_str(&format!("<caption>{}</caption>\n", html_escape(caption)));
        }
    }
    for row in &rows {
        html.push_str("<tr>");
        for cell in row {
            let tag = if cell.is_header { "th" } else { "td" };
            let mut attrs = String::new();
            if cell.col_span > 1 {
                attrs.push_str(&format!(" colspan=\"{}\"", cell.col_span));
            }
            if cell.row_span > 1 {
                attrs.push_str(&format!(" rowspan=\"{}\"", cell.row_span));
            }
            html.push_str(&format!(
                "<{tag}{attrs}>{}</{tag}>",
                html_escape(cell.text.trim())
            ));
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</table>");
    Some(html)
}

fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn render_markdown_figure(figure: &FigureBlock) -> String {
    let alt_text = figure
        .alt_text
        .as_deref()
        .or(figure.caption.as_deref())
        .or(figure.image_ref.as_deref())
        .unwrap_or("image");
    let image_ref = figure.image_ref.as_deref().unwrap_or("#image");
    let image = format!(
        "![{}]({})",
        sanitize_markdown_text(alt_text).replace(['[', ']'], ""),
        image_ref
    );
    if let Some(caption) = &figure.caption {
        let caption = sanitize_markdown_text(caption);
        if !caption.is_empty() && caption != alt_text {
            return format!("{image}\n\n{caption}");
        }
    }
    image
}

fn markdown_row(cells: &[String]) -> String {
    format!(
        "| {} |",
        cells
            .iter()
            .map(|cell| sanitize_markdown_text(cell).replace('|', "\\|"))
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

fn sanitize_markdown_text(text: &str) -> String {
    text.lines()
        .map(|line| {
            line.chars()
                .filter(|character| !is_non_printing_control(*character))
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_non_printing_control(character: char) -> bool {
    character.is_control() && !matches!(character, '\n' | '\r' | '\t')
}

fn normalize_row(row: &[String], width: usize) -> Vec<String> {
    let mut normalized = row.to_vec();
    normalized.resize(width, String::new());
    normalized
}

fn render_latex_text(text: &TextBlock) -> String {
    if let Some(level) = heading_level(&text.kind) {
        let command = match level {
            1 => "section",
            2 => "subsection",
            3 => "subsubsection",
            _ => "paragraph",
        };
        return format!("\\{command}{{{}}}", escape_latex(&text.text));
    }
    if text.kind == "list" {
        let items = text
            .text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| format!("\\item {}", escape_latex(line.trim())))
            .collect::<Vec<_>>();
        if !items.is_empty() {
            return format!("\\begin{{itemize}}\n{}\n\\end{{itemize}}", items.join("\n"));
        }
    }
    let body = escape_latex(&text.text);
    let (bold, italic) = block_emphasis(text);
    emphasize_latex(&body, bold, italic)
}

fn render_latex_table(table: &TableBlock) -> String {
    let width = table
        .headers
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or_default());

    if width == 0 {
        return String::new();
    }

    let spec = latex_column_spec(table, width);
    // A long statement (e.g. a full cash-flow) overruns a single `tabular` page;
    // `longtable` breaks across pages so the whole table is actually readable.
    let environment = if table.rows.len() > 24 {
        "longtable"
    } else {
        "tabular"
    };

    let mut output = format!("\\begin{{{environment}}}{{{spec}}}\n");
    if !table.headers.is_empty() {
        output.push_str(&latex_row(&normalize_row(&table.headers, width)));
        output.push_str("\\hline\n");
    }

    for row in &table.rows {
        output.push_str(&latex_row(&normalize_row(row, width)));
    }

    output.push_str(&format!("\\end{{{environment}}}"));
    output
}

/// LaTeX column spec: a column is right-aligned (`r`) when its body cells are
/// mostly figures (so a financial statement's number columns line up the way the
/// source does); everything else is left-aligned (`l`).
fn latex_column_spec(table: &TableBlock, width: usize) -> String {
    (0..width)
        .map(|column| {
            let (mut total, mut numeric) = (0usize, 0usize);
            for row in &table.rows {
                if let Some(cell) = row.get(column) {
                    let cell = cell.trim();
                    if cell.is_empty() {
                        continue;
                    }
                    total += 1;
                    if cell_is_numeric(cell) {
                        numeric += 1;
                    }
                }
            }
            if total > 0 && numeric * 2 >= total {
                'r'
            } else {
                'l'
            }
        })
        .collect()
}

/// A cell that reads as a figure — digits possibly wrapped in `$`, parentheses,
/// commas, a percent, a decimal point, or a dash placeholder.
fn cell_is_numeric(text: &str) -> bool {
    let mut digits = 0usize;
    for character in text.chars() {
        match character {
            '0'..='9' => digits += 1,
            '$' | '(' | ')' | ',' | '.' | '%' | '-' | '+' | ' ' | '\u{2014}' | '\u{2013}' => {}
            _ => return false,
        }
    }
    digits >= 1
}

fn render_latex_figure(figure: &FigureBlock) -> String {
    let label = figure
        .caption
        .as_deref()
        .or(figure.alt_text.as_deref())
        .or(figure.image_ref.as_deref())
        .unwrap_or("image");
    format!("[Image: {}]", escape_latex(label))
}

fn heading_level(kind: &str) -> Option<usize> {
    let level = kind.strip_prefix("heading_")?.parse::<usize>().ok()?;
    (1..=6).contains(&level).then_some(level)
}

fn latex_row(cells: &[String]) -> String {
    format!(
        "{} \\\\\n",
        cells
            .iter()
            .map(|cell| escape_latex(cell))
            .collect::<Vec<_>>()
            .join(" & ")
    )
}

fn escape_latex(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\textbackslash{}"),
            '&' => escaped.push_str("\\&"),
            '%' => escaped.push_str("\\%"),
            '$' => escaped.push_str("\\$"),
            '#' => escaped.push_str("\\#"),
            '_' => escaped.push_str("\\_"),
            '{' => escaped.push_str("\\{"),
            '}' => escaped.push_str("\\}"),
            '~' => escaped.push_str("\\textasciitilde{}"),
            '^' => escaped.push_str("\\textasciicircum{}"),
            '\n' => escaped.push('\n'),
            character if character.is_control() && character.is_whitespace() => escaped.push(' '),
            character if character.is_control() => {}
            character if !character.is_ascii() => {
                escaped.push_str(latex_unicode_ascii_fallback(character));
            }
            _ => escaped.push(character),
        }
    }

    escaped
}

fn latex_unicode_ascii_fallback(character: char) -> &'static str {
    match character {
        '\u{00a0}' => " ",
        '–' | '−' => "-",
        '—' => "---",
        '‘' | '’' | '‚' => "'",
        '“' | '”' | '„' => "\"",
        '•' => "*",
        '…' => "...",
        '×' => "x",
        '÷' => "/",
        '≤' => "<=",
        '≥' => ">=",
        '≠' => "!=",
        '±' => "+/-",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Metadata, Page, TableCell};

    fn cell(row: usize, column: usize, text: &str, col_span: usize, row_span: usize) -> TableCell {
        TableCell {
            row,
            column,
            text: text.to_owned(),
            bbox: None,
            is_header: row == 0,
            col_span,
            row_span,
        }
    }

    fn doc_with(blocks: Vec<Block>) -> Document {
        Document {
            schema_version: crate::ir::SCHEMA_VERSION.to_owned(),
            metadata: Metadata {
                format: "pdf".to_owned(),
                engine: "test".to_owned(),
                source: None,
                title: None,
                character_count: 0,
                word_count: 0,
                block_count: blocks.len(),
                file_size_bytes: None,
                pdf_version: None,
                encrypted: false,
            },
            pages: vec![Page {
                number: 1,
                blocks,
                ..Default::default()
            }],
            assets: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn prerendered_html_table_is_emitted_verbatim() {
        let table = TableBlock {
            html: Some("<table><tr><td>X</td></tr></table>".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            render_markdown_table(&table),
            "<table><tr><td>X</td></tr></table>"
        );
    }

    #[test]
    fn spanning_cells_render_as_html_with_span_attrs() {
        let table = TableBlock {
            cells: vec![
                cell(0, 0, "Header", 2, 1),
                cell(1, 0, "a", 1, 1),
                cell(1, 1, "b", 1, 1),
            ],
            ..Default::default()
        };
        let out = render_markdown_table(&table);
        assert!(out.starts_with("<table>"), "got: {out}");
        assert!(out.contains("colspan=\"2\""), "got: {out}");
        assert!(out.contains("<th colspan=\"2\">Header</th>"), "got: {out}");
        assert!(out.contains("<td>a</td>"), "got: {out}");
    }

    #[test]
    fn simple_table_without_spans_stays_pipe_markdown() {
        let table = TableBlock {
            headers: vec!["a".to_owned(), "b".to_owned()],
            rows: vec![vec!["1".to_owned(), "2".to_owned()]],
            ..Default::default()
        };
        let out = render_markdown_table(&table);
        assert!(out.contains("| a | b |"), "got: {out}");
        assert!(!out.contains("<table>"), "got: {out}");
    }

    #[test]
    fn html_escape_escapes_markup() {
        assert_eq!(html_escape("a < b & \"c\""), "a &lt; b &amp; &quot;c&quot;");
    }

    #[test]
    fn page_furniture_excluded_from_markdown() {
        let blocks = vec![
            Block::Text(TextBlock {
                text: "RUNNING HEADER".to_owned(),
                kind: "page_header".to_owned(),
                ..Default::default()
            }),
            Block::Text(TextBlock {
                text: "Body paragraph.".to_owned(),
                kind: "paragraph".to_owned(),
                ..Default::default()
            }),
        ];
        let md = MarkdownRenderer.render(&doc_with(blocks)).unwrap();
        assert!(md.contains("Body paragraph."));
        assert!(!md.contains("RUNNING HEADER"), "furniture leaked: {md}");
    }

    #[test]
    fn heading_kind_renders_with_hashes() {
        let blocks = vec![Block::Text(TextBlock {
            text: "Title".to_owned(),
            kind: "heading_2".to_owned(),
            ..Default::default()
        })];
        let md = MarkdownRenderer.render(&doc_with(blocks)).unwrap();
        assert_eq!(md.trim(), "## Title");
    }
}
