use crate::error::Result;
use crate::ir::{Block, Document, TableBlock};

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
                    Block::Text(text) => rendered_blocks.push(text.text.clone()),
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
                        output.push_str(&escape_latex(&text.text));
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
            _ => escaped.push(character),
        }
    }

    escaped
}
