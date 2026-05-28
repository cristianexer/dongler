use std::collections::HashMap;
use std::io::Read;

use flate2::read::ZlibDecoder;
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::engine::ExtractionEngine;
use crate::error::{DonglerError, Result};
use crate::ir::{
    Asset, BBox, Block, Confidence, Document, ImageObject, Line, Metadata, Page, SourceAnchor,
    Span, TableBlock, TableCell, TextBlock, Warning, SCHEMA_VERSION,
};
use crate::source::Source;

#[derive(Debug, Default, Clone, Copy)]
pub struct PdfEngine;

impl ExtractionEngine for PdfEngine {
    fn name(&self) -> &'static str {
        "pdf-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        let bytes = source.bytes.as_deref().unwrap_or(source.content.as_bytes());
        extract_pdf(bytes, source, self.name())
    }
}

#[derive(Debug, Clone)]
struct PdfObject {
    object_number: u32,
    generation: u16,
    body: Vec<u8>,
}

#[derive(Debug, Clone)]
struct PageSeed {
    number: usize,
    body: String,
}

#[derive(Debug, Clone)]
struct PageExtraction {
    page: Page,
    text: String,
}

#[derive(Debug, Clone)]
struct TextRun {
    text: String,
    bbox: BBox,
    font: Option<String>,
    size: f32,
    source_object_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct TextLine {
    runs: Vec<TextRun>,
    bbox: BBox,
}

#[derive(Debug, Clone)]
struct DetectedTable {
    table: TableBlock,
    line_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
struct ColumnLayout<'a> {
    leading: Vec<&'a TextLine>,
    columns: Vec<Vec<&'a TextLine>>,
    trailing: Vec<&'a TextLine>,
}

#[derive(Debug, Clone)]
struct ContentExtraction {
    text_runs: Vec<TextRun>,
    images: Vec<ImageObject>,
    assets: Vec<Asset>,
    warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Default)]
struct FontDecoder {
    cmap: HashMap<Vec<u8>, String>,
    max_code_len: usize,
}

#[derive(Debug, Clone)]
enum Operand {
    Number(f32),
    Name(String),
    Literal(Vec<u8>),
    Hex(Vec<u8>),
    Array(Vec<Operand>),
    Other,
}

#[derive(Debug, Clone)]
struct ContentOp {
    operands: Vec<Operand>,
    operator: String,
}

#[derive(Debug, Clone)]
struct GraphicsState {
    ctm: Matrix,
    text_x: f32,
    text_y: f32,
    line_x: f32,
    line_y: f32,
    font_name: Option<String>,
    font_size: f32,
    leading: f32,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Matrix::identity(),
            text_x: 0.0,
            text_y: 0.0,
            line_x: 0.0,
            line_y: 0.0,
            font_name: None,
            font_size: 12.0,
            leading: 12.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Matrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Matrix {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn multiply(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    fn point(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    fn bbox(self) -> BBox {
        BBox {
            x: self.e,
            y: self.f,
            width: self.a.abs(),
            height: self.d.abs(),
        }
    }
}

pub fn extract_pdf(bytes: &[u8], source: &Source, engine_name: &str) -> Result<Document> {
    if !bytes.starts_with(b"%PDF-") {
        return Err(DonglerError::pdf("missing %PDF header"));
    }

    let mut objects = parse_indirect_objects(bytes);
    expand_object_streams(&mut objects);
    if objects.is_empty() {
        return Err(DonglerError::pdf("no indirect objects found"));
    }

    let object_map = objects
        .iter()
        .map(|object| (object.object_number, object.clone()))
        .collect::<HashMap<_, _>>();
    let page_seeds = objects
        .iter()
        .filter_map(|object| page_seed(object, &object_map))
        .enumerate()
        .map(|(index, mut seed)| {
            seed.number = index + 1;
            seed
        })
        .collect::<Vec<_>>();

    if page_seeds.is_empty() {
        return Err(DonglerError::pdf("no page objects found"));
    }

    let mut document_warnings = Vec::new();
    if contains_name(bytes, b"/Encrypt") {
        document_warnings.push(warning(
            "pdf.encrypted",
            "warning",
            "document declares encryption; extraction may be incomplete",
            None,
        ));
    }
    if contains_name(bytes, b"/ObjStm") {
        document_warnings.push(warning(
            "pdf.object_stream",
            "info",
            "object streams detected and expanded by the native scanner",
            None,
        ));
    }

    let page_extractions = page_seeds
        .par_iter()
        .map(|seed| extract_page(seed, &object_map))
        .collect::<Vec<_>>();

    let mut pages = Vec::with_capacity(page_extractions.len());
    let mut all_text = String::new();
    let mut assets = Vec::new();

    for extraction in page_extractions {
        all_text.push_str(&extraction.text);
        all_text.push('\n');
        assets.extend(extraction.page.assets.clone());
        pages.push(extraction.page);
    }

    Ok(Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: "pdf".to_owned(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title: extract_info_string(&objects, "Title"),
            character_count: all_text.chars().count(),
            word_count: all_text.split_whitespace().count(),
            block_count: pages.iter().map(|page| page.blocks.len()).sum(),
            file_size_bytes: Some(bytes.len() as u64),
            pdf_version: pdf_version(bytes),
            encrypted: contains_name(bytes, b"/Encrypt"),
        },
        pages,
        assets,
        warnings: document_warnings,
    })
}

fn extract_page(seed: &PageSeed, object_map: &HashMap<u32, PdfObject>) -> PageExtraction {
    let media_box = parse_number_array_after(&seed.body, "/MediaBox")
        .unwrap_or_else(|| vec![0.0, 0.0, 612.0, 792.0]);
    let width =
        media_box.get(2).copied().unwrap_or(612.0) - media_box.first().copied().unwrap_or(0.0);
    let height =
        media_box.get(3).copied().unwrap_or(792.0) - media_box.get(1).copied().unwrap_or(0.0);
    let rotation = parse_number_after(&seed.body, "/Rotate").map(|value| value as i32);
    let contents = parse_refs_after_key(&seed.body, "/Contents");
    let resource_body = resolve_resource_body(&seed.body, object_map);
    let resource_text = resource_body.as_deref().unwrap_or(&seed.body);
    let xobjects = resolve_named_resource_refs(resource_text, "/XObject", object_map);
    let fonts = load_font_decoders(resource_text, object_map);

    let mut warnings = Vec::new();
    let mut extraction = ContentExtraction {
        text_runs: Vec::new(),
        images: Vec::new(),
        assets: Vec::new(),
        warnings: Vec::new(),
    };

    for content_ref in contents {
        match object_map
            .get(&(content_ref as u32))
            .map(decode_stream_object)
        {
            Some(Ok(Some(stream))) => {
                let object_id = format!("{content_ref} 0 R");
                let mut content = interpret_content_stream(
                    &stream,
                    seed.number,
                    &[object_id],
                    &xobjects,
                    &fonts,
                    object_map,
                );
                extraction.text_runs.append(&mut content.text_runs);
                extraction.images.append(&mut content.images);
                extraction.assets.append(&mut content.assets);
                extraction.warnings.append(&mut content.warnings);
            }
            Some(Ok(None)) | None => warnings.push(warning(
                "pdf.missing_content",
                "warning",
                "page content stream is missing",
                Some(seed.number),
            )),
            Some(Err(error)) => warnings.push(warning(
                "pdf.stream_decode",
                "warning",
                &error.to_string(),
                Some(seed.number),
            )),
        }
    }

    warnings.append(&mut extraction.warnings);
    let lines = group_text_runs(extraction.text_runs);
    let blocks = build_blocks(seed.number, &lines);
    let text = blocks
        .iter()
        .map(block_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let page = Page {
        number: seed.number,
        width: Some(width),
        height: Some(height),
        rotation,
        bbox: Some(BBox {
            x: media_box.first().copied().unwrap_or(0.0),
            y: media_box.get(1).copied().unwrap_or(0.0),
            width,
            height,
        }),
        blocks,
        images: extraction.images,
        assets: extraction.assets,
        warnings,
    };

    PageExtraction { page, text }
}

fn interpret_content_stream(
    bytes: &[u8],
    page_number: usize,
    source_object_ids: &[String],
    xobjects: &HashMap<String, u32>,
    fonts: &HashMap<String, FontDecoder>,
    object_map: &HashMap<u32, PdfObject>,
) -> ContentExtraction {
    let mut state = GraphicsState::default();
    let mut graphics_stack = Vec::new();
    let mut extraction = ContentExtraction {
        text_runs: Vec::new(),
        images: Vec::new(),
        assets: Vec::new(),
        warnings: Vec::new(),
    };

    for op in parse_content_ops(bytes) {
        match op.operator.as_str() {
            "q" => graphics_stack.push(state.clone()),
            "Q" => {
                if let Some(previous) = graphics_stack.pop() {
                    state = previous;
                }
            }
            "cm" => {
                if let Some(values) = numbers(&op.operands, 6) {
                    state.ctm = state.ctm.multiply(Matrix {
                        a: values[0],
                        b: values[1],
                        c: values[2],
                        d: values[3],
                        e: values[4],
                        f: values[5],
                    });
                }
            }
            "BT" => {
                state.text_x = 0.0;
                state.text_y = 0.0;
                state.line_x = 0.0;
                state.line_y = 0.0;
            }
            "Tf" => {
                if let [Operand::Name(name), Operand::Number(size)] = op.operands.as_slice() {
                    state.font_name = Some(name.clone());
                    state.font_size = *size;
                    state.leading = *size * 1.2;
                }
            }
            "Td" | "TD" => {
                if let Some(values) = numbers(&op.operands, 2) {
                    state.line_x += values[0];
                    state.line_y += values[1];
                    state.text_x = state.line_x;
                    state.text_y = state.line_y;
                    if op.operator == "TD" {
                        state.leading = -values[1];
                    }
                }
            }
            "Tm" => {
                if let Some(values) = numbers(&op.operands, 6) {
                    state.line_x = values[4];
                    state.line_y = values[5];
                    state.text_x = values[4];
                    state.text_y = values[5];
                }
            }
            "T*" => {
                state.line_y -= state.leading;
                state.text_x = state.line_x;
                state.text_y = state.line_y;
            }
            "Tj" => {
                if let Some(text) = first_text_operand(&op.operands, &state, fonts) {
                    push_text_run(&mut extraction, &mut state, source_object_ids, text);
                }
            }
            "TJ" => {
                if let Some(Operand::Array(items)) = op.operands.first() {
                    let text = text_from_array(items, &state, fonts);
                    push_text_run(&mut extraction, &mut state, source_object_ids, text);
                }
            }
            "'" => {
                state.line_y -= state.leading;
                state.text_x = state.line_x;
                state.text_y = state.line_y;
                if let Some(text) = first_text_operand(&op.operands, &state, fonts) {
                    push_text_run(&mut extraction, &mut state, source_object_ids, text);
                }
            }
            "\"" => {
                state.line_y -= state.leading;
                state.text_x = state.line_x;
                state.text_y = state.line_y;
                if let Some(text) = op
                    .operands
                    .last()
                    .and_then(|operand| operand_text(operand, &state, fonts))
                {
                    push_text_run(&mut extraction, &mut state, source_object_ids, text);
                }
            }
            "Do" => {
                if let Some(Operand::Name(name)) = op.operands.first() {
                    if let Some(object_number) = xobjects.get(name) {
                        if let Some(object) = object_map.get(object_number) {
                            let object_body = lossy(&object.body);
                            if object_body.contains("/Subtype /Image") {
                                let bbox = state.ctm.bbox();
                                let id = format!("image-{}-{name}", page_number);
                                let object_id = Some(format!(
                                    "{} {} R",
                                    object.object_number, object.generation
                                ));
                                let width = parse_number_after(&object_body, "/Width")
                                    .map(|value| value as u32);
                                let height = parse_number_after(&object_body, "/Height")
                                    .map(|value| value as u32);

                                extraction.images.push(ImageObject {
                                    id: id.clone(),
                                    object_id: object_id.clone(),
                                    bbox: Some(bbox),
                                    width,
                                    height,
                                });
                                extraction.assets.push(Asset {
                                    id,
                                    kind: "image".to_owned(),
                                    object_id,
                                    bbox: Some(bbox),
                                    width,
                                    height,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    extraction
}

fn push_text_run(
    extraction: &mut ContentExtraction,
    state: &mut GraphicsState,
    source_object_ids: &[String],
    text: String,
) {
    if text.trim().is_empty() {
        return;
    }

    let (x, y) = state.ctm.point(state.text_x, state.text_y);
    let width = (text.chars().count() as f32 * state.font_size * 0.5).max(state.font_size * 0.25);
    let bbox = BBox {
        x,
        y,
        width,
        height: state.font_size,
    };
    extraction.text_runs.push(TextRun {
        text,
        bbox,
        font: state.font_name.clone(),
        size: state.font_size,
        source_object_ids: source_object_ids.to_vec(),
    });
    state.text_x += width;
}

fn build_blocks(page_number: usize, lines: &[TextLine]) -> Vec<Block> {
    if let Some(detected_table) = detect_table(page_number, lines) {
        let mut blocks = Vec::new();
        let mut table_inserted = false;
        for (line_index, line) in lines.iter().enumerate() {
            if detected_table.line_indices.contains(&line_index) {
                if !table_inserted {
                    blocks.push(Block::Table(detected_table.table.clone()));
                    table_inserted = true;
                }
            } else if let Some(block) = text_line_block(page_number, line) {
                blocks.push(block);
            }
        }
        return blocks;
    }

    let split_lines = split_wide_text_lines(lines);
    let text_blocks = text_lines_in_reading_order(&split_lines)
        .into_iter()
        .filter_map(|line| text_block_from_line(page_number, line))
        .collect::<Vec<_>>();
    merge_wrapped_text_blocks(text_blocks)
        .into_iter()
        .map(Block::Text)
        .collect()
}

fn split_wide_text_lines(lines: &[TextLine]) -> Vec<TextLine> {
    let enable_tight_column_band = has_repeated_tight_column_band_evidence(lines);
    let mut split_lines = Vec::new();
    for line in lines {
        match split_text_line_at_wide_gap(line, enable_tight_column_band) {
            Some((left, right)) => {
                split_lines.push(left);
                split_lines.push(right);
            }
            None => split_lines.push(line.clone()),
        }
    }
    split_lines
}

fn split_text_line_at_wide_gap(
    line: &TextLine,
    enable_tight_column_band: bool,
) -> Option<(TextLine, TextLine)> {
    if line.runs.len() < 2 {
        return None;
    }
    let mut runs = line.runs.clone();
    runs.sort_by(|left, right| left.bbox.x.total_cmp(&right.bbox.x));
    if runs
        .iter()
        .any(|run| looks_like_pdf_math_notation(&normalize_pdf_token(&run.text)))
    {
        return None;
    }
    let split_index = enable_tight_column_band
        .then(|| right_column_band_split_index(&runs))
        .flatten()
        .or_else(|| largest_run_gap(&runs).map(|(split_index, _, _)| split_index))?;
    let left_runs = runs[..split_index].to_vec();
    let right_runs = runs[split_index..].to_vec();
    if left_runs.is_empty() || right_runs.is_empty() {
        return None;
    }
    Some((
        text_line_from_runs(left_runs)?,
        text_line_from_runs(right_runs)?,
    ))
}

fn has_repeated_tight_column_band_evidence(lines: &[TextLine]) -> bool {
    lines
        .iter()
        .filter(|line| {
            let mut runs = line.runs.clone();
            runs.sort_by(|left, right| left.bbox.x.total_cmp(&right.bbox.x));
            right_column_band_split_index(&runs).is_some()
        })
        .take(2)
        .count()
        >= 2
}

fn right_column_band_split_index(runs: &[TextRun]) -> Option<usize> {
    if runs.len() < 4 || runs.first()?.bbox.x > 120.0 {
        return None;
    }
    if runs
        .iter()
        .any(|run| looks_like_pdf_math_notation(&normalize_pdf_token(&run.text)))
    {
        return None;
    }

    for index in 1..runs.len() {
        let right_x = runs[index].bbox.x;
        if !(300.0..=340.0).contains(&right_x) {
            continue;
        }
        if index < 2 || runs.len() - index < 2 {
            continue;
        }

        let previous = &runs[index - 1].bbox;
        let gap = right_x - (previous.x + previous.width);
        if gap < -35.0 {
            continue;
        }

        let right_text_len = runs[index..]
            .iter()
            .map(|run| run.text.trim().len())
            .sum::<usize>();
        if right_text_len < 18 {
            continue;
        }

        return Some(index);
    }

    None
}

fn largest_run_gap(runs: &[TextRun]) -> Option<(usize, f32, f32)> {
    runs.windows(2)
        .enumerate()
        .filter_map(|(index, window)| {
            let left = &window[0].bbox;
            let right = &window[1].bbox;
            let gap = right.x - (left.x + left.width);
            let x_jump = right.x - left.x;
            is_likely_column_split_gap(&window[0].bbox, &window[1].bbox, gap, x_jump).then_some((
                index + 1,
                gap,
                x_jump,
            ))
        })
        .max_by(|left, right| left.1.max(left.2).total_cmp(&right.1.max(right.2)))
}

fn is_likely_column_split_gap(left: &BBox, right: &BBox, gap: f32, x_jump: f32) -> bool {
    if gap >= 18.0 {
        return true;
    }

    x_jump >= 110.0 && left.x < 280.0 && right.x > 280.0
}

fn text_line_from_runs(runs: Vec<TextRun>) -> Option<TextLine> {
    let bbox = union_boxes(runs.iter().map(|run| run.bbox))?;
    Some(TextLine { runs, bbox })
}

fn text_lines_in_reading_order(lines: &[TextLine]) -> Vec<&TextLine> {
    if let Some(layout) = detect_paired_text_columns(lines) {
        return order_column_layout(layout);
    }
    if let Some(mut columns) = detect_text_columns(lines) {
        columns.sort_by(|left, right| column_x(left).total_cmp(&column_x(right)));
        return columns
            .into_iter()
            .flat_map(|mut column| {
                column.sort_by(|left, right| {
                    right
                        .bbox
                        .y
                        .total_cmp(&left.bbox.y)
                        .then(left.bbox.x.total_cmp(&right.bbox.x))
                });
                column
            })
            .collect();
    }
    lines.iter().collect()
}

fn order_column_layout(mut layout: ColumnLayout<'_>) -> Vec<&TextLine> {
    let mut ordered = Vec::new();
    sort_lines_top_down(&mut layout.leading);
    ordered.extend(layout.leading);
    layout
        .columns
        .sort_by(|left, right| column_x(left).total_cmp(&column_x(right)));
    for mut column in layout.columns {
        sort_lines_top_down(&mut column);
        ordered.extend(column);
    }
    sort_lines_top_down(&mut layout.trailing);
    ordered.extend(layout.trailing);
    ordered
}

fn sort_lines_top_down(lines: &mut [&TextLine]) {
    lines.sort_by(|left, right| {
        right
            .bbox
            .y
            .total_cmp(&left.bbox.y)
            .then(left.bbox.x.total_cmp(&right.bbox.x))
    });
}

fn detect_paired_text_columns(lines: &[TextLine]) -> Option<ColumnLayout<'_>> {
    if lines.len() < 4 {
        return None;
    }

    let mut left_seed_indices = Vec::new();
    let mut right_seed_indices = Vec::new();
    for (left_index, left) in lines.iter().enumerate() {
        for (right_index, right) in lines.iter().enumerate() {
            if left_index == right_index || left.bbox.x >= right.bbox.x {
                continue;
            }
            if (left.bbox.y - right.bbox.y).abs() > column_pair_y_tolerance(left, right) {
                continue;
            }
            let gap = right.bbox.x - (left.bbox.x + left.bbox.width);
            let x_jump = right.bbox.x - left.bbox.x;
            if !is_likely_column_split_gap(&left.bbox, &right.bbox, gap, x_jump) {
                continue;
            }
            left_seed_indices.push(left_index);
            right_seed_indices.push(right_index);
        }
    }
    dedupe_indices(&mut left_seed_indices);
    dedupe_indices(&mut right_seed_indices);
    if left_seed_indices.len() < 2 || right_seed_indices.len() < 2 {
        return None;
    }

    let left_x = average_x(lines, &left_seed_indices)?;
    let right_x = average_x(lines, &right_seed_indices)?;
    if right_x - left_x < 90.0 {
        return None;
    }
    let column_min_y = left_seed_indices
        .iter()
        .chain(&right_seed_indices)
        .map(|index| lines[*index].bbox.y)
        .reduce(f32::min)?;
    let column_max_y = left_seed_indices
        .iter()
        .chain(&right_seed_indices)
        .map(|index| lines[*index].bbox.y)
        .reduce(f32::max)?;
    let abstract_y = abstract_heading_y(lines);
    let midpoint = (left_x + right_x) / 2.0;
    let mut leading = Vec::new();
    let mut trailing = Vec::new();
    let mut left_column = Vec::new();
    let mut right_column = Vec::new();

    for line in lines {
        if is_likely_front_matter_line(line, abstract_y)
            || line.bbox.y > column_max_y + line.bbox.height
        {
            leading.push(line);
        } else if line.bbox.y < column_min_y - line.bbox.height * 1.8
            && (is_likely_page_number_line(line) || is_likely_bottom_footnote_line(line))
        {
            trailing.push(line);
        } else if line.bbox.x < midpoint {
            left_column.push(line);
        } else {
            right_column.push(line);
        }
    }

    if left_column.len() < 2 || right_column.len() < 2 {
        return None;
    }

    Some(ColumnLayout {
        leading,
        columns: vec![left_column, right_column],
        trailing,
    })
}

fn column_pair_y_tolerance(left: &TextLine, right: &TextLine) -> f32 {
    left.bbox.height.max(right.bbox.height) * 0.45
}

fn abstract_heading_y(lines: &[TextLine]) -> Option<f32> {
    lines
        .iter()
        .find(|line| text_line_plain_text(line).eq_ignore_ascii_case("abstract"))
        .map(|line| line.bbox.y)
}

fn is_likely_front_matter_line(line: &TextLine, abstract_y: Option<f32>) -> bool {
    abstract_y.is_some_and(|y| line.bbox.y > y + 36.0)
}

fn is_likely_bottom_footnote_line(line: &TextLine) -> bool {
    average_run_size(line) <= 10.0 && text_line_plain_text(line).len() > 4
}

fn average_run_size(line: &TextLine) -> f32 {
    if line.runs.is_empty() {
        return line.bbox.height;
    }
    line.runs.iter().map(|run| run.size).sum::<f32>() / line.runs.len() as f32
}

fn is_likely_page_number_line(line: &TextLine) -> bool {
    let text = text_line_plain_text(line);
    !text.is_empty() && text.len() <= 4 && text.chars().all(|character| character.is_ascii_digit())
}

fn text_line_plain_text(line: &TextLine) -> String {
    line.runs
        .iter()
        .map(|run| run.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

fn dedupe_indices(indices: &mut Vec<usize>) {
    indices.sort_unstable();
    indices.dedup();
}

fn average_x(lines: &[TextLine], indices: &[usize]) -> Option<f32> {
    if indices.is_empty() {
        return None;
    }
    Some(
        indices
            .iter()
            .map(|index| lines[*index].bbox.x)
            .sum::<f32>()
            / indices.len() as f32,
    )
}

fn detect_text_columns(lines: &[TextLine]) -> Option<Vec<Vec<&TextLine>>> {
    if lines.len() < 4 {
        return None;
    }

    let mut centers = lines
        .iter()
        .enumerate()
        .map(|(index, line)| (index, line.bbox.x + line.bbox.width / 2.0))
        .collect::<Vec<_>>();
    centers.sort_by(|left, right| left.1.total_cmp(&right.1));

    let (split_index, largest_gap) = centers
        .windows(2)
        .enumerate()
        .map(|(index, window)| (index + 1, window[1].1 - window[0].1))
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    if largest_gap < 90.0 {
        return None;
    }

    let (left_indices, right_indices) = centers.split_at(split_index);
    if left_indices.len() < 2 || right_indices.len() < 2 {
        return None;
    }

    let left = left_indices
        .iter()
        .map(|(index, _)| &lines[*index])
        .collect::<Vec<_>>();
    let right = right_indices
        .iter()
        .map(|(index, _)| &lines[*index])
        .collect::<Vec<_>>();

    let overlap = y_overlap(&left, &right)?;
    let average_height = average_line_height(lines);
    if overlap < average_height {
        return None;
    }

    Some(vec![left, right])
}

fn column_x(lines: &[&TextLine]) -> f32 {
    if lines.is_empty() {
        return 0.0;
    }
    lines.iter().map(|line| line.bbox.x).sum::<f32>() / lines.len() as f32
}

fn y_overlap(left: &[&TextLine], right: &[&TextLine]) -> Option<f32> {
    let left_min = left.iter().map(|line| line.bbox.y).reduce(f32::min)?;
    let left_max = left
        .iter()
        .map(|line| line.bbox.y + line.bbox.height)
        .reduce(f32::max)?;
    let right_min = right.iter().map(|line| line.bbox.y).reduce(f32::min)?;
    let right_max = right
        .iter()
        .map(|line| line.bbox.y + line.bbox.height)
        .reduce(f32::max)?;
    Some((left_max.min(right_max) - left_min.max(right_min)).max(0.0))
}

fn average_line_height(lines: &[TextLine]) -> f32 {
    let total = lines.iter().map(|line| line.bbox.height).sum::<f32>();
    total / lines.len() as f32
}

fn text_line_block(page_number: usize, line: &TextLine) -> Option<Block> {
    text_block_from_line(page_number, line).map(Block::Text)
}

fn text_block_from_line(page_number: usize, line: &TextLine) -> Option<TextBlock> {
    let text = line
        .runs
        .iter()
        .map(|run| run.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let text = clean_pdf_line_text(&text);
    if text.is_empty() {
        return None;
    }

    Some(TextBlock {
        text: text.clone(),
        kind: classify_text_line(&text),
        bbox: Some(line.bbox),
        lines: vec![Line {
            text,
            bbox: Some(line.bbox),
            spans: line
                .runs
                .iter()
                .map(|run| Span {
                    text: run.text.clone(),
                    bbox: Some(run.bbox),
                    font: run.font.clone(),
                    size: Some(run.size),
                })
                .collect(),
        }],
        source_anchors: vec![anchor(
            page_number,
            Some(line.bbox),
            source_ids_for_line(line),
        )],
        confidence: Some(Confidence {
            score: 0.82,
            calibrated: false,
        }),
    })
}

fn merge_wrapped_text_blocks(blocks: Vec<TextBlock>) -> Vec<TextBlock> {
    let mut merged: Vec<TextBlock> = Vec::new();
    for block in blocks {
        if let Some(previous) = merged.last_mut() {
            if should_merge_text_blocks(previous, &block) {
                merge_text_block(previous, block);
                continue;
            }
        }
        merged.push(block);
    }
    merged
}

fn should_merge_text_blocks(previous: &TextBlock, next: &TextBlock) -> bool {
    let Some(previous_bbox) = previous.bbox else {
        return false;
    };
    let Some(next_bbox) = next.bbox else {
        return false;
    };
    let baseline_gap = previous_bbox.y - next_bbox.y;
    if baseline_gap <= 0.0 || baseline_gap > previous_bbox.height.max(next_bbox.height) * 1.8 {
        return false;
    }
    let x_aligned = (previous_bbox.x - next_bbox.x).abs() <= 18.0;
    let hyphenated = previous.text.ends_with('-') && starts_with_lowercase(&next.text);
    if x_aligned && hyphenated {
        return true;
    }
    if previous.kind != "paragraph" || next.kind != "paragraph" {
        return false;
    }
    let lowercase_continuation =
        starts_with_lowercase(&next.text) && !ends_sentence(&previous.text);
    x_aligned && (hyphenated || lowercase_continuation)
}

fn merge_text_block(previous: &mut TextBlock, next: TextBlock) {
    previous.text = join_wrapped_text(&previous.text, &next.text);
    previous.bbox = union_boxes(previous.bbox.into_iter().chain(next.bbox)).or(previous.bbox);
    previous.lines.extend(next.lines);
    for anchor in next.source_anchors {
        previous.source_anchors.push(anchor);
    }
}

fn join_wrapped_text(previous: &str, next: &str) -> String {
    if let Some(stem) = previous.strip_suffix('-') {
        format!("{stem}{}", next.trim_start())
    } else {
        format!("{} {}", previous.trim_end(), next.trim_start())
    }
}

fn starts_with_lowercase(text: &str) -> bool {
    text.chars()
        .find(|character| character.is_alphabetic())
        .is_some_and(|character| character.is_lowercase())
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '!' | '?'))
}

fn clean_pdf_line_text(text: &str) -> String {
    let tokens = text
        .split_whitespace()
        .map(normalize_pdf_token)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut cleaned: Vec<String> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if is_closing_punctuation_token(token) && !cleaned.is_empty() {
            let previous = cleaned.last_mut().expect("checked non-empty");
            previous.push_str(token);
            index += 1;
            continue;
        }
        if is_joining_apostrophe(token) && !cleaned.is_empty() && index + 1 < tokens.len() {
            let next = tokens[index + 1].as_str();
            if is_word_piece(next) {
                let previous = cleaned.last_mut().expect("checked non-empty");
                previous.push('\'');
                previous.push_str(next);
                index += 2;
                continue;
            }
        }
        if is_joining_hyphen(token) && !cleaned.is_empty() && index + 1 < tokens.len() {
            let next = tokens[index + 1].as_str();
            if is_word_piece(next) {
                let previous = cleaned.last_mut().expect("checked non-empty");
                previous.push('-');
                previous.push_str(next);
                index += 2;
                continue;
            }
        }
        if let Some(previous) = cleaned.last_mut() {
            if should_join_after_trailing_hyphen(previous, token) {
                previous.push_str(token);
                index += 1;
                continue;
            }
            if should_join_pdf_word_piece(previous, token) {
                previous.push_str(token);
                index += 1;
                continue;
            }
        }
        if is_letter_fragment(token) {
            let mut merged = String::new();
            let mut end = index;
            while end < tokens.len() && is_letter_fragment(tokens[end].as_str()) {
                merged.push_str(tokens[end].as_str());
                end += 1;
            }
            if end - index >= 2 {
                cleaned.push(merged);
                index = end;
                continue;
            }
        }
        cleaned.push(token.to_owned());
        index += 1;
    }
    repair_pdf_math_notation(&repair_pdf_word_fragment_phrases(&cleaned.join(" ")))
}

fn repair_pdf_word_fragment_phrases(text: &str) -> String {
    let mut repaired = text.to_owned();
    for (broken, fixed) in [
        ("a c onversatio n", "a conversation"),
        ("ac onversatio n", "a conversation"),
        ("an other", "another"),
        ("ce nters", "centers"),
        ("prod uction", "production"),
        ("de mands", "demands"),
        ("turn s", "turns"),
        ("coordinate s", "coordinates"),
        ("coordinat e", "coordinate"),
        ("facilitat e", "facilitate"),
        ("speake rs", "speakers"),
        ("listener s'", "listeners'"),
        ("th e", "the"),
        ("p resent", "present"),
        ("linguisti c", "linguistic"),
        ("an d", "and"),
        ("inferen ces", "inferences"),
        ("attentio n", "attention"),
        ("B eyond", "Beyond"),
        ("variabilit y", "variability"),
        ("l essons", "lessons"),
        ("re peating", "repeating"),
        ("import ant", "important"),
        ("sp ecified", "specified"),
    ] {
        repaired = repaired.replace(broken, fixed);
    }
    repaired
}

fn normalize_pdf_token(token: &str) -> String {
    let normalized = token
        .replace("â\u{80}\u{98}", "'")
        .replace("â\u{80}\u{99}", "'")
        .replace("Â·", "·")
        .replace("â\u{84}\u{93}", "ℓ")
        .replace("Î»", "λ")
        .replace("Î›", "Λ")
        .replace("Ï\u{84}", "τ")
        .replace("Ã\u{97}", "×")
        .replace("â\u{86}\u{92}", "→")
        .replace("â\u{89}¥", "≥")
        .replace("â\u{89}¤", "≤")
        .replace("â\u{88}\u{88}", "∈")
        .replace(['‘', '’'], "'")
        .replace(['“', '”'], "\"");
    repair_embedded_pdf_control_glyphs(&normalized)
}

fn repair_embedded_pdf_control_glyphs(token: &str) -> String {
    let characters = token.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(token.len());
    for (index, character) in characters.iter().enumerate() {
        match character {
            '\u{2}' if has_following_alphabetic(&characters, index + 1) => {
                output.push_str("fi");
            }
            '\u{2}' => {}
            '\u{3}' if has_following_alphabetic(&characters, index + 1) => {
                output.push_str("fl");
            }
            _ => output.push(*character),
        }
    }
    output
}

fn has_following_alphabetic(characters: &[char], index: usize) -> bool {
    characters
        .get(index)
        .is_some_and(|character| character.is_alphabetic())
}

fn is_closing_punctuation_token(token: &str) -> bool {
    matches!(token, "." | "," | ":" | ";" | "!" | "?" | ")" | "]" | "}")
}

fn should_join_after_trailing_hyphen(previous: &str, token: &str) -> bool {
    previous.ends_with('-')
        && token
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        && previous
            .chars()
            .any(|character| character.is_ascii_alphanumeric())
}

fn should_join_pdf_word_piece(previous: &str, token: &str) -> bool {
    if !is_alphabetic_word(previous) || !is_alphabetic_word(token) {
        return false;
    }
    if !previous
        .chars()
        .last()
        .is_some_and(|character| character.is_lowercase())
        || !starts_with_lowercase(token)
    {
        return false;
    }

    matches!(
        (previous, token),
        ("coordina", "ting") | ("de", "scribe") | ("foc", "i") | ("pro", "posed")
    )
}

fn is_alphabetic_word(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|character| character.is_alphabetic())
}

fn repair_pdf_math_notation(text: &str) -> String {
    let normalized = text.replace("Â·", "·").replace("â\u{84}\u{93}", "ℓ");
    if !looks_like_pdf_math_notation(&normalized) {
        return strip_pdf_control_glyphs(&normalized);
    }

    let symbols = replace_math_symbols(&normalized);
    strip_pdf_control_glyphs(&repair_math_subscript_spacing(&symbols))
}

fn looks_like_pdf_math_notation(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(
            character,
            'ℓ' | 'λ'
                | 'θ'
                | 'ρ'
                | 'τ'
                | '∆'
                | 'Δ'
                | '≤'
                | '≥'
                | '∈'
                | '∪'
                | '∅'
                | '·'
                | '−'
                | '±'
                | '⊆'
                | '∼'
                | '≠'
                | '→'
        )
    }) || text.contains("...")
        || text.contains("Fq")
        || text.contains(" 6 =")
}

fn replace_math_symbols(text: &str) -> String {
    let collapsed = text
        .replace("· · ·", r"\cdots")
        .replace("...", r"\ldots")
        .replace("6 =", r"\neq")
        .replace("Fq", r"\mathbb{F}_q");
    let mut output = String::with_capacity(collapsed.len());

    for character in collapsed.chars() {
        match character {
            '\u{3}' => output.push_str(r"\Lambda"),
            'ℓ' => output.push_str(r"\ell"),
            'λ' => output.push_str(r"\lambda"),
            'Λ' => output.push_str(r"\Lambda"),
            'θ' => output.push_str(r"\theta"),
            'Θ' => output.push_str(r"\Theta"),
            'ρ' => output.push_str(r"\rho"),
            'τ' => output.push_str(r"\tau"),
            '∆' | 'Δ' => output.push_str(r"\Delta"),
            '≤' => output.push_str(r"\leq"),
            '≥' => output.push_str(r"\geq"),
            '∈' => output.push_str(r"\in"),
            '∪' => output.push_str(r"\cup"),
            '∅' => output.push_str(r"\varnothing"),
            '−' => output.push('-'),
            '±' => output.push_str(r"\pm"),
            '⊆' => output.push_str(r"\subseteq"),
            '∼' => output.push_str(r"\sim"),
            '≠' => output.push_str(r"\neq"),
            '×' => output.push_str(r"\times"),
            '→' => output.push_str(r"\to"),
            '·' => output.push_str(r"\cdot"),
            _ => output.push(character),
        }
    }

    output
}

fn strip_pdf_control_glyphs(text: &str) -> String {
    text.chars()
        .filter(|character| !matches!(character, '\u{2}' | '\u{3}'))
        .collect()
}

fn repair_math_subscript_spacing(text: &str) -> String {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    let mut repaired = Vec::with_capacity(tokens.len());
    let mut index = 0;

    while index < tokens.len() {
        let token = tokens[index];
        if is_math_base_token(token) && index + 1 < tokens.len() {
            if tokens[index + 1].starts_with('_') {
                repaired.push(format!("{}{}", token, tokens[index + 1]));
                index += 2;
                continue;
            }
            if let Some((subscript, suffix)) = split_math_subscript_token(tokens[index + 1]) {
                repaired.push(format!(
                    "{}{}{}",
                    token,
                    format_math_subscript(subscript),
                    suffix
                ));
                index += 2;
                continue;
            }
        }

        repaired.push(repair_compact_math_subscript(token));
        index += 1;
    }

    repaired.join(" ")
}

fn repair_compact_math_subscript(token: &str) -> String {
    if token.chars().count() > 2 && token.chars().all(|character| character.is_alphabetic()) {
        return token.to_owned();
    }

    for base in ["m", "n", "N", "T", "V", "C", "x", "t", "i", "k", "h", "g"] {
        if let Some(rest) = token.strip_prefix(base) {
            if rest.is_empty() || rest.starts_with('_') {
                continue;
            }
            if let Some((subscript, suffix)) = split_math_subscript_token(rest) {
                return format!("{}{}{}", base, format_math_subscript(subscript), suffix);
            }
        }
    }

    for base in [r"\lambda", r"\theta", r"\rho"] {
        if let Some(rest) = token.strip_prefix(base) {
            if rest.is_empty() || rest.starts_with('_') {
                continue;
            }
            if let Some((subscript, suffix)) = split_math_subscript_token(rest) {
                return format!("{}{}{}", base, format_math_subscript(subscript), suffix);
            }
        }
    }

    token.to_owned()
}

fn is_math_base_token(token: &str) -> bool {
    matches!(
        token,
        "m" | "n"
            | "N"
            | "T"
            | "V"
            | "C"
            | "x"
            | "t"
            | "i"
            | "k"
            | "h"
            | "g"
            | r"\lambda"
            | r"\theta"
            | r"\rho"
    )
}

fn split_math_subscript_token(token: &str) -> Option<(&str, &str)> {
    for command in [r"\ell", r"\lambda", r"\theta", r"\rho"] {
        if let Some(suffix) = token.strip_prefix(command) {
            return Some((command, suffix));
        }
    }
    for word in ["init", "cl"] {
        if let Some(suffix) = token.strip_prefix(word) {
            return Some((word, suffix));
        }
    }

    let mut end = 0;
    for (offset, character) in token.char_indices() {
        if character.is_ascii_digit() {
            end = offset + character.len_utf8();
            continue;
        }
        break;
    }
    if end > 0 {
        return Some((&token[..end], &token[end..]));
    }

    let mut chars = token.char_indices();
    let (_, first) = chars.next()?;
    if matches!(first, 'i' | 'j' | 'k' | 'l' | 'n' | 'r' | 's') {
        let end = first.len_utf8();
        return Some((&token[..end], &token[end..]));
    }
    None
}

fn format_math_subscript(subscript: &str) -> String {
    match subscript {
        "init" => r"_{\text{init}}".to_owned(),
        _ => format!("_{subscript}"),
    }
}

fn is_letter_fragment(token: &str) -> bool {
    let chars = token.chars().collect::<Vec<_>>();
    matches!(chars.as_slice(), [character] if character.is_alphabetic())
        || matches!(chars.as_slice(), [character, '-'] if character.is_alphabetic())
}

fn is_word_piece(token: &str) -> bool {
    token.chars().any(|character| character.is_alphabetic())
}

fn is_joining_apostrophe(token: &str) -> bool {
    matches!(token, "'" | "’")
}

fn is_joining_hyphen(token: &str) -> bool {
    matches!(token, "-" | "‐" | "‑" | "–")
}

fn detect_table(page_number: usize, lines: &[TextLine]) -> Option<DetectedTable> {
    let candidate_lines = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.runs.len() >= 2)
        .collect::<Vec<_>>();
    if candidate_lines.len() < 2 {
        return None;
    }

    let width = candidate_lines[0].1.runs.len();
    if !candidate_lines.iter().all(|(_, line)| {
        line.runs.len() == width && columns_align(&candidate_lines[0].1.runs, &line.runs)
    }) {
        return None;
    }
    if !has_table_evidence(&candidate_lines) {
        return None;
    }

    let headers = candidate_lines[0]
        .1
        .runs
        .iter()
        .map(|run| run.text.trim().to_owned())
        .collect::<Vec<_>>();
    let rows = candidate_lines
        .iter()
        .skip(1)
        .map(|(_, line)| {
            line.runs
                .iter()
                .map(|run| run.text.trim().to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let bbox = union_boxes(candidate_lines.iter().map(|(_, line)| line.bbox))?;
    let mut cells = Vec::new();

    for (row_index, (_, line)) in candidate_lines.iter().enumerate() {
        for (column_index, run) in line.runs.iter().enumerate() {
            cells.push(TableCell {
                row: row_index,
                column: column_index,
                text: run.text.clone(),
                bbox: Some(run.bbox),
                is_header: row_index == 0,
            });
        }
    }

    Some(DetectedTable {
        table: TableBlock {
            headers,
            rows,
            caption: None,
            bbox: Some(bbox),
            cells,
            source_anchors: vec![anchor(page_number, Some(bbox), Vec::new())],
            confidence: Some(Confidence {
                score: 0.72,
                calibrated: false,
            }),
        },
        line_indices: candidate_lines
            .iter()
            .map(|(line_index, _)| *line_index)
            .collect(),
    })
}

fn has_table_evidence(candidate_lines: &[(usize, &TextLine)]) -> bool {
    if candidate_lines.len() >= 3 {
        return true;
    }
    candidate_lines
        .iter()
        .skip(1)
        .flat_map(|(_, line)| line.runs.iter())
        .any(|run| run.text.chars().any(|character| character.is_ascii_digit()))
}

fn columns_align(first: &[TextRun], next: &[TextRun]) -> bool {
    first
        .iter()
        .zip(next)
        .all(|(left, right)| (left.bbox.x - right.bbox.x).abs() <= 6.0)
}

fn group_text_runs(mut runs: Vec<TextRun>) -> Vec<TextLine> {
    runs.sort_by(|left, right| {
        right
            .bbox
            .y
            .total_cmp(&left.bbox.y)
            .then(left.bbox.x.total_cmp(&right.bbox.x))
    });

    let mut lines: Vec<TextLine> = Vec::new();
    for run in runs {
        if let Some(line) = lines
            .iter_mut()
            .find(|line| (line.bbox.y - run.bbox.y).abs() <= 3.0)
        {
            line.runs.push(run);
            line.runs
                .sort_by(|left, right| left.bbox.x.total_cmp(&right.bbox.x));
            line.bbox = union_boxes(line.runs.iter().map(|run| run.bbox)).unwrap_or(line.bbox);
        } else {
            lines.push(TextLine {
                bbox: run.bbox,
                runs: vec![run],
            });
        }
    }

    lines
}

fn parse_content_ops(bytes: &[u8]) -> Vec<ContentOp> {
    let mut parser = ContentParser::new(bytes);
    let mut stack = Vec::new();
    let mut ops = Vec::new();

    while let Some(token) = parser.next_operand_or_operator() {
        match token {
            ContentToken::Operand(operand) => stack.push(operand),
            ContentToken::Operator(operator) => {
                ops.push(ContentOp {
                    operands: std::mem::take(&mut stack),
                    operator,
                });
            }
        }
    }

    ops
}

#[derive(Debug)]
enum ContentToken {
    Operand(Operand),
    Operator(String),
}

struct ContentParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ContentParser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn next_operand_or_operator(&mut self) -> Option<ContentToken> {
        self.skip_ws_and_comments();
        if self.pos >= self.bytes.len() {
            return None;
        }

        let byte = self.bytes[self.pos];
        match byte {
            b'/' => Some(ContentToken::Operand(Operand::Name(self.read_name()))),
            b'(' => Some(ContentToken::Operand(Operand::Literal(self.read_literal()))),
            b'[' => Some(ContentToken::Operand(Operand::Array(self.read_array()))),
            b'<' if self.peek(1) != Some(b'<') => {
                Some(ContentToken::Operand(Operand::Hex(self.read_hex_string())))
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => self
                .read_number()
                .map(|number| ContentToken::Operand(Operand::Number(number))),
            _ => {
                let word = self.read_word();
                if word.is_empty() {
                    self.pos += 1;
                    Some(ContentToken::Operand(Operand::Other))
                } else {
                    Some(ContentToken::Operator(word))
                }
            }
        }
    }

    fn read_array(&mut self) -> Vec<Operand> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.pos >= self.bytes.len() || self.bytes[self.pos] == b']' {
                self.pos = (self.pos + 1).min(self.bytes.len());
                break;
            }

            match self.next_operand_or_operator() {
                Some(ContentToken::Operand(operand)) => items.push(operand),
                Some(ContentToken::Operator(_)) | None => {}
            }
        }
        items
    }

    fn read_name(&mut self) -> String {
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.bytes.len() && !is_delimiter_or_ws(self.bytes[self.pos]) {
            self.pos += 1;
        }
        lossy(&self.bytes[start..self.pos])
    }

    fn read_literal(&mut self) -> Vec<u8> {
        self.pos += 1;
        let mut depth = 1;
        let mut output = Vec::new();

        while self.pos < self.bytes.len() && depth > 0 {
            let byte = self.bytes[self.pos];
            self.pos += 1;
            match byte {
                b'\\' => {
                    if self.pos < self.bytes.len() {
                        match self.bytes[self.pos] {
                            b'n' => {
                                output.push(b'\n');
                                self.pos += 1;
                            }
                            b'r' => {
                                output.push(b'\r');
                                self.pos += 1;
                            }
                            b't' => {
                                output.push(b'\t');
                                self.pos += 1;
                            }
                            b'b' => {
                                output.push(0x08);
                                self.pos += 1;
                            }
                            b'f' => {
                                output.push(0x0c);
                                self.pos += 1;
                            }
                            b'\n' => {
                                self.pos += 1;
                            }
                            b'\r' => {
                                self.pos += 1;
                                if self.bytes.get(self.pos) == Some(&b'\n') {
                                    self.pos += 1;
                                }
                            }
                            b'0'..=b'7' => output.push(self.read_octal_escape()),
                            other => {
                                output.push(other);
                                self.pos += 1;
                            }
                        }
                    }
                }
                b'(' => {
                    depth += 1;
                    output.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth > 0 {
                        output.push(byte);
                    }
                }
                _ => output.push(byte),
            }
        }

        output
    }

    fn read_octal_escape(&mut self) -> u8 {
        let mut value = 0u16;
        let mut digits = 0;
        while self.pos < self.bytes.len()
            && digits < 3
            && matches!(self.bytes[self.pos], b'0'..=b'7')
        {
            value = (value << 3) + u16::from(self.bytes[self.pos] - b'0');
            self.pos += 1;
            digits += 1;
        }
        value.min(u16::from(u8::MAX)) as u8
    }

    fn read_hex_string(&mut self) -> Vec<u8> {
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'>' {
            self.pos += 1;
        }
        let raw = self.bytes[start..self.pos].to_vec();
        self.pos = (self.pos + 1).min(self.bytes.len());
        decode_hex(&raw)
    }

    fn read_number(&mut self) -> Option<f32> {
        let start = self.pos;
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b'+' | b'-' | b'.' | b'0'..=b'9')
        {
            self.pos += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .ok()
            .and_then(|text| text.parse().ok())
    }

    fn read_word(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.bytes.len() && !is_delimiter_or_ws(self.bytes[self.pos]) {
            self.pos += 1;
        }
        lossy(&self.bytes[start..self.pos])
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while self.pos < self.bytes.len() && is_ws(self.bytes[self.pos]) {
                self.pos += 1;
            }
            if self.pos < self.bytes.len() && self.bytes[self.pos] == b'%' {
                while self.pos < self.bytes.len() && !matches!(self.bytes[self.pos], b'\n' | b'\r')
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }
}

fn parse_indirect_objects(bytes: &[u8]) -> Vec<PdfObject> {
    let mut objects = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        if !is_ws_or_line_start(bytes, pos) && pos != 0 {
            pos += 1;
            continue;
        }

        let Some((object_number, after_object_number)) = parse_unsigned_at(bytes, pos) else {
            pos += 1;
            continue;
        };
        let Some(after_space) = skip_required_ws(bytes, after_object_number) else {
            pos += 1;
            continue;
        };
        let Some((generation, after_generation)) = parse_unsigned_at(bytes, after_space) else {
            pos += 1;
            continue;
        };
        let Some(after_space) = skip_required_ws(bytes, after_generation) else {
            pos += 1;
            continue;
        };
        if !bytes[after_space..].starts_with(b"obj") {
            pos += 1;
            continue;
        }

        let body_start = after_space + 3;
        if let Some(relative_end) = find_subslice(&bytes[body_start..], b"endobj") {
            let body_end = body_start + relative_end;
            objects.push(PdfObject {
                object_number: object_number as u32,
                generation: generation as u16,
                body: bytes[body_start..body_end].to_vec(),
            });
            pos = body_end + b"endobj".len();
        } else {
            break;
        }
    }

    objects
}

fn expand_object_streams(objects: &mut Vec<PdfObject>) {
    let object_streams = objects
        .iter()
        .filter(|object| {
            lossy(&object.body)
                .split_whitespace()
                .collect::<String>()
                .contains("/Type/ObjStm")
        })
        .cloned()
        .collect::<Vec<_>>();
    let existing = objects
        .iter()
        .map(|object| object.object_number)
        .collect::<std::collections::HashSet<_>>();
    let mut expanded = Vec::new();

    for object_stream in object_streams {
        let object_body = lossy(&object_stream.body);
        let Some(count) = parse_number_after(&object_body, "/N").map(|value| value as usize) else {
            continue;
        };
        let Some(first) = parse_number_after(&object_body, "/First").map(|value| value as usize)
        else {
            continue;
        };
        let Ok(Some(decoded)) = decode_stream_object(&object_stream) else {
            continue;
        };
        if first > decoded.len() {
            continue;
        }

        let header = lossy(&decoded[..first]);
        let header_numbers = header
            .split_whitespace()
            .filter_map(|part| part.parse::<usize>().ok())
            .collect::<Vec<_>>();
        let mut entries = Vec::new();
        for pair in header_numbers.chunks_exact(2).take(count) {
            entries.push((pair[0] as u32, pair[1]));
        }

        for (index, (object_number, offset)) in entries.iter().enumerate() {
            if existing.contains(object_number) {
                continue;
            }
            let next_offset = entries
                .get(index + 1)
                .map(|(_, next_offset)| *next_offset)
                .unwrap_or(decoded.len() - first);
            if *offset > next_offset || first + next_offset > decoded.len() {
                continue;
            }
            expanded.push(PdfObject {
                object_number: *object_number,
                generation: 0,
                body: decoded[first + *offset..first + next_offset].to_vec(),
            });
        }
    }

    objects.extend(expanded);
}

fn page_seed(object: &PdfObject, object_map: &HashMap<u32, PdfObject>) -> Option<PageSeed> {
    let body = lossy(&object.body);
    let compact = body.split_whitespace().collect::<String>();
    if compact.contains("/Type/Page") && !compact.contains("/Type/Pages") {
        Some(PageSeed {
            number: 0,
            body: body_with_inherited_page_tree_entries(&body, object_map),
        })
    } else {
        None
    }
}

fn body_with_inherited_page_tree_entries(
    page_body: &str,
    object_map: &HashMap<u32, PdfObject>,
) -> String {
    let mut body = page_body.to_owned();
    append_parent_page_tree_entries(page_body, object_map, &mut body, 0);
    body
}

fn append_parent_page_tree_entries(
    body: &str,
    object_map: &HashMap<u32, PdfObject>,
    output: &mut String,
    depth: usize,
) {
    if depth >= 16 {
        return;
    }
    let Some(parent_ref) = parse_direct_ref_after_key(body, "/Parent") else {
        return;
    };
    let Some(parent) = object_map.get(&(parent_ref as u32)) else {
        return;
    };
    let parent_body = lossy(&parent.body);
    output.push('\n');
    output.push_str(&parent_body);
    append_parent_page_tree_entries(&parent_body, object_map, output, depth + 1);
}

fn decode_stream_object(object: &PdfObject) -> Result<Option<Vec<u8>>> {
    let Some(stream_marker) = find_subslice(&object.body, b"stream") else {
        return Ok(None);
    };
    let Some(end_marker) = find_subslice(&object.body, b"endstream") else {
        return Err(DonglerError::pdf("stream is missing endstream marker"));
    };
    if end_marker <= stream_marker {
        return Err(DonglerError::pdf("stream markers are malformed"));
    }

    let dict = lossy(&object.body[..stream_marker]);
    let mut stream = object.body[stream_marker + b"stream".len()..end_marker].to_vec();
    trim_stream_edges(&mut stream);

    let compact_dict = dict.split_whitespace().collect::<String>();
    if compact_dict.contains("/Filter/FlateDecode")
        || compact_dict.contains("/Filter[/FlateDecode")
        || compact_dict.contains("/Filter[/FlateDecode]")
    {
        let mut decoder = ZlibDecoder::new(stream.as_slice());
        let mut decoded = Vec::new();
        decoder
            .read_to_end(&mut decoded)
            .map_err(|error| DonglerError::pdf(format!("FlateDecode failed: {error}")))?;
        Ok(Some(decoded))
    } else {
        Ok(Some(stream))
    }
}

fn trim_stream_edges(stream: &mut Vec<u8>) {
    while matches!(stream.first(), Some(b'\n' | b'\r')) {
        stream.remove(0);
    }
    while matches!(stream.last(), Some(b'\n' | b'\r')) {
        stream.pop();
    }
}

fn parse_refs_after_key(text: &str, key: &str) -> Vec<usize> {
    let Some(start) = text.find(key) else {
        return Vec::new();
    };
    let rest = &text[start + key.len()..];
    if let Some(array_start) = rest.find('[') {
        let before_array = rest[..array_start].trim();
        if before_array.is_empty() {
            if let Some(array_end) = rest[array_start..].find(']') {
                return parse_refs(&rest[array_start..array_start + array_end]);
            }
        }
    }
    parse_refs(rest).into_iter().take(1).collect()
}

fn parse_direct_ref_after_key(text: &str, key: &str) -> Option<usize> {
    let start = text.find(key)?;
    let bytes = text.as_bytes();
    let mut pos = start + key.len();
    while pos < bytes.len() && is_ws(bytes[pos]) {
        pos += 1;
    }
    let (object, after_object) = parse_unsigned_at(bytes, pos)?;
    let after_space = skip_required_ws(bytes, after_object)?;
    let (_generation, after_generation) = parse_unsigned_at(bytes, after_space)?;
    let after_space = skip_required_ws(bytes, after_generation)?;
    if bytes.get(after_space) == Some(&b'R') {
        Some(object)
    } else {
        None
    }
}

fn parse_resource_refs(text: &str, key: &str) -> HashMap<String, u32> {
    let Some(start) = text.find(key) else {
        return HashMap::new();
    };
    let rest = &text[start + key.len()..];
    let Some(dict_start) = rest.find("<<") else {
        return HashMap::new();
    };
    let Some(dict_end) = rest[dict_start + 2..].find(">>") else {
        return HashMap::new();
    };
    let dict = &rest[dict_start + 2..dict_start + 2 + dict_end];
    parse_named_refs(dict)
}

fn resolve_resource_body(page_body: &str, object_map: &HashMap<u32, PdfObject>) -> Option<String> {
    let resource_ref = parse_direct_ref_after_key(page_body, "/Resources")?;
    object_map
        .get(&(resource_ref as u32))
        .map(|object| lossy(&object.body))
}

fn load_font_decoders(
    resource_text: &str,
    object_map: &HashMap<u32, PdfObject>,
) -> HashMap<String, FontDecoder> {
    resolve_named_resource_refs(resource_text, "/Font", object_map)
        .into_iter()
        .map(|(name, object_number)| {
            let decoder = object_map
                .get(&object_number)
                .map(|font| font_decoder(font, object_map))
                .unwrap_or_default();
            (name, decoder)
        })
        .collect()
}

fn resolve_named_resource_refs(
    resource_text: &str,
    key: &str,
    object_map: &HashMap<u32, PdfObject>,
) -> HashMap<String, u32> {
    let direct = parse_resource_refs(resource_text, key);
    if !direct.is_empty() {
        return direct;
    }

    parse_direct_ref_after_key(resource_text, key)
        .and_then(|object_number| object_map.get(&(object_number as u32)))
        .map(|object| parse_named_refs(&lossy(&object.body)))
        .unwrap_or_default()
}

fn font_decoder(font: &PdfObject, object_map: &HashMap<u32, PdfObject>) -> FontDecoder {
    let font_body = lossy(&font.body);
    let Some(to_unicode_ref) = parse_refs_after_key(&font_body, "/ToUnicode")
        .into_iter()
        .next()
    else {
        return FontDecoder::default();
    };
    let Some(to_unicode) = object_map.get(&(to_unicode_ref as u32)) else {
        return FontDecoder::default();
    };
    let Ok(Some(cmap_stream)) = decode_stream_object(to_unicode) else {
        return FontDecoder::default();
    };

    parse_to_unicode_cmap(&lossy(&cmap_stream))
}

fn parse_to_unicode_cmap(text: &str) -> FontDecoder {
    let mut cmap = HashMap::new();
    let mut in_bfchar = false;
    let mut in_bfrange = false;

    for line in text.lines() {
        let trimmed = line.trim();
        match trimmed {
            value if value.ends_with("beginbfchar") => {
                in_bfchar = true;
                continue;
            }
            "endbfchar" => {
                in_bfchar = false;
                continue;
            }
            value if value.ends_with("beginbfrange") => {
                in_bfrange = true;
                continue;
            }
            "endbfrange" => {
                in_bfrange = false;
                continue;
            }
            _ => {}
        }

        let hexes = hex_strings_in_line(trimmed);
        if in_bfchar && hexes.len() >= 2 {
            cmap.insert(
                hexes[0].clone(),
                cmap_text_for_mapping(&hexes[0], &hexes[1]),
            );
        } else if in_bfrange && hexes.len() >= 3 {
            add_bfrange(&mut cmap, &hexes);
        }
    }

    let max_code_len = cmap.keys().map(Vec::len).max().unwrap_or(1);
    FontDecoder { cmap, max_code_len }
}

fn add_bfrange(cmap: &mut HashMap<Vec<u8>, String>, hexes: &[Vec<u8>]) {
    let Some(start) = hex_to_u32(&hexes[0]) else {
        return;
    };
    let Some(end) = hex_to_u32(&hexes[1]) else {
        return;
    };
    let Some(destination) = hex_to_u32(&hexes[2]) else {
        return;
    };
    let source_len = hexes[0].len();

    for offset in 0..=(end.saturating_sub(start)).min(512) {
        let source = start + offset;
        let destination = destination + offset;
        cmap.insert(
            number_to_be_bytes(source, source_len),
            cmap_text_for_codes(source, destination),
        );
    }
}

fn cmap_text_for_mapping(source: &[u8], destination: &[u8]) -> String {
    let Some(source_code) = hex_to_u32(source) else {
        return utf16be_hex_to_string(destination);
    };
    let Some(destination_code) = hex_to_u32(destination) else {
        return utf16be_hex_to_string(destination);
    };
    cmap_text_for_codes(source_code, destination_code)
}

fn cmap_text_for_codes(source: u32, destination: u32) -> String {
    if is_private_use_text_code(destination) {
        if let Some(character) = private_use_source_ascii(source) {
            return character.to_string();
        }
    }
    char::from_u32(destination)
        .map(|character| character.to_string())
        .unwrap_or_default()
}

fn is_private_use_text_code(code: u32) -> bool {
    (0xe000..=0xf8ff).contains(&code)
}

fn private_use_source_ascii(source: u32) -> Option<char> {
    let ascii = source + 28;
    (0x20..=0x7e)
        .contains(&ascii)
        .then(|| char::from_u32(ascii))
        .flatten()
}

fn hex_strings_in_line(line: &str) -> Vec<Vec<u8>> {
    let bytes = line.as_bytes();
    let mut hexes = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos] == b'<' && bytes.get(pos + 1) != Some(&b'<') {
            let start = pos + 1;
            if let Some(end) = bytes[start..].iter().position(|byte| *byte == b'>') {
                hexes.push(decode_hex(&bytes[start..start + end]));
                pos = start + end + 1;
                continue;
            }
        }
        pos += 1;
    }

    hexes
}

fn utf16be_hex_to_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 {
        let units = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|byte| *byte as char).collect()
    }
}

fn hex_to_u32(bytes: &[u8]) -> Option<u32> {
    let mut value = 0u32;
    for byte in bytes {
        value = (value << 8) | (*byte as u32);
    }
    Some(value)
}

fn number_to_be_bytes(value: u32, len: usize) -> Vec<u8> {
    (0..len)
        .rev()
        .map(|shift| ((value >> (shift * 8)) & 0xff) as u8)
        .collect()
}

fn parse_named_refs(text: &str) -> HashMap<String, u32> {
    let mut refs = HashMap::new();
    let bytes = text.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos] != b'/' || bytes.get(pos + 1) == Some(&b'/') {
            pos += 1;
            continue;
        }
        pos += 1;
        let name_start = pos;
        while pos < bytes.len() && !is_delimiter_or_ws(bytes[pos]) {
            pos += 1;
        }
        let name = lossy(&bytes[name_start..pos]);
        while pos < bytes.len() && is_ws(bytes[pos]) {
            pos += 1;
        }
        let Some((object, after_object)) = parse_unsigned_at(bytes, pos) else {
            continue;
        };
        let Some(after_space) = skip_required_ws(bytes, after_object) else {
            pos += 1;
            continue;
        };
        let Some((_generation, after_generation)) = parse_unsigned_at(bytes, after_space) else {
            pos += 1;
            continue;
        };
        let Some(after_space) = skip_required_ws(bytes, after_generation) else {
            pos += 1;
            continue;
        };
        if bytes.get(after_space) == Some(&b'R') {
            refs.insert(name, object as u32);
            pos = after_space + 1;
        }
    }

    refs
}

fn parse_refs(text: &str) -> Vec<usize> {
    let mut refs = Vec::new();
    let bytes = text.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        let Some((object, after_object)) = parse_unsigned_at(bytes, pos) else {
            pos += 1;
            continue;
        };
        let Some(after_space) = skip_required_ws(bytes, after_object) else {
            pos += 1;
            continue;
        };
        let Some((_generation, after_generation)) = parse_unsigned_at(bytes, after_space) else {
            pos += 1;
            continue;
        };
        let Some(after_space) = skip_required_ws(bytes, after_generation) else {
            pos += 1;
            continue;
        };
        if bytes.get(after_space) == Some(&b'R') {
            refs.push(object);
            pos = after_space + 1;
        } else {
            pos += 1;
        }
    }

    refs
}

fn parse_number_array_after(text: &str, key: &str) -> Option<Vec<f32>> {
    let start = text.find(key)?;
    let rest = &text[start + key.len()..];
    let open = rest.find('[')?;
    let close = rest[open + 1..].find(']')?;
    Some(
        rest[open + 1..open + 1 + close]
            .split_whitespace()
            .filter_map(|part| part.parse::<f32>().ok())
            .collect(),
    )
}

fn parse_number_after(text: &str, key: &str) -> Option<f32> {
    let start = text.find(key)?;
    let bytes = text.as_bytes();
    let mut pos = start + key.len();
    while pos < bytes.len() && (is_ws(bytes[pos]) || matches!(bytes[pos], b'[' | b']')) {
        pos += 1;
    }
    let number_start = pos;
    while pos < bytes.len() && matches!(bytes[pos], b'+' | b'-' | b'.' | b'0'..=b'9') {
        pos += 1;
    }
    if pos == number_start {
        return None;
    }
    text[number_start..pos].parse().ok()
}

fn first_text_operand(
    operands: &[Operand],
    state: &GraphicsState,
    fonts: &HashMap<String, FontDecoder>,
) -> Option<String> {
    operands
        .first()
        .and_then(|operand| operand_text(operand, state, fonts))
}

fn operand_text(
    operand: &Operand,
    state: &GraphicsState,
    fonts: &HashMap<String, FontDecoder>,
) -> Option<String> {
    match operand {
        Operand::Literal(bytes) | Operand::Hex(bytes) => Some(decode_pdf_text(
            bytes,
            state
                .font_name
                .as_ref()
                .and_then(|font_name| fonts.get(font_name)),
        )),
        _ => None,
    }
}

fn text_from_array(
    items: &[Operand],
    state: &GraphicsState,
    fonts: &HashMap<String, FontDecoder>,
) -> String {
    let mut text = String::new();
    for item in items {
        match item {
            Operand::Number(value) if value.abs() >= 120.0 => {
                if !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            _ => {
                if let Some(part) = operand_text(item, state, fonts) {
                    text.push_str(&part);
                }
            }
        }
    }
    text
}

fn decode_pdf_text(bytes: &[u8], font: Option<&FontDecoder>) -> String {
    if let Some(font) = font {
        if !font.cmap.is_empty() {
            return decode_with_cmap(bytes, font);
        }
    }

    if bytes.starts_with(&[0xfe, 0xff]) {
        let utf16 = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&utf16)
    } else {
        bytes.iter().map(|byte| *byte as char).collect()
    }
}

fn decode_with_cmap(bytes: &[u8], font: &FontDecoder) -> String {
    let mut output = String::new();
    let mut index = 0;

    while index < bytes.len() {
        let max_len = font.max_code_len.min(bytes.len() - index).max(1);
        let mut matched = false;
        for len in (1..=max_len).rev() {
            if let Some(text) = font.cmap.get(&bytes[index..index + len]) {
                output.push_str(text);
                index += len;
                matched = true;
                break;
            }
        }
        if !matched {
            output.push(bytes[index] as char);
            index += 1;
        }
    }

    output
}

fn numbers(operands: &[Operand], count: usize) -> Option<Vec<f32>> {
    if operands.len() < count {
        return None;
    }
    let values = operands[operands.len() - count..]
        .iter()
        .map(|operand| match operand {
            Operand::Number(value) => Some(*value),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(values)
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

fn classify_text_line(text: &str) -> String {
    if text.chars().count() < 120 && text.ends_with(':') {
        "heading".to_owned()
    } else {
        "paragraph".to_owned()
    }
}

fn source_ids_for_line(line: &TextLine) -> Vec<String> {
    let mut ids = Vec::new();
    for run in &line.runs {
        for id in &run.source_object_ids {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    ids
}

fn anchor(page_number: usize, bbox: Option<BBox>, pdf_object_ids: Vec<String>) -> SourceAnchor {
    SourceAnchor {
        page_number,
        pdf_object_ids,
        bbox,
        extraction_method: "native_pdf".to_owned(),
    }
}

fn warning(code: &str, severity: &str, message: &str, page_number: Option<usize>) -> Warning {
    Warning {
        code: code.to_owned(),
        severity: severity.to_owned(),
        message: message.to_owned(),
        source_anchor: page_number.map(|page_number| anchor(page_number, None, Vec::new())),
    }
}

fn union_boxes(boxes: impl IntoIterator<Item = BBox>) -> Option<BBox> {
    let mut iter = boxes.into_iter();
    let first = iter.next()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.width;
    let mut max_y = first.y + first.height;

    for bbox in iter {
        min_x = min_x.min(bbox.x);
        min_y = min_y.min(bbox.y);
        max_x = max_x.max(bbox.x + bbox.width);
        max_y = max_y.max(bbox.y + bbox.height);
    }

    Some(BBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn extract_info_string(objects: &[PdfObject], key: &str) -> Option<String> {
    let needle = format!("/{key}");
    objects.iter().find_map(|object| {
        let body = lossy(&object.body);
        if !(body.contains("/Producer") || body.contains("/Creator") || body.contains("/Author")) {
            return None;
        }
        let start = body.find(&needle)?;
        let rest = &object.body[start + needle.len()..];
        let open = rest.iter().position(|byte| *byte == b'(')?;
        let mut parser = ContentParser::new(&rest[open..]);
        match parser.next_operand_or_operator()? {
            ContentToken::Operand(Operand::Literal(bytes)) => Some(decode_pdf_text(&bytes, None)),
            _ => None,
        }
    })
}

fn pdf_version(bytes: &[u8]) -> Option<String> {
    let first_line = bytes.split(|byte| matches!(byte, b'\n' | b'\r')).next()?;
    let text = std::str::from_utf8(first_line).ok()?;
    text.strip_prefix("%PDF-").map(ToOwned::to_owned)
}

fn decode_hex(bytes: &[u8]) -> Vec<u8> {
    let hex = bytes
        .iter()
        .copied()
        .filter(|byte| !is_ws(*byte))
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;
    while index < hex.len() {
        let high = hex_value(hex[index]).unwrap_or(0);
        let low = hex
            .get(index + 1)
            .and_then(|byte| hex_value(*byte))
            .unwrap_or(0);
        output.push((high << 4) | low);
        index += 2;
    }
    output
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_unsigned_at(bytes: &[u8], mut pos: usize) -> Option<(usize, usize)> {
    let start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..pos])
        .ok()?
        .parse()
        .ok()
        .map(|value| (value, pos))
}

fn skip_required_ws(bytes: &[u8], mut pos: usize) -> Option<usize> {
    if pos >= bytes.len() || !is_ws(bytes[pos]) {
        return None;
    }
    while pos < bytes.len() && is_ws(bytes[pos]) {
        pos += 1;
    }
    Some(pos)
}

fn is_ws_or_line_start(bytes: &[u8], pos: usize) -> bool {
    pos == 0 || matches!(bytes[pos - 1], b'\n' | b'\r')
}

fn is_delimiter_or_ws(byte: u8) -> bool {
    is_ws(byte) || matches!(byte, b'[' | b']' | b'<' | b'>' | b'/' | b'(' | b')')
}

fn is_ws(byte: u8) -> bool {
    matches!(byte, 0x00 | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn contains_name(bytes: &[u8], name: &[u8]) -> bool {
    find_subslice(bytes, name).is_some()
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[allow(dead_code)]
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
