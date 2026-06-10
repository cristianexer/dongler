use crate::error::Result;
use crate::ir::{
    BBox, Block, Confidence, Document, Line, Metadata, Page, SourceAnchor, Span, TableBlock,
    TextBlock, SCHEMA_VERSION,
};
use crate::source::Source;

pub trait ExtractionEngine {
    fn name(&self) -> &'static str;
    fn extract(&self, source: &Source) -> Result<Document>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PlainTextEngine;

impl ExtractionEngine for PlainTextEngine {
    fn name(&self) -> &'static str {
        "plain-text"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        if let Some(document) = docbank_token_label_document(source, self.name()) {
            return Ok(document);
        }
        if let Some(document) = latex_document(source) {
            return Ok(document);
        }
        if let Some(document) = markdown_document(source) {
            return Ok(document);
        }
        text_document_from_paragraphs(source, self.name(), split_paragraphs(&source.content), None)
    }
}

const DOCBANK_EXTRACTION_METHOD: &str = "docbank_token_labels";
const LATEX_ENGINE_NAME: &str = "latex-native";
const LATEX_EXTRACTION_METHOD: &str = "latex_native";
const MARKDOWN_ENGINE_NAME: &str = "markdown-native";
const MARKDOWN_EXTRACTION_METHOD: &str = "markdown_native";

#[derive(Debug)]
struct DocBankToken {
    text: String,
    label: String,
    bbox: BBox,
}

#[derive(Debug)]
struct DocBankLine {
    label: String,
    y: f32,
    height: f32,
    tokens: Vec<DocBankToken>,
}

fn docbank_token_label_document(source: &Source, engine_name: &str) -> Option<Document> {
    let mut tokens = Vec::new();
    let mut non_empty_lines = 0usize;

    for line in source.content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        non_empty_lines += 1;
        if let Some(token) = docbank_token_from_line(line) {
            tokens.push(token);
        }
    }

    if tokens.is_empty() || tokens.len() != non_empty_lines {
        return None;
    }

    let blocks = docbank_lines(tokens)
        .into_iter()
        .filter_map(docbank_line_block)
        .collect::<Vec<_>>();
    if blocks.is_empty() {
        return None;
    }

    let page_bbox = inferred_text_block_bbox(&blocks);
    let plain_text = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    Some(Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title: None,
            character_count: plain_text.chars().count(),
            word_count: plain_text.split_whitespace().count(),
            block_count: blocks.len(),
            file_size_bytes: source.bytes.as_ref().map(|bytes| bytes.len() as u64),
            pdf_version: None,
            encrypted: false,
        },
        pages: vec![Page {
            number: 1,
            width: page_bbox.map(|bbox| bbox.width),
            height: page_bbox.map(|bbox| bbox.height),
            rotation: None,
            bbox: page_bbox,
            blocks,
            images: Vec::new(),
            assets: Vec::new(),
            warnings: Vec::new(),
        }],
        assets: Vec::new(),
        warnings: Vec::new(),
    })
}

fn docbank_token_from_line(line: &str) -> Option<DocBankToken> {
    let cells = line.split('\t').collect::<Vec<_>>();
    if cells.len() < 10 {
        return None;
    }
    let text = cells[0].trim();
    let label = cells[9].trim();
    if text.is_empty() || !is_docbank_label(label) {
        return None;
    }

    let x0 = cells[1].parse::<f32>().ok()?;
    let y0 = cells[2].parse::<f32>().ok()?;
    let x1 = cells[3].parse::<f32>().ok()?;
    let y1 = cells[4].parse::<f32>().ok()?;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }

    Some(DocBankToken {
        text: text.to_owned(),
        label: label.to_owned(),
        bbox: BBox {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        },
    })
}

fn is_docbank_label(label: &str) -> bool {
    matches!(
        label,
        "abstract"
            | "author"
            | "caption"
            | "date"
            | "equation"
            | "figure"
            | "footer"
            | "list"
            | "paragraph"
            | "reference"
            | "section"
            | "table"
            | "title"
    )
}

fn docbank_lines(tokens: Vec<DocBankToken>) -> Vec<DocBankLine> {
    let mut lines = Vec::new();

    for token in tokens {
        let same_line = lines
            .last()
            .map(|line: &DocBankLine| {
                line.label == token.label
                    && (line.y - token.bbox.y).abs() <= line.height.max(token.bbox.height).max(3.0)
            })
            .unwrap_or(false);
        if same_line {
            if let Some(line) = lines.last_mut() {
                line.height = line.height.max(token.bbox.height);
                line.tokens.push(token);
            }
        } else {
            lines.push(DocBankLine {
                label: token.label.clone(),
                y: token.bbox.y,
                height: token.bbox.height,
                tokens: vec![token],
            });
        }
    }

    lines
}

fn docbank_line_block(line: DocBankLine) -> Option<Block> {
    if line.tokens.is_empty() {
        return None;
    }

    let text = line
        .tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let bbox = bbox_union(line.tokens.iter().map(|token| token.bbox))?;
    let spans = line
        .tokens
        .iter()
        .map(|token| Span {
            text: token.text.clone(),
            bbox: Some(token.bbox),
            font: None,
            size: None,
            bold: false,
            italic: false,
        })
        .collect::<Vec<_>>();

    Some(Block::Text(TextBlock {
        text: text.clone(),
        kind: line.label,
        bbox: Some(bbox),
        lines: vec![Line {
            text,
            bbox: Some(bbox),
            spans,
        }],
        source_anchors: vec![SourceAnchor {
            page_number: 1,
            pdf_object_ids: Vec::new(),
            bbox: Some(bbox),
            extraction_method: DOCBANK_EXTRACTION_METHOD.to_owned(),
        }],
        confidence: Some(Confidence {
            score: 0.9,
            calibrated: false,
        }),
    }))
}

fn inferred_text_block_bbox(blocks: &[Block]) -> Option<BBox> {
    let mut max_x = 0.0f32;
    let mut max_y = 0.0f32;
    let mut has_bbox = false;
    for block in blocks {
        let Block::Text(text) = block else {
            continue;
        };
        let Some(bbox) = text.bbox else {
            continue;
        };
        has_bbox = true;
        max_x = max_x.max(bbox.x + bbox.width);
        max_y = max_y.max(bbox.y + bbox.height);
    }
    has_bbox.then_some(BBox {
        x: 0.0,
        y: 0.0,
        width: max_x,
        height: max_y,
    })
}

fn bbox_union(boxes: impl Iterator<Item = BBox>) -> Option<BBox> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut has_box = false;
    for bbox in boxes {
        has_box = true;
        min_x = min_x.min(bbox.x);
        min_y = min_y.min(bbox.y);
        max_x = max_x.max(bbox.x + bbox.width);
        max_y = max_y.max(bbox.y + bbox.height);
    }
    has_box.then_some(BBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn document_from_blocks(
    source: &Source,
    engine_name: &str,
    title: Option<String>,
    blocks: Vec<Block>,
) -> Option<Document> {
    if blocks.is_empty() {
        return None;
    }
    let plain_text = blocks
        .iter()
        .map(block_markdown_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    Some(Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title,
            character_count: plain_text.chars().count(),
            word_count: plain_text.split_whitespace().count(),
            block_count: blocks.len(),
            file_size_bytes: source.bytes.as_ref().map(|bytes| bytes.len() as u64),
            pdf_version: None,
            encrypted: false,
        },
        pages: vec![Page {
            number: 1,
            width: None,
            height: None,
            rotation: None,
            bbox: None,
            blocks,
            images: Vec::new(),
            assets: Vec::new(),
            warnings: Vec::new(),
        }],
        assets: Vec::new(),
        warnings: Vec::new(),
    })
}

fn latex_document(source: &Source) -> Option<Document> {
    if !is_latex_source(source) {
        return None;
    }

    let stripped = strip_latex_comments(&source.content);
    let title = latex_command_argument(&stripped, "title").map(|text| clean_latex_inline(&text));
    let body = latex_document_body(&stripped);
    let blocks = latex_blocks(body, title.clone());
    document_from_blocks(source, LATEX_ENGINE_NAME, title, blocks)
}

fn is_latex_source(source: &Source) -> bool {
    source
        .path
        .as_deref()
        .map(|path| {
            let path = path.to_ascii_lowercase();
            path.ends_with(".tex")
                || path.ends_with(".latex")
                || path.ends_with(".ltx")
                || path.ends_with(".tex.gz")
                || path.ends_with(".latex.gz")
                || path.ends_with(".ltx.gz")
        })
        .unwrap_or(false)
}

fn strip_latex_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for line in text.lines() {
        let mut escaped = false;
        for character in line.chars() {
            if character == '%' && !escaped {
                break;
            }
            escaped = character == '\\' && !escaped;
            if character != '\\' {
                escaped = false;
            }
            output.push(character);
        }
        output.push('\n');
    }
    output
}

fn latex_document_body(text: &str) -> &str {
    let Some(start) = text.find("\\begin{document}") else {
        return text;
    };
    let body_start = start + "\\begin{document}".len();
    let body = &text[body_start..];
    if let Some(end) = body.find("\\end{document}") {
        &body[..end]
    } else {
        body
    }
}

fn latex_blocks(body: &str, title: Option<String>) -> Vec<Block> {
    let lines = body.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    let mut index = 0usize;

    if let Some(title) = title.filter(|title| !title.is_empty()) {
        blocks.push(latex_text_block(title, "heading_1".to_owned()));
    }

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.is_empty() {
            flush_latex_paragraph(&mut blocks, &mut paragraph);
            index += 1;
            continue;
        }
        if is_latex_skip_line(trimmed) {
            flush_latex_paragraph(&mut blocks, &mut paragraph);
            index += 1;
            continue;
        }
        if let Some((level, text)) = latex_heading(trimmed) {
            flush_latex_paragraph(&mut blocks, &mut paragraph);
            blocks.push(latex_text_block(text, format!("heading_{level}")));
            index += 1;
            continue;
        }
        if contains_latex_begin(trimmed, "abstract") {
            flush_latex_paragraph(&mut blocks, &mut paragraph);
            let (environment, next_index) = collect_latex_environment(&lines, index, &["abstract"]);
            if let Some(abstract_text) = latex_environment_body(&environment, "abstract") {
                let text = clean_latex_inline(&abstract_text);
                if !text.is_empty() {
                    blocks.push(latex_text_block(text, "abstract".to_owned()));
                }
            }
            index = next_index;
            continue;
        }
        if contains_any_latex_begin(trimmed, &["itemize", "enumerate"]) {
            flush_latex_paragraph(&mut blocks, &mut paragraph);
            let (environment, next_index) =
                collect_latex_environment(&lines, index, &["itemize", "enumerate"]);
            if let Some(block) = latex_list_block(&environment) {
                blocks.push(block);
            }
            index = next_index;
            continue;
        }
        if contains_any_latex_begin(
            trimmed,
            &[
                "table",
                "table*",
                "tabular",
                "tabular*",
                "tabularx",
                "longtable",
                "array",
            ],
        ) {
            flush_latex_paragraph(&mut blocks, &mut paragraph);
            let (environment, next_index) = collect_latex_environment(
                &lines,
                index,
                &[
                    "table",
                    "table*",
                    "tabular",
                    "tabular*",
                    "tabularx",
                    "longtable",
                    "array",
                ],
            );
            if let Some(block) = latex_table_block(&environment) {
                blocks.push(block);
            }
            index = next_index;
            continue;
        }

        let text = clean_latex_inline(trimmed);
        if !text.is_empty() {
            paragraph.push(text);
        }
        index += 1;
    }

    flush_latex_paragraph(&mut blocks, &mut paragraph);
    blocks
}

fn flush_latex_paragraph(blocks: &mut Vec<Block>, paragraph: &mut Vec<String>) {
    if paragraph.is_empty() {
        return;
    }
    blocks.push(latex_text_block(
        paragraph.join(" "),
        "paragraph".to_owned(),
    ));
    paragraph.clear();
}

fn is_latex_skip_line(line: &str) -> bool {
    matches!(
        latex_command_name_at(line, 1).as_deref(),
        Some(
            "author"
                | "date"
                | "documentclass"
                | "end"
                | "input"
                | "include"
                | "label"
                | "maketitle"
                | "newcommand"
                | "renewcommand"
                | "bibliography"
                | "bibliographystyle"
                | "usepackage"
        )
    )
}

fn latex_heading(line: &str) -> Option<(usize, String)> {
    for (command, level) in [
        ("part", 1usize),
        ("chapter", 1),
        ("section", 1),
        ("subsection", 2),
        ("subsubsection", 3),
        ("paragraph", 4),
        ("subparagraph", 5),
    ] {
        if let Some(text) = latex_line_command_argument(line, command) {
            let text = clean_latex_inline(&text);
            if !text.is_empty() {
                return Some((level, text));
            }
        }
    }
    None
}

fn latex_line_command_argument(line: &str, command: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let marker = format!("\\{command}");
    if !trimmed.starts_with(&marker) {
        return None;
    }
    latex_command_argument(trimmed, command)
}

fn contains_any_latex_begin(line: &str, names: &[&str]) -> bool {
    names.iter().any(|name| contains_latex_begin(line, name))
}

fn contains_latex_begin(line: &str, name: &str) -> bool {
    line.contains(&format!("\\begin{{{name}}}"))
}

fn collect_latex_environment(lines: &[&str], index: usize, names: &[&str]) -> (String, usize) {
    let mut output = String::new();
    let mut next_index = index;
    while next_index < lines.len() {
        let line = lines[next_index];
        output.push_str(line);
        output.push('\n');
        next_index += 1;
        if names
            .iter()
            .any(|name| line.contains(&format!("\\end{{{name}}}")))
        {
            break;
        }
    }
    (output, next_index)
}

fn latex_list_block(environment: &str) -> Option<Block> {
    let body = latex_environment_body(environment, "itemize")
        .or_else(|| latex_environment_body(environment, "enumerate"))?;
    let items = latex_item_texts(&body);
    if items.is_empty() {
        return None;
    }
    Some(latex_text_block(items.join("\n"), "list".to_owned()))
}

fn latex_item_texts(body: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut search_start = 0usize;
    while let Some(relative_start) = body[search_start..].find("\\item") {
        let item_start = search_start + relative_start;
        let mut content_start = item_start + "\\item".len();
        content_start = skip_latex_whitespace(body, content_start);
        if body.as_bytes().get(content_start) == Some(&b'[') {
            content_start = skip_latex_optional_argument(body, content_start);
            content_start = skip_latex_whitespace(body, content_start);
        }
        let next_item = body[content_start..]
            .find("\\item")
            .map(|relative| content_start + relative)
            .unwrap_or(body.len());
        let text = clean_latex_inline(&body[content_start..next_item]);
        if !text.is_empty() {
            items.push(text);
        }
        search_start = next_item;
    }
    items
}

fn latex_table_block(environment: &str) -> Option<Block> {
    let caption =
        latex_command_argument(environment, "caption").map(|text| clean_latex_inline(&text));
    let body = latex_environment_body(environment, "tabular")
        .or_else(|| latex_environment_body(environment, "tabular*"))
        .or_else(|| latex_environment_body(environment, "tabularx"))
        .or_else(|| latex_environment_body(environment, "longtable"))
        .or_else(|| latex_environment_body(environment, "array"))?;

    let mut rows = split_latex_table_rows(&body)
        .into_iter()
        .filter_map(|row| latex_table_row(&row))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return None;
    }

    let headers = if rows.len() > 1 {
        rows.remove(0)
    } else {
        Vec::new()
    };

    Some(Block::Table(TableBlock {
        headers,
        rows,
        caption,
        bbox: None,
        cells: Vec::new(),
        source_anchors: vec![latex_source_anchor()],
        confidence: Some(latex_confidence()),
    }))
}

fn split_latex_table_rows(body: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut current = String::new();
    let bytes = body.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() {
        if bytes[pos] == b'\\' && bytes.get(pos + 1) == Some(&b'\\') {
            rows.push(current);
            current = String::new();
            pos += 2;
        } else {
            current.push(body[pos..].chars().next().unwrap());
            pos += body[pos..].chars().next().unwrap().len_utf8();
        }
    }
    if !current.trim().is_empty() {
        rows.push(current);
    }
    rows
}

fn latex_table_row(row: &str) -> Option<Vec<String>> {
    let row = strip_latex_table_rules(row);
    let cells = split_latex_cells(&row)
        .into_iter()
        .map(|cell| clean_latex_inline(&cell))
        .filter(|cell| !cell.is_empty())
        .collect::<Vec<_>>();
    if cells.is_empty() {
        None
    } else {
        Some(cells)
    }
}

fn strip_latex_table_rules(row: &str) -> String {
    let mut cleaned = row.to_owned();
    for command in [
        "\\hline",
        "\\toprule",
        "\\midrule",
        "\\bottomrule",
        "\\cmidrule",
        "\\cline",
    ] {
        cleaned = cleaned.replace(command, " ");
    }
    cleaned
}

fn split_latex_cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for character in row.chars() {
        if character == '&' && !escaped {
            cells.push(current);
            current = String::new();
        } else {
            escaped = character == '\\' && !escaped;
            if character != '\\' {
                escaped = false;
            }
            current.push(character);
        }
    }
    cells.push(current);
    cells
}

fn latex_environment_body(text: &str, name: &str) -> Option<String> {
    let marker = format!("\\begin{{{name}}}");
    let start = text.find(&marker)?;
    let mut body_start = start + marker.len();
    loop {
        body_start = skip_latex_whitespace(text, body_start);
        match text.as_bytes().get(body_start) {
            Some(b'[') => body_start = skip_latex_optional_argument(text, body_start),
            Some(b'{') => {
                let (_, end) = read_latex_braced_argument(text, body_start)?;
                body_start = end;
            }
            _ => break,
        }
    }
    let end_marker = format!("\\end{{{name}}}");
    let end = text[body_start..]
        .find(&end_marker)
        .map(|relative| body_start + relative)
        .unwrap_or(text.len());
    Some(text[body_start..end].to_owned())
}

fn latex_command_argument(text: &str, command: &str) -> Option<String> {
    let marker = format!("\\{command}");
    let mut search_start = 0usize;
    while let Some(relative_start) = text[search_start..].find(&marker) {
        let start = search_start + relative_start;
        let mut cursor = start + marker.len();
        if text[cursor..]
            .chars()
            .next()
            .map(|character| character.is_ascii_alphabetic())
            .unwrap_or(false)
        {
            search_start = cursor;
            continue;
        }
        if text.as_bytes().get(cursor) == Some(&b'*') {
            cursor += 1;
        }
        cursor = skip_latex_whitespace(text, cursor);
        if text.as_bytes().get(cursor) == Some(&b'[') {
            cursor = skip_latex_optional_argument(text, cursor);
            cursor = skip_latex_whitespace(text, cursor);
        }
        if text.as_bytes().get(cursor) == Some(&b'{') {
            let (argument, _) = read_latex_braced_argument(text, cursor)?;
            return Some(argument);
        }
        search_start = cursor.max(start + 1);
    }
    None
}

fn read_latex_braced_argument(text: &str, open: usize) -> Option<(String, usize)> {
    if text.as_bytes().get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut escaped = false;
    for (relative, character) in text[open..].char_indices() {
        let index = open + relative;
        if character == '{' && !escaped {
            depth += 1;
        } else if character == '}' && !escaped {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some((text[open + 1..index].to_owned(), index + 1));
            }
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    None
}

fn skip_latex_optional_argument(text: &str, open: usize) -> usize {
    if text.as_bytes().get(open) != Some(&b'[') {
        return open;
    }
    let mut escaped = false;
    for (relative, character) in text[open + 1..].char_indices() {
        if character == ']' && !escaped {
            return open + 1 + relative + 1;
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    open + 1
}

fn skip_latex_whitespace(text: &str, mut pos: usize) -> usize {
    while pos < text.len() && text.as_bytes()[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

fn clean_latex_inline(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pos = 0usize;
    while pos < text.len() {
        let character = text[pos..].chars().next().unwrap();
        if character == '\\' {
            let next_pos = pos + character.len_utf8();
            let Some(next_character) = text[next_pos..].chars().next() else {
                break;
            };
            if next_character == '\\' {
                output.push(' ');
                pos = next_pos + next_character.len_utf8();
                continue;
            }
            if matches!(
                next_character,
                '%' | '&' | '_' | '$' | '#' | '{' | '}' | '[' | ']'
            ) {
                output.push(next_character);
                pos = next_pos + next_character.len_utf8();
                continue;
            }
            let (name, after_name) = latex_command_name(text, next_pos);
            if name.is_empty() {
                pos = next_pos;
                continue;
            }
            let (replacement, after_command) =
                clean_latex_command_argument(text, &name, after_name);
            output.push_str(&replacement);
            pos = after_command;
            continue;
        }
        if matches!(character, '{' | '}' | '$') {
            pos += character.len_utf8();
            continue;
        }
        if character == '~' {
            output.push(' ');
        } else {
            output.push(character);
        }
        pos += character.len_utf8();
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_latex_command_argument(text: &str, name: &str, after_name: usize) -> (String, usize) {
    let mut cursor = skip_latex_whitespace(text, after_name);
    if text.as_bytes().get(cursor) == Some(&b'[') {
        cursor = skip_latex_optional_argument(text, cursor);
        cursor = skip_latex_whitespace(text, cursor);
    }

    if matches!(
        name,
        "label" | "pageref" | "ref" | "cite" | "citep" | "citet"
    ) {
        if text.as_bytes().get(cursor) == Some(&b'{') {
            let (_, end) = read_latex_braced_argument(text, cursor).unwrap_or_default();
            return (String::new(), end.max(cursor + 1));
        }
        return (String::new(), cursor);
    }

    if name == "href" {
        if text.as_bytes().get(cursor) == Some(&b'{') {
            let (_, first_end) = read_latex_braced_argument(text, cursor).unwrap_or_default();
            let second_start = skip_latex_whitespace(text, first_end);
            if text.as_bytes().get(second_start) == Some(&b'{') {
                if let Some((argument, end)) = read_latex_braced_argument(text, second_start) {
                    return (clean_latex_inline(&argument), end);
                }
            }
            return (String::new(), first_end.max(cursor + 1));
        }
    }

    if matches!(name, "multicolumn" | "multirow") {
        let mut arguments = Vec::new();
        for _ in 0..3 {
            cursor = skip_latex_whitespace(text, cursor);
            if text.as_bytes().get(cursor) != Some(&b'{') {
                break;
            }
            if let Some((argument, end)) = read_latex_braced_argument(text, cursor) {
                arguments.push(argument);
                cursor = end;
            }
        }
        return (
            arguments
                .last()
                .map(|argument| clean_latex_inline(argument))
                .unwrap_or_default(),
            cursor,
        );
    }

    if text.as_bytes().get(cursor) == Some(&b'{') {
        if let Some((argument, end)) = read_latex_braced_argument(text, cursor) {
            return (clean_latex_inline(&argument), end);
        }
    }

    let replacement = match name {
        "LaTeX" => "LaTeX",
        "TeX" => "TeX",
        "quad" | "qquad" | "enspace" | "thinspace" => " ",
        _ => "",
    };
    (replacement.to_owned(), cursor)
}

fn latex_command_name(text: &str, start: usize) -> (String, usize) {
    let mut end = start;
    for (relative, character) in text[start..].char_indices() {
        if !character.is_ascii_alphabetic() {
            break;
        }
        end = start + relative + character.len_utf8();
    }
    if end > start {
        return (text[start..end].to_owned(), end);
    }
    if let Some(character) = text[start..].chars().next() {
        let end = start + character.len_utf8();
        (character.to_string(), end)
    } else {
        (String::new(), start)
    }
}

fn latex_command_name_at(line: &str, start: usize) -> Option<String> {
    if !line.starts_with('\\') {
        return None;
    }
    let (name, _) = latex_command_name(line, start);
    (!name.is_empty()).then_some(name)
}

fn latex_text_block(text: String, kind: String) -> Block {
    Block::Text(TextBlock {
        text,
        kind,
        bbox: None,
        lines: Vec::new(),
        source_anchors: vec![latex_source_anchor()],
        confidence: Some(latex_confidence()),
    })
}

fn latex_source_anchor() -> SourceAnchor {
    SourceAnchor {
        page_number: 1,
        pdf_object_ids: Vec::new(),
        bbox: None,
        extraction_method: LATEX_EXTRACTION_METHOD.to_owned(),
    }
}

fn latex_confidence() -> Confidence {
    Confidence {
        score: 0.85,
        calibrated: false,
    }
}

fn markdown_document(source: &Source) -> Option<Document> {
    if !is_markdown_source(source) {
        return None;
    }

    let blocks = markdown_blocks(&source.content);
    document_from_blocks(source, MARKDOWN_ENGINE_NAME, None, blocks)
}

fn is_markdown_source(source: &Source) -> bool {
    source
        .path
        .as_deref()
        .map(|path| {
            let path = path.to_ascii_lowercase();
            path.ends_with(".md") || path.ends_with(".markdown")
        })
        .unwrap_or(false)
}

fn markdown_blocks(content: &str) -> Vec<Block> {
    let lines = content.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.is_empty() {
            flush_markdown_paragraph(&mut blocks, &mut paragraph);
            index += 1;
            continue;
        }
        if let Some((level, text)) = markdown_heading(trimmed) {
            flush_markdown_paragraph(&mut blocks, &mut paragraph);
            blocks.push(markdown_text_block(text, format!("heading_{level}")));
            index += 1;
            continue;
        }
        if is_markdown_table_start(&lines, index) {
            flush_markdown_paragraph(&mut blocks, &mut paragraph);
            let (table, next_index) = markdown_table_block(&lines, index);
            blocks.push(table);
            index = next_index;
            continue;
        }
        if is_markdown_list_item(trimmed) {
            flush_markdown_paragraph(&mut blocks, &mut paragraph);
            let (list, next_index) = markdown_list_block(&lines, index);
            blocks.push(list);
            index = next_index;
            continue;
        }

        paragraph.push(trimmed.to_owned());
        index += 1;
    }

    flush_markdown_paragraph(&mut blocks, &mut paragraph);
    blocks
}

fn flush_markdown_paragraph(blocks: &mut Vec<Block>, paragraph: &mut Vec<String>) {
    if paragraph.is_empty() {
        return;
    }
    blocks.push(markdown_text_block(
        paragraph.join(" "),
        "paragraph".to_owned(),
    ));
    paragraph.clear();
}

fn markdown_heading(line: &str) -> Option<(usize, String)> {
    let hashes = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let text = line.get(hashes..)?.trim();
    if text.is_empty() {
        return None;
    }
    Some((hashes, clean_markdown_inline(text)))
}

fn is_markdown_table_start(lines: &[&str], index: usize) -> bool {
    index + 1 < lines.len()
        && markdown_row_cells(lines[index]).len() >= 2
        && is_markdown_separator_row(lines[index + 1])
}

fn markdown_table_block(lines: &[&str], index: usize) -> (Block, usize) {
    let headers = markdown_row_cells(lines[index]);
    let mut rows = Vec::new();
    let mut next_index = index + 2;

    while next_index < lines.len() {
        let line = lines[next_index].trim();
        if line.is_empty() || !line.contains('|') {
            break;
        }
        let row = markdown_row_cells(line);
        if row.is_empty() {
            break;
        }
        rows.push(row);
        next_index += 1;
    }

    (
        Block::Table(TableBlock {
            headers,
            rows,
            caption: None,
            bbox: None,
            cells: Vec::new(),
            source_anchors: vec![markdown_source_anchor()],
            confidence: Some(markdown_confidence()),
        }),
        next_index,
    )
}

fn markdown_row_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim().trim_matches('|');
    trimmed
        .split('|')
        .map(|cell| clean_markdown_inline(cell.trim()))
        .collect::<Vec<_>>()
}

fn is_markdown_separator_row(line: &str) -> bool {
    let cells = line.trim().trim_matches('|').split('|').collect::<Vec<_>>();
    if cells.len() < 2 {
        return false;
    }
    cells.iter().all(|cell| {
        let cell = cell.trim();
        let cell = cell.trim_matches(':');
        !cell.is_empty() && cell.chars().all(|character| character == '-')
    })
}

fn is_markdown_list_item(line: &str) -> bool {
    markdown_list_text(line).is_some()
}

fn markdown_list_block(lines: &[&str], index: usize) -> (Block, usize) {
    let mut items = Vec::new();
    let mut next_index = index;
    while next_index < lines.len() {
        let trimmed = lines[next_index].trim();
        let Some(item) = markdown_list_text(trimmed) else {
            break;
        };
        items.push(item);
        next_index += 1;
    }
    (
        markdown_text_block(items.join("\n"), "list".to_owned()),
        next_index,
    )
}

fn markdown_list_text(line: &str) -> Option<String> {
    if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some(clean_markdown_inline(text));
    }
    let dot = line.find('.')?;
    if dot == 0
        || dot + 1 >= line.len()
        || !line[..dot]
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return None;
    }
    line[dot + 1..].strip_prefix(' ').map(clean_markdown_inline)
}

fn clean_markdown_inline(text: &str) -> String {
    text.trim()
        .trim_matches('`')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn markdown_text_block(text: String, kind: String) -> Block {
    Block::Text(TextBlock {
        text,
        kind,
        bbox: None,
        lines: Vec::new(),
        source_anchors: vec![markdown_source_anchor()],
        confidence: Some(markdown_confidence()),
    })
}

fn markdown_source_anchor() -> SourceAnchor {
    SourceAnchor {
        page_number: 1,
        pdf_object_ids: Vec::new(),
        bbox: None,
        extraction_method: MARKDOWN_EXTRACTION_METHOD.to_owned(),
    }
}

fn markdown_confidence() -> Confidence {
    Confidence {
        score: 0.9,
        calibrated: false,
    }
}

fn block_markdown_text(block: &Block) -> String {
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

pub(crate) fn split_paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_paragraph(&mut paragraphs, &mut current);
        } else {
            current.push(trimmed.to_owned());
        }
    }

    flush_paragraph(&mut paragraphs, &mut current);
    paragraphs
}

fn flush_paragraph(paragraphs: &mut Vec<String>, current: &mut Vec<String>) {
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
        current.clear();
    }
}

pub(crate) fn text_document_from_text(
    source: &Source,
    engine_name: &str,
    text: &str,
    title: Option<String>,
) -> Result<Document> {
    text_document_from_paragraphs(source, engine_name, split_paragraphs(text), title)
}

pub(crate) fn text_document_from_paragraphs(
    source: &Source,
    engine_name: &str,
    paragraphs: Vec<String>,
    title: Option<String>,
) -> Result<Document> {
    let blocks = paragraphs
        .into_iter()
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            Block::Text(TextBlock {
                text,
                kind: "paragraph".to_owned(),
                bbox: None,
                lines: Vec::new(),
                source_anchors: vec![SourceAnchor {
                    page_number: 1,
                    pdf_object_ids: Vec::new(),
                    bbox: None,
                    extraction_method: engine_name.to_owned(),
                }],
                confidence: Some(Confidence {
                    score: 0.9,
                    calibrated: false,
                }),
            })
        })
        .collect::<Vec<_>>();
    let plain_text = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title,
            character_count: plain_text.chars().count(),
            word_count: plain_text.split_whitespace().count(),
            block_count: blocks.len(),
            file_size_bytes: source.bytes.as_ref().map(|bytes| bytes.len() as u64),
            pdf_version: None,
            encrypted: false,
        },
        pages: vec![Page {
            number: 1,
            width: None,
            height: None,
            rotation: None,
            bbox: None,
            blocks,
            images: Vec::new(),
            assets: Vec::new(),
            warnings: Vec::new(),
        }],
        assets: Vec::new(),
        warnings: Vec::new(),
    })
}
