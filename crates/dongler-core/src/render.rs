use crate::error::Result;
use crate::ir::{Block, Document, TableBlock, TextBlock};

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
                    Block::Text(text) => rendered_blocks.push(render_markdown_text(text)),
                    Block::Table(table) => rendered_blocks.push(render_markdown_table(table)),
                    Block::Figure(figure) => {
                        if let Some(caption) = &figure.caption {
                            rendered_blocks.push(caption.clone());
                        }
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
        let mut output = String::from("\\documentclass{article}\n\\begin{document}\n\n");

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
                        if let Some(caption) = &figure.caption {
                            output.push_str(&escape_latex(caption));
                            output.push_str("\n\n");
                        }
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
        return format!("{} {}", "#".repeat(level), text.text);
    }
    if text.kind == "list" {
        return text
            .text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| format!("- {}", line.trim()))
            .collect::<Vec<_>>()
            .join("\n");
    }
    text.text.clone()
}

fn render_markdown_table(table: &TableBlock) -> String {
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

fn markdown_row(cells: &[String]) -> String {
    format!(
        "| {} |",
        cells
            .iter()
            .map(|cell| cell.replace('|', "\\|"))
            .collect::<Vec<_>>()
            .join(" | ")
    )
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
    escape_latex(&text.text)
}

fn render_latex_table(table: &TableBlock) -> String {
    let width = table
        .headers
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or_default());

    if width == 0 {
        return String::new();
    }

    let mut output = format!("\\begin{{tabular}}{{{}}}\n", "l".repeat(width));
    if !table.headers.is_empty() {
        output.push_str(&latex_row(&normalize_row(&table.headers, width)));
        output.push_str("\\hline\n");
    }

    for row in &table.rows {
        output.push_str(&latex_row(&normalize_row(row, width)));
    }

    output.push_str("\\end{tabular}");
    output
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
