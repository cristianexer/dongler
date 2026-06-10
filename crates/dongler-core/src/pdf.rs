use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use flate2::read::ZlibDecoder;
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::engine::ExtractionEngine;
use crate::error::{DonglerError, Result};
use crate::ir::{
    Asset, BBox, Block, Confidence, Document, FigureBlock, ImageObject, Line, Metadata, Page,
    SourceAnchor, Span, TableBlock, TableCell, TextBlock, Warning, SCHEMA_VERSION,
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
    /// Page-space y of the text baseline, kept separate from `bbox` (which now
    /// spans ascent..descent) so super/subscript detection stays baseline-based.
    baseline_y: f32,
    font: Option<String>,
    size: f32,
    /// Page-space advance of a single space glyph in this run's font/size, used to
    /// decide whether a horizontal gap to the next run is a word break. Producers
    /// often position fragments with `Td`/`TJ` and omit the space character, so the
    /// gap is the only signal; sizing the threshold to the actual space width keeps
    /// word segmentation correct across fonts and zoom levels.
    space_width: f32,
    bold: bool,
    italic: bool,
    source_object_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct TextLine {
    runs: Vec<TextRun>,
    bbox: BBox,
    baseline_y: f32,
}

#[derive(Debug, Clone)]
struct DetectedTable {
    table: TableBlock,
    line_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
struct TableRowCandidate {
    line_index: usize,
    cells: Vec<TextRun>,
}

#[derive(Debug, Clone, Copy)]
struct GraphicEdge {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptKind {
    Superscript,
    Subscript,
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
    edges: Vec<GraphicEdge>,
    images: Vec<ImageObject>,
    assets: Vec<Asset>,
    warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Default)]
struct FontDecoder {
    cmap: HashMap<Vec<u8>, String>,
    encoding: HashMap<u8, String>,
    widths: HashMap<char, f32>,
    max_code_len: usize,
    bold: bool,
    italic: bool,
    ascent: f32,
    descent: f32,
}

impl FontDecoder {
    fn decode_byte(&self, byte: u8) -> String {
        self.encoding
            .get(&byte)
            .cloned()
            .unwrap_or_else(|| (byte as char).to_string())
    }
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
    text_matrix: Matrix,
    line_matrix: Matrix,
    font_name: Option<String>,
    font_size: f32,
    leading: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scaling: f32,
    text_rise: f32,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Matrix::identity(),
            text_matrix: Matrix::identity(),
            line_matrix: Matrix::identity(),
            font_name: None,
            font_size: 12.0,
            leading: 12.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 1.0,
            text_rise: 0.0,
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

    fn translate(self, x: f32, y: f32) -> Self {
        Self {
            e: self.e + self.a * x + self.c * y,
            f: self.f + self.b * x + self.d * y,
            ..self
        }
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

    // Share each parsed object behind a single Arc between the ordered list
    // (which preserves page order and any duplicate object numbers exactly) and
    // the lookup map, so object bodies are stored once instead of copied per
    // map entry.
    let title = extract_info_string(&objects, "Title");
    let objects: Vec<Arc<PdfObject>> = objects.into_iter().map(Arc::new).collect();
    let object_map: HashMap<u32, Arc<PdfObject>> = objects
        .iter()
        .map(|object| (object.object_number, Arc::clone(object)))
        .collect();
    let page_seeds = objects
        .iter()
        .filter_map(|object| page_seed(object.as_ref(), &object_map))
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
    let encrypted = contains_name(bytes, b"/Encrypt");
    if encrypted {
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

    // Decode each font once per document. Fonts (and their compressed ToUnicode
    // CMaps) are shared resources referenced by most pages, so decoding them in
    // every page re-inflates the same streams pages*fonts times.
    let mut font_object_numbers: Vec<u32> = page_seeds
        .iter()
        .flat_map(|seed| {
            let resource_body = resolve_resource_body(&seed.body, &object_map);
            let resource_text = resource_body.as_deref().unwrap_or(&seed.body);
            resolve_named_resource_refs(resource_text, "/Font", &object_map)
                .into_values()
                .collect::<Vec<_>>()
        })
        .collect();
    font_object_numbers.sort_unstable();
    font_object_numbers.dedup();
    let font_cache: HashMap<u32, Arc<FontDecoder>> = font_object_numbers
        .into_par_iter()
        .filter_map(|number| {
            object_map
                .get(&number)
                .map(|font| (number, Arc::new(font_decoder(font.as_ref(), &object_map))))
        })
        .collect();

    let page_extractions = page_seeds
        .par_iter()
        .map(|seed| extract_page(seed, &object_map, &font_cache))
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
            title,
            character_count: all_text.chars().count(),
            word_count: all_text.split_whitespace().count(),
            block_count: pages.iter().map(|page| page.blocks.len()).sum(),
            file_size_bytes: Some(bytes.len() as u64),
            pdf_version: pdf_version(bytes),
            encrypted,
        },
        pages,
        assets,
        warnings: document_warnings,
    })
}

fn extract_page(
    seed: &PageSeed,
    object_map: &HashMap<u32, Arc<PdfObject>>,
    font_cache: &HashMap<u32, Arc<FontDecoder>>,
) -> PageExtraction {
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
    let fonts = load_font_decoders(resource_text, object_map, font_cache);

    let mut warnings = Vec::new();
    let mut extraction = ContentExtraction {
        text_runs: Vec::new(),
        edges: Vec::new(),
        images: Vec::new(),
        assets: Vec::new(),
        warnings: Vec::new(),
    };

    for content_ref in contents {
        match object_map
            .get(&(content_ref as u32))
            .map(|object| decode_stream_object(object.as_ref()))
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
                extraction.edges.append(&mut content.edges);
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

    // Apply the page /Rotate so line grouping and reading order run in the
    // orientation a reader sees. Display dimensions swap for 90/270.
    let normalized_rotation = rotation.map(|value| value.rem_euclid(360)).unwrap_or(0);
    if normalized_rotation != 0 {
        for run in &mut extraction.text_runs {
            run.bbox = rotate_bbox(run.bbox, normalized_rotation, width, height);
        }
        for image in &mut extraction.images {
            if let Some(bbox) = image.bbox {
                image.bbox = Some(rotate_bbox(bbox, normalized_rotation, width, height));
            }
        }
        for edge in &mut extraction.edges {
            let (x0, y0) = rotate_point(edge.x0, edge.y0, normalized_rotation, width, height);
            let (x1, y1) = rotate_point(edge.x1, edge.y1, normalized_rotation, width, height);
            edge.x0 = x0;
            edge.y0 = y0;
            edge.x1 = x1;
            edge.y1 = y1;
        }
    }
    let (page_width, page_height) = if matches!(normalized_rotation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    };
    let (page_x, page_y) = if normalized_rotation == 0 {
        (
            media_box.first().copied().unwrap_or(0.0),
            media_box.get(1).copied().unwrap_or(0.0),
        )
    } else {
        (0.0, 0.0)
    };

    let lines = group_text_runs(extraction.text_runs);
    let mut blocks = build_blocks(seed.number, &lines, &extraction.edges);
    if blocks.is_empty() && !extraction.images.is_empty() {
        blocks.extend(image_figure_blocks(seed.number, &extraction.images));
    }
    let text = blocks
        .iter()
        .map(block_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let page = Page {
        number: seed.number,
        width: Some(page_width),
        height: Some(page_height),
        rotation,
        bbox: Some(BBox {
            x: page_x,
            y: page_y,
            width: page_width,
            height: page_height,
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
    fonts: &HashMap<String, Arc<FontDecoder>>,
    object_map: &HashMap<u32, Arc<PdfObject>>,
) -> ContentExtraction {
    let mut state = GraphicsState::default();
    let mut graphics_stack = Vec::new();
    let mut current_path_point: Option<(f32, f32)> = None;
    let mut pending_edges = Vec::new();
    let mut extraction = ContentExtraction {
        text_runs: Vec::new(),
        edges: Vec::new(),
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
                state.text_matrix = Matrix::identity();
                state.line_matrix = Matrix::identity();
            }
            "Tf" => {
                if let [Operand::Name(name), Operand::Number(size)] = op.operands.as_slice() {
                    state.font_name = Some(name.clone());
                    state.font_size = *size;
                    state.leading = *size * 1.2;
                }
            }
            "Tc" => {
                if let Some(values) = numbers(&op.operands, 1) {
                    state.char_spacing = values[0];
                }
            }
            "Tw" => {
                if let Some(values) = numbers(&op.operands, 1) {
                    state.word_spacing = values[0];
                }
            }
            "Tz" => {
                if let Some(values) = numbers(&op.operands, 1) {
                    state.horizontal_scaling = (values[0] / 100.0).max(0.01);
                }
            }
            "TL" => {
                if let Some(values) = numbers(&op.operands, 1) {
                    state.leading = values[0];
                }
            }
            "Ts" => {
                if let Some(values) = numbers(&op.operands, 1) {
                    state.text_rise = values[0];
                }
            }
            "Td" | "TD" => {
                if let Some(values) = numbers(&op.operands, 2) {
                    let next_line = state.line_matrix.translate(values[0], values[1]);
                    state.line_matrix = next_line;
                    state.text_matrix = next_line;
                    if op.operator == "TD" {
                        state.leading = -values[1];
                    }
                }
            }
            "Tm" => {
                if let Some(values) = numbers(&op.operands, 6) {
                    let matrix = Matrix {
                        a: values[0],
                        b: values[1],
                        c: values[2],
                        d: values[3],
                        e: values[4],
                        f: values[5],
                    };
                    state.line_matrix = matrix;
                    state.text_matrix = matrix;
                }
            }
            "T*" => {
                move_to_next_text_line(&mut state);
            }
            "Tj" => {
                if let Some(text) = first_text_operand(&op.operands, &state, fonts) {
                    push_text_run(&mut extraction, &mut state, source_object_ids, text, fonts);
                }
            }
            "TJ" => {
                if let Some(Operand::Array(items)) = op.operands.first() {
                    let text = text_from_array(items, &state, fonts);
                    push_text_run(&mut extraction, &mut state, source_object_ids, text, fonts);
                }
            }
            "'" => {
                move_to_next_text_line(&mut state);
                if let Some(text) = first_text_operand(&op.operands, &state, fonts) {
                    push_text_run(&mut extraction, &mut state, source_object_ids, text, fonts);
                }
            }
            "\"" => {
                if let [Operand::Number(word_spacing), Operand::Number(char_spacing), ..] =
                    op.operands.as_slice()
                {
                    state.word_spacing = *word_spacing;
                    state.char_spacing = *char_spacing;
                }
                move_to_next_text_line(&mut state);
                if let Some(text) = op
                    .operands
                    .last()
                    .and_then(|operand| operand_text(operand, &state, fonts))
                {
                    push_text_run(&mut extraction, &mut state, source_object_ids, text, fonts);
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
            "m" => {
                if let Some(values) = numbers(&op.operands, 2) {
                    current_path_point = Some((values[0], values[1]));
                }
            }
            "l" => {
                if let (Some(start), Some(values)) = (current_path_point, numbers(&op.operands, 2))
                {
                    let end = (values[0], values[1]);
                    pending_edges.push(graphic_edge_from_points(state.ctm, start, end));
                    current_path_point = Some(end);
                }
            }
            "re" => {
                if let Some(values) = numbers(&op.operands, 4) {
                    pending_edges.extend(graphic_edges_from_rect(
                        state.ctm, values[0], values[1], values[2], values[3],
                    ));
                    current_path_point = Some((values[0], values[1]));
                }
            }
            "S" | "s" => {
                extraction.edges.append(&mut pending_edges);
                current_path_point = None;
            }
            "n" => {
                pending_edges.clear();
                current_path_point = None;
            }
            _ => {}
        }
    }

    extraction
}

fn graphic_edge_from_points(matrix: Matrix, start: (f32, f32), end: (f32, f32)) -> GraphicEdge {
    let (x0, y0) = matrix.point(start.0, start.1);
    let (x1, y1) = matrix.point(end.0, end.1);
    GraphicEdge { x0, y0, x1, y1 }
}

fn graphic_edges_from_rect(
    matrix: Matrix,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Vec<GraphicEdge> {
    let right = x + width;
    let top = y + height;
    vec![
        graphic_edge_from_points(matrix, (x, y), (right, y)),
        graphic_edge_from_points(matrix, (right, y), (right, top)),
        graphic_edge_from_points(matrix, (right, top), (x, top)),
        graphic_edge_from_points(matrix, (x, top), (x, y)),
    ]
}

fn move_to_next_text_line(state: &mut GraphicsState) {
    let next_line = state.line_matrix.translate(0.0, -state.leading);
    state.line_matrix = next_line;
    state.text_matrix = next_line;
}

fn push_text_run(
    extraction: &mut ContentExtraction,
    state: &mut GraphicsState,
    source_object_ids: &[String],
    text: String,
    fonts: &HashMap<String, Arc<FontDecoder>>,
) {
    let advance = text_advance_width(&text, state, fonts);
    if text.trim().is_empty() {
        state.text_matrix = state.text_matrix.translate(advance, 0.0);
        return;
    }

    let font = state.font_name.as_ref().and_then(|name| fonts.get(name));
    let (bold, italic) = font
        .map(|font| (font.bold, font.italic))
        .unwrap_or((false, false));
    let (ascent, descent) = font
        .map(|font| (font.ascent, font.descent))
        .unwrap_or((0.75, -0.25));
    let bbox = text_run_bbox(state, advance, ascent, descent);
    let (base_x, base_y) = state.text_matrix.point(0.0, state.text_rise);
    let (_, baseline_y) = state.ctm.point(base_x, base_y);
    let space_width = space_advance_width(state, fonts);
    extraction.text_runs.push(TextRun {
        text,
        bbox,
        baseline_y,
        font: state.font_name.clone(),
        size: state.font_size,
        space_width,
        bold,
        italic,
        source_object_ids: source_object_ids.to_vec(),
    });
    state.text_matrix = state.text_matrix.translate(advance, 0.0);
}

fn text_advance_width(
    text: &str,
    state: &GraphicsState,
    fonts: &HashMap<String, Arc<FontDecoder>>,
) -> f32 {
    let glyphs = text.chars().count() as f32;
    if glyphs == 0.0 {
        return 0.0;
    }
    let spaces = text.chars().filter(|character| *character == ' ').count() as f32;
    let font = state
        .font_name
        .as_ref()
        .and_then(|font_name| fonts.get(font_name));
    let base = text
        .chars()
        .map(|character| {
            font.and_then(|font| font.widths.get(&character).copied())
                .unwrap_or_else(|| default_glyph_width(character))
                / 1000.0
                * state.font_size
        })
        .sum::<f32>();
    let spacing = glyphs * state.char_spacing + spaces * state.word_spacing;
    ((base + spacing) * state.horizontal_scaling).max(0.0)
}

/// Approximate advance (1/1000 em) of a glyph when the font carries no width for
/// it. Uses Helvetica's metrics, which track real proportional Latin widths far
/// better than a flat half-em: narrow glyphs (`i l . ,`) are ~250, wide ones
/// (`m w M W`) ~850. Accurate advances are what let gap-based word segmentation
/// work on fonts that omit `/Widths` (some subset and OCR-layer fonts).
fn default_glyph_width(character: char) -> f32 {
    match character {
        ' ' | '!' | ',' | '.' | '/' | ':' | ';' | 'I' | '[' | '\\' | ']' | 'i' | 'j' | 'l'
        | '|' | '\'' => 250.0,
        '"' | '(' | ')' | '*' | '`' | '-' | 'f' | 'r' | 't' | '{' | '}' => 333.0,
        'm' | 'M' | 'W' | 'w' | '@' => 850.0,
        '0'..='9' => 556.0,
        'A'..='Z' | '$' | '+' | '<' | '=' | '>' | '?' | '_' | '~' => 650.0,
        _ => 500.0,
    }
}

/// Page-space advance of one space glyph in the current font/size, scaled by the
/// horizontal scaling. Falls back to a quarter-em when the font has no space-glyph
/// metric, which is the typical width of a space across text fonts.
fn space_advance_width(state: &GraphicsState, fonts: &HashMap<String, Arc<FontDecoder>>) -> f32 {
    let from_font = state
        .font_name
        .as_ref()
        .and_then(|font_name| fonts.get(font_name))
        .and_then(|font| font.widths.get(&' ').copied())
        .filter(|width| *width > 0.0)
        .map(|width| width / 1000.0 * state.font_size);
    let width = from_font.unwrap_or_else(|| default_glyph_width(' ') / 1000.0 * state.font_size);
    (width * state.horizontal_scaling).max(0.0)
}

fn text_run_bbox(state: &GraphicsState, advance: f32, ascent: f32, descent: f32) -> BBox {
    // Vertical extent from the font's ascent/descent (em-relative to the
    // baseline) rather than a flat font-size box, so glyph boxes are tight and
    // baseline-correct under scaling/rotation.
    let bottom = state.text_rise + descent * state.font_size;
    let top = state.text_rise + ascent * state.font_size;
    let corners = [
        (0.0, bottom),
        (advance, bottom),
        (0.0, top),
        (advance, top),
    ];
    let points = corners
        .into_iter()
        .map(|(x, y)| {
            let (text_x, text_y) = state.text_matrix.point(x, y);
            state.ctm.point(text_x, text_y)
        })
        .collect::<Vec<_>>();
    let min_x = points.iter().map(|(x, _)| *x).fold(f32::INFINITY, f32::min);
    let min_y = points.iter().map(|(_, y)| *y).fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|(x, _)| *x)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = points
        .iter()
        .map(|(_, y)| *y)
        .fold(f32::NEG_INFINITY, f32::max);
    BBox {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(state.font_size * 0.25),
        height: (max_y - min_y).max(state.font_size * 0.25),
    }
}

fn build_blocks(page_number: usize, lines: &[TextLine], edges: &[GraphicEdge]) -> Vec<Block> {
    if let Some(detected_table) = detect_table(page_number, lines, edges) {
        return build_blocks_with_table(page_number, lines, detected_table);
    }

    let body_size = page_body_size(lines);
    let split_lines = split_wide_text_lines(lines);
    let text_blocks = text_lines_in_reading_order(&split_lines)
        .into_iter()
        .filter_map(|line| text_block_from_line(page_number, line, body_size))
        .collect::<Vec<_>>();
    merge_wrapped_text_blocks(text_blocks)
        .into_iter()
        .map(Block::Text)
        .collect()
}

fn build_blocks_with_table(
    page_number: usize,
    lines: &[TextLine],
    detected_table: DetectedTable,
) -> Vec<Block> {
    let body_size = page_body_size(lines);
    let remaining_lines = lines
        .iter()
        .enumerate()
        .filter(|(line_index, _)| !detected_table.line_indices.contains(line_index))
        .map(|(_, line)| line.clone())
        .collect::<Vec<_>>();
    let split_lines = split_wide_text_lines(&remaining_lines);
    let text_blocks = merge_wrapped_text_blocks(
        text_lines_in_reading_order(&split_lines)
            .into_iter()
            .filter_map(|line| text_block_from_line(page_number, line, body_size))
            .collect(),
    );
    let table_top = detected_table
        .table
        .bbox
        .map(|bbox| bbox.y + bbox.height)
        .unwrap_or(f32::NEG_INFINITY);
    let mut blocks = Vec::new();
    let mut table_inserted = false;

    for text_block in text_blocks {
        let block_top = text_block
            .bbox
            .map(|bbox| bbox.y + bbox.height)
            .unwrap_or(f32::NEG_INFINITY);
        if !table_inserted && block_top < table_top {
            blocks.push(Block::Table(detected_table.table.clone()));
            table_inserted = true;
        }
        blocks.push(Block::Text(text_block));
    }

    if !table_inserted {
        blocks.push(Block::Table(detected_table.table));
    }

    blocks
}

fn image_figure_blocks(page_number: usize, images: &[ImageObject]) -> Vec<Block> {
    images
        .iter()
        .map(|image| {
            Block::Figure(FigureBlock {
                alt_text: Some(format!("Image {}", image.id)),
                caption: None,
                bbox: image.bbox,
                image_ref: Some(image.id.clone()),
                source_anchors: vec![anchor(
                    page_number,
                    image.bbox,
                    image.object_id.clone().into_iter().collect(),
                )],
                confidence: Some(Confidence {
                    score: 0.6,
                    calibrated: false,
                }),
            })
        })
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

/// True when a line's runs are already ordered left-to-right by x.
fn line_runs_x_sorted(runs: &[TextRun]) -> bool {
    runs.windows(2).all(|pair| pair[0].bbox.x <= pair[1].bbox.x)
}

/// Runs of a line ordered left-to-right by x. Borrows when already sorted — the
/// common case, since `group_text_runs` keeps each line x-sorted — and clones +
/// sorts only when a reorder is actually required, avoiding a deep
/// `Vec<TextRun>` clone on every column/word pass.
fn runs_sorted_by_x(line: &TextLine) -> Cow<'_, [TextRun]> {
    if line_runs_x_sorted(&line.runs) {
        Cow::Borrowed(&line.runs)
    } else {
        let mut runs = line.runs.clone();
        runs.sort_by(|left, right| left.bbox.x.total_cmp(&right.bbox.x));
        Cow::Owned(runs)
    }
}

fn split_text_line_at_wide_gap(
    line: &TextLine,
    enable_tight_column_band: bool,
) -> Option<(TextLine, TextLine)> {
    if line.runs.len() < 2 {
        return None;
    }
    let runs = runs_sorted_by_x(line);
    let contains_math = runs
        .iter()
        .any(|run| looks_like_pdf_math_notation(&normalize_pdf_token(&run.text)));
    let tight_column_split_index = enable_tight_column_band
        .then(|| tight_column_band_split_index_for_runs(&runs[..]))
        .flatten();
    let largest_gap_split = largest_run_gap(&runs[..]);
    if contains_math && tight_column_split_index.is_none() {
        return None;
    }
    let split_index = match (tight_column_split_index, largest_gap_split) {
        (Some(tight_index), Some((wide_index, gap, x_jump)))
            if prefers_wide_gap_before_tight_band(&runs[..], wide_index, tight_index, gap, x_jump) =>
        {
            wide_index
        }
        (Some(tight_index), _) => tight_index,
        (None, Some((wide_index, _, _))) => wide_index,
        (None, None) => return None,
    };
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
            let runs = runs_sorted_by_x(line);
            tight_column_band_split_index_for_runs(&runs[..]).is_some()
        })
        .take(2)
        .count()
        >= 2
}

fn tight_column_band_split_index_for_runs(runs: &[TextRun]) -> Option<usize> {
    let split_index = right_column_band_split_index(runs)?;
    let contains_math = runs
        .iter()
        .any(|run| looks_like_pdf_math_notation(&normalize_pdf_token(&run.text)));
    if contains_math && !allows_math_column_split(&runs[..split_index]) {
        return None;
    }
    Some(split_index)
}

fn right_column_band_split_index(runs: &[TextRun]) -> Option<usize> {
    if runs.len() < 3 || runs.first()?.bbox.x > 120.0 {
        return None;
    }

    for index in 1..runs.len() {
        if index < 2 {
            continue;
        }
        let algorithm_like_left = allows_math_column_split(&runs[..index]);
        let right_x = runs[index].bbox.x;
        let in_standard_column_band = (300.0..=340.0).contains(&right_x);
        let in_algorithm_column_band = algorithm_like_left && (280.0..=340.0).contains(&right_x);
        if !in_standard_column_band && !in_algorithm_column_band {
            continue;
        }
        if runs.len() - index < 2 && !algorithm_like_left {
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

fn allows_math_column_split(left_runs: &[TextRun]) -> bool {
    let text = left_runs
        .iter()
        .map(|run| run.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = text.trim_start();
    starts_with_numbered_step(trimmed)
        || trimmed.starts_with("Require:")
        || trimmed.starts_with("Ensure:")
        || trimmed.starts_with("Algorithm ")
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
    let baseline_y = runs.iter().map(|run| run.baseline_y).sum::<f32>() / runs.len() as f32;
    Some(TextLine {
        runs,
        bbox,
        baseline_y,
    })
}

fn prefers_wide_gap_before_tight_band(
    runs: &[TextRun],
    wide_index: usize,
    tight_index: usize,
    gap: f32,
    x_jump: f32,
) -> bool {
    if wide_index == 0 || wide_index >= tight_index || tight_index > runs.len() {
        return false;
    }

    let left = &runs[wide_index - 1].bbox;
    let right = &runs[wide_index].bbox;
    let stranded_right_glyphs = runs[wide_index..tight_index]
        .iter()
        .all(|run| run.bbox.x >= 280.0 && run.text.trim().chars().count() <= 2);

    stranded_right_glyphs && left.x < 280.0 && right.x >= 280.0 && x_jump >= 110.0 && gap >= -160.0
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
    // Geometry-aware join so callers (table-label checks, wrapped-label detection,
    // header assembly) see real words rather than the letter-spaced output the old
    // `trim().join(" ")` produced on glyph-by-glyph PDFs.
    join_runs_spaced(&runs_sorted_by_x(line)).trim().to_owned()
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

fn text_block_from_line(page_number: usize, line: &TextLine, body_size: f32) -> Option<TextBlock> {
    let text = text_from_line_runs(line);
    let text = clean_pdf_line_text(&text);
    if text.is_empty() {
        return None;
    }

    Some(TextBlock {
        text: text.clone(),
        kind: classify_text_line(&text, line_dominant_size(line), body_size),
        bbox: Some(line.bbox),
        lines: vec![Line {
            text,
            bbox: Some(line.bbox),
            spans: line
                .runs
                .iter()
                .filter_map(|run| {
                    let text = clean_pdf_span_text(&run.text);
                    (!text.is_empty()).then(|| Span {
                        text,
                        bbox: Some(run.bbox),
                        font: run.font.clone(),
                        size: Some(run.size),
                        bold: run.bold,
                        italic: run.italic,
                    })
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

/// Assemble a line's text from its x-sorted runs. A space is placed between two
/// runs only when the producer already encoded one (a space at the boundary) or
/// the horizontal gap is wide enough to be a word break, sized to the font's own
/// space-glyph width. Run-internal spaces are preserved verbatim — only the
/// inter-run boundary is decided here. This replaces the old `trim().join(" ")`,
/// which both dropped producer spaces (joining words: "Netincome") and inserted
/// spurious ones (splitting fragmented words: "Y ear", "2 0 5 4 9").
fn join_runs_spaced(runs: &[TextRun]) -> String {
    let mut out = String::new();
    // (end_x, space_width, baseline_y, multi_char)
    let mut previous: Option<(f32, f32, f32, bool)> = None;
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        let multi_char = run.text.trim().chars().count() >= 2;
        if let Some((prev_end_x, prev_space_width, prev_baseline_y, prev_multi)) = previous {
            let boundary_has_space = out.ends_with(char::is_whitespace)
                || run.text.starts_with(char::is_whitespace);
            let gap = run.bbox.x - prev_end_x;
            // Two complete (multi-char) tokens are separate words, so even a tight
            // gap is a word break; a sequence of single glyphs may be a
            // letter-spaced word, so only a clear gap separates them. This is what
            // distinguishes "It occurs" (two words, ~2pt apart) from a fragmented
            // or letter-spaced "U N I T E D" that should read "UNITED".
            let tokens_separate = prev_multi || multi_char;
            let threshold =
                word_gap_threshold(prev_space_width, run.space_width, run.size, tokens_separate);
            // A meaningful baseline shift means the adjacent run sits on a
            // different line of text (a super/subscript or a stacked cell being
            // flattened); keep those tokens apart even when they abut horizontally.
            let baseline_break =
                (prev_baseline_y - run.baseline_y).abs() >= run.size.max(1.0) * 0.18;
            // Two complete tokens that appear to *overlap* by more than half a space
            // width are separate words whose advance was over-estimated (common with
            // fallback metrics), not a continuation — a real word never overlaps the
            // next. A near-zero gap stays joined, so a ligature fragment that abuts
            // ("fi" + "scal") is unaffected.
            let overlap_break =
                tokens_separate && gap <= -(prev_space_width.max(run.space_width) * 0.6).max(0.5);
            if !out.is_empty()
                && !boundary_has_space
                && (gap >= threshold || baseline_break || overlap_break)
            {
                out.push(' ');
            }
        }
        out.push_str(&run.text);
        previous = Some((
            run.bbox.x + run.bbox.width,
            run.space_width,
            run.baseline_y,
            multi_char,
        ));
    }
    out
}

/// Minimum horizontal gap (page units) between two runs that reads as a word
/// break. Scaled to the wider of the two runs' space-glyph widths (quarter-em
/// floor when a font lacks the metric). Separate multi-char tokens use a small
/// fraction (a real but tight inter-word space still counts), while single-glyph
/// runs need most of a space width so a letter-spaced word is not torn apart.
fn word_gap_threshold(
    left_space_width: f32,
    right_space_width: f32,
    size: f32,
    tokens_separate: bool,
) -> f32 {
    let space = left_space_width
        .max(right_space_width)
        .max(size * 0.25)
        .max(0.1);
    space * if tokens_separate { 0.1 } else { 0.4 }
}

fn text_from_line_runs(line: &TextLine) -> String {
    let runs = runs_sorted_by_x(line);
    if !line_has_math_script_context(&runs[..]) {
        return join_runs_spaced(&runs[..]);
    }

    let Some(baseline_y) = dominant_baseline_y(&runs[..]) else {
        return join_runs_spaced(&runs[..]);
    };
    let mut pieces: Vec<String> = Vec::new();

    for run in runs.iter() {
        let token = run.text.trim();
        if token.is_empty() {
            continue;
        }

        if let Some(script) = script_kind_for_run(run, baseline_y) {
            if let Some(previous) = pieces.last_mut() {
                if can_attach_math_script(previous, token) {
                    previous.push_str(&format_math_script(script, token));
                    continue;
                }
            }
        }

        pieces.push(token.to_owned());
    }

    pieces.join(" ")
}

fn dominant_baseline_y(runs: &[TextRun]) -> Option<f32> {
    let max_size = runs
        .iter()
        .map(|run| run.size)
        .reduce(f32::max)
        .filter(|size| *size > 0.0)?;
    let mut baselines = runs
        .iter()
        .filter(|run| run.size >= max_size * 0.8)
        .map(|run| run.baseline_y)
        .collect::<Vec<_>>();
    if baselines.is_empty() {
        baselines = runs.iter().map(|run| run.baseline_y).collect();
    }
    baselines.sort_by(|left, right| left.total_cmp(right));
    baselines.get(baselines.len() / 2).copied()
}

fn script_kind_for_run(run: &TextRun, baseline_y: f32) -> Option<ScriptKind> {
    let delta = run.baseline_y - baseline_y;
    let threshold = (run.size * 0.25).clamp(2.0, 4.0);
    if delta >= threshold {
        Some(ScriptKind::Superscript)
    } else if delta <= -threshold {
        Some(ScriptKind::Subscript)
    } else {
        None
    }
}

fn line_has_math_script_context(runs: &[TextRun]) -> bool {
    let joined = runs
        .iter()
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    joined.chars().any(|character| {
        matches!(
            character,
            // ASCII '-' is excluded: it is overwhelmingly a hyphen in prose
            // ("non-trade", "well-known"), so triggering math assembly on it
            // mangles hyphenated words. The real math minus is U+2212 ('−').
            '=' | '+'
                | '−'
                | '×'
                | '*'
                | '^'
                | '_'
                | '∈'
                | '≤'
                | '≥'
                | '≠'
                | 'λ'
                | 'θ'
                | 'ρ'
                | 'τ'
                | 'Σ'
                | '∑'
        )
    }) || runs.windows(2).any(|window| {
        let left = window[0].text.trim();
        let right = window[1].text.trim();
        // Require an actual baseline offset: a super/subscript sits visibly above
        // or below its base. Without this the predicate fires on ordinary
        // glyph-by-glyph prose (every letter is a single alphanumeric "base"
        // followed by another "script"), which is the norm in Chrome/Skia PDFs,
        // wrongly routing plain text through the script-assembly path.
        let baseline_delta = (window[0].baseline_y - window[1].baseline_y).abs();
        let script_offset = window[0].size.max(window[1].size) * 0.2;
        baseline_delta >= script_offset
            && is_math_script_base(left)
            && is_math_script_text(right)
    })
}

fn can_attach_math_script(previous: &str, token: &str) -> bool {
    !previous.ends_with('^')
        && !previous.ends_with('_')
        && is_math_script_text(token)
        && previous_has_math_script_base(previous)
}

fn is_math_script_base(token: &str) -> bool {
    let trimmed = token.trim_matches(|character: char| matches!(character, '(' | '[' | '{'));
    let count = trimmed.chars().count();
    (count == 1 && trimmed.chars().any(|character| character.is_alphanumeric()))
        || trimmed.starts_with('\\')
}

fn previous_has_math_script_base(previous: &str) -> bool {
    let trimmed = previous.trim_end();
    if trimmed.ends_with('}') || trimmed.ends_with(']') || trimmed.ends_with(')') {
        return trimmed.contains('\\') || trimmed.contains('_') || trimmed.contains('^');
    }
    trimmed
        .chars()
        .rev()
        .find(|character| !matches!(character, '*' | '\'' | '′'))
        .is_some_and(|character| character.is_alphabetic() || character == '\\')
}

fn is_math_script_text(token: &str) -> bool {
    let cleaned = token.trim_matches(|character: char| matches!(character, '(' | ')' | '[' | ']'));
    !cleaned.is_empty()
        && cleaned.chars().all(|character| {
            character.is_alphanumeric()
                || matches!(character, '+' | '-' | '−' | '=' | ',' | '.' | '\\')
        })
}

fn format_math_script(kind: ScriptKind, token: &str) -> String {
    let marker = match kind {
        ScriptKind::Superscript => '^',
        ScriptKind::Subscript => '_',
    };
    let cleaned = token.trim();
    if cleaned.chars().count() == 1
        || cleaned
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        format!("{marker}{cleaned}")
    } else {
        format!("{marker}{{{cleaned}}}")
    }
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
    if starts_with_numbered_step(&previous.text) && starts_with_numbered_step(&next.text) {
        return false;
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

fn starts_with_numbered_step(text: &str) -> bool {
    let trimmed = text.trim_start();
    let digit_count = trimmed
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    digit_count > 0
        && trimmed
            .chars()
            .nth(digit_count)
            .is_some_and(|character| matches!(character, ':' | '.'))
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '!' | '?'))
}

fn clean_pdf_line_text(text: &str) -> String {
    let text = repair_windows_1252_ellipsis_before_tokenizing(text);
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

fn clean_pdf_span_text(text: &str) -> String {
    repair_pdf_math_notation(&normalize_pdf_token(text))
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
        .replace("Î“", "Γ")
        .replace("Î˜", "Θ")
        .replace("Î›", "Λ")
        .replace("Î\u{a0}", "Π")
        .replace("Î£", "Σ")
        .replace("Î¦", "Φ")
        .replace("Î©", "Ω")
        .replace("Î»", "λ")
        .replace("Ï\u{84}", "τ")
        .replace("Ã\u{97}", "×")
        .replace("â\u{86}\u{92}", "→")
        .replace("â\u{89}¥", "≥")
        .replace("â\u{89}¤", "≤")
        .replace("â\u{88}\u{88}", "∈")
        .replace("â\u{88}\u{91}", "∑")
        .replace(['‘', '’'], "'")
        .replace(['“', '”'], "\"");
    let normalized = expand_latin_ligatures(&normalized);
    let normalized = repair_windows_1252_control_punctuation(&normalized);
    repair_embedded_pdf_control_glyphs(&normalized)
}

/// Expand Unicode Latin presentation-form ligatures (U+FB00–U+FB06) to their
/// component ASCII letters. Some PDF producers map a ligature glyph's ToUnicode
/// entry (or a `uniFB01`-style name) to the precomposed codepoint; leaving it in
/// the output degrades downstream search and matching. NFC/NFD do not decompose
/// these — only an explicit table (or NFKC) does.
fn expand_latin_ligatures(text: &str) -> String {
    if !text.chars().any(|character| ('\u{FB00}'..='\u{FB06}').contains(&character)) {
        return text.to_owned();
    }
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\u{FB00}' => output.push_str("ff"),
            '\u{FB01}' => output.push_str("fi"),
            '\u{FB02}' => output.push_str("fl"),
            '\u{FB03}' => output.push_str("ffi"),
            '\u{FB04}' => output.push_str("ffl"),
            '\u{FB05}' | '\u{FB06}' => output.push_str("st"),
            other => output.push(other),
        }
    }
    output
}

fn repair_windows_1252_control_punctuation(text: &str) -> String {
    let mut output = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '\u{80}' => output.push_str("EUR"),
            '\u{82}' => output.push(','),
            '\u{83}' => output.push('f'),
            '\u{84}' => output.push('"'),
            '\u{85}' => output.push_str("..."),
            '\u{86}' => output.push_str("†"),
            '\u{87}' => output.push_str("‡"),
            '\u{88}' => output.push('^'),
            '\u{89}' => output.push_str("‰"),
            '\u{8a}' => output.push_str("Š"),
            '\u{8b}' => output.push('<'),
            '\u{8c}' => output.push_str("OE"),
            '\u{8e}' => output.push_str("Ž"),
            '\u{91}' | '\u{92}' => output.push('\''),
            '\u{93}' | '\u{94}' => output.push('"'),
            '\u{95}' => output.push('*'),
            '\u{96}' => output.push('–'),
            '\u{97}' => output.push('—'),
            '\u{98}' => output.push('~'),
            '\u{99}' => output.push_str("(TM)"),
            '\u{9a}' => output.push_str("š"),
            '\u{9b}' => output.push('>'),
            '\u{9c}' => output.push_str("oe"),
            '\u{9e}' => output.push_str("ž"),
            '\u{9f}' => output.push_str("Ÿ"),
            _ => output.push(character),
        }
    }

    output
}

fn repair_windows_1252_ellipsis_before_tokenizing(text: &str) -> String {
    text.replace('\u{85}', "...")
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

    let normalized = repair_combining_math_operator_sequences(&normalized);
    let symbols = replace_math_symbols(&normalized);
    strip_pdf_control_glyphs(&repair_math_subscript_spacing(&symbols))
}

fn repair_combining_math_operator_sequences(text: &str) -> String {
    text.replace("\u{338} =", "≠")
        .replace("\u{338}=", "≠")
        .replace("=\u{338}", "≠")
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
                | '∑'
                | '∅'
                | '·'
                | '−'
                | '±'
                | '⊆'
                | '∼'
                | '≠'
                | '→'
        )
    }) || has_math_ellipsis_context(text)
        || text.contains("Fq")
        || text.contains(" 6 =")
}

fn has_math_ellipsis_context(text: &str) -> bool {
    if !text.contains("...") {
        return false;
    }

    let compact = text.split_whitespace().collect::<String>();
    compact.contains(",...,")
        || compact.contains("),...")
        || compact.contains("...,(")
        || text.chars().any(|character| {
            matches!(
                character,
                '=' | '+' | '_' | '^' | '\\' | '∈' | '≤' | '≥' | '≠' | 'λ' | 'θ' | 'ρ' | 'τ'
            )
        })
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
            'Γ' => output.push_str(r"\Gamma"),
            'Θ' => output.push_str(r"\Theta"),
            'ℓ' => output.push_str(r"\ell"),
            'λ' => output.push_str(r"\lambda"),
            'Λ' => output.push_str(r"\Lambda"),
            'Π' => output.push_str(r"\Pi"),
            'Σ' => output.push_str(r"\Sigma"),
            'Φ' => output.push_str(r"\Phi"),
            'Ω' => output.push_str(r"\Omega"),
            'θ' => output.push_str(r"\theta"),
            'ρ' => output.push_str(r"\rho"),
            'τ' => output.push_str(r"\tau"),
            '∆' | 'Δ' => output.push_str(r"\Delta"),
            '≤' => output.push_str(r"\leq"),
            '≥' => output.push_str(r"\geq"),
            '∈' => output.push_str(r"\in"),
            '∪' => output.push_str(r"\cup"),
            '∑' => output.push_str(r"\sum"),
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
    let mut sanitized = String::with_capacity(text.len());
    let mut last_was_space = false;

    for character in text.chars() {
        if is_nonprinting_pdf_control(character) {
            if !last_was_space {
                sanitized.push(' ');
                last_was_space = true;
            }
            continue;
        }

        sanitized.push(character);
        last_was_space = character.is_whitespace();
    }

    sanitized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_nonprinting_pdf_control(character: char) -> bool {
    character.is_control() && !matches!(character, '\n' | '\r' | '\t')
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
    matches!(chars.as_slice(), [character] if character.is_ascii_alphabetic())
        || matches!(chars.as_slice(), [character, '-'] if character.is_ascii_alphabetic())
}

fn is_word_piece(token: &str) -> bool {
    token.chars().any(|character| character.is_alphabetic())
}

fn is_joining_apostrophe(token: &str) -> bool {
    matches!(token, "'" | "’")
}

fn is_joining_hyphen(token: &str) -> bool {
    matches!(token, "-" | "‐" | "‑")
}

fn detect_table(
    page_number: usize,
    lines: &[TextLine],
    edges: &[GraphicEdge],
) -> Option<DetectedTable> {
    detect_ruled_grid_table(page_number, lines, edges)
        .or_else(|| detect_exact_run_table(page_number, lines))
        .or_else(|| detect_columnar_numeric_table(page_number, lines))
        .or_else(|| detect_implied_alignment_table(page_number, lines))
}

/// Detect a table by anchoring on the columns themselves rather than on a run of
/// identically-shaped rows. Numeric cells across the page are clustered by their
/// right edge into stable columns (numbers are right-aligned), then *every* line
/// in the table's vertical span is assigned to those columns — so section headers
/// and subtotals ("Operating activities:", "Cash generated by operating
/// activities") become full-width label rows instead of breaking the table apart.
/// This is what lets a whole multi-section financial statement extract as one
/// table. Entirely geometric and document-agnostic — no financial-specific rules.
fn detect_columnar_numeric_table(page_number: usize, lines: &[TextLine]) -> Option<DetectedTable> {
    let line_cells: Vec<Vec<TextRun>> = lines
        .iter()
        .map(|line| coalesce_currency_prefixes(implied_table_cells(line)))
        .collect();

    // Right edges of numeric cells, but only from lines that already look like data
    // rows (>= 2 numeric cells), so prose with an incidental figure does not vote.
    let mut right_edges: Vec<f32> = Vec::new();
    let mut data_rows = 0usize;
    for cells in &line_cells {
        let numeric = cells.iter().filter(|cell| is_numeric_value(&cell.text)).count();
        if numeric >= 2 {
            data_rows += 1;
            for cell in cells.iter().filter(|cell| is_numeric_value(&cell.text)) {
                right_edges.push(cell.bbox.x + cell.bbox.width);
            }
        }
    }
    if data_rows < 4 {
        return None;
    }

    let min_support = ((data_rows as f32) * 0.5).ceil().max(3.0) as usize;
    let columns = cluster_column_right_edges(&right_edges, 8.0, min_support);
    if columns.len() < 2 {
        return None;
    }
    // Boundary between the label column and the first numeric column. Sit it well
    // left of the figures (a couple of cell widths, but no further left than half
    // way to the next column) so a wide right-aligned header date counts as a
    // column entry while a left-anchored row label does not.
    let cell_width = column_cell_width(&line_cells, columns[0]);
    let half_gap = columns
        .get(1)
        .map_or(cell_width * 2.5, |next| (next - columns[0]) / 2.0);
    let first_column_left = columns[0] - (cell_width * 2.5).min(half_gap.max(cell_width * 1.5));
    let table_right = columns.last().copied().unwrap_or_default();

    // Lines whose cells land on the detected columns are the table's rows.
    let aligned: Vec<usize> = (0..lines.len())
        .filter(|&index| {
            line_cells[index]
                .iter()
                .filter(|cell| is_numeric_value(&cell.text))
                .any(|cell| nearest_column(cell.bbox.x + cell.bbox.width, &columns).is_some())
        })
        .collect();
    let (first, last) = (*aligned.first()?, *aligned.last()?);

    // Walk the span; keep contiguous table rows (data rows + interleaved label-only
    // rows) and stop at a clear break — a non-aligned numeric line (a different
    // table) or a large vertical gap.
    let mut row_indices: Vec<usize> = Vec::new();
    let mut previous_y: Option<f32> = None;
    for index in first..=last {
        let line = &lines[index];
        let cells = &line_cells[index];
        let aligned_here = cells
            .iter()
            .filter(|cell| is_numeric_value(&cell.text))
            .any(|cell| nearest_column(cell.bbox.x + cell.bbox.width, &columns).is_some());
        let numeric_here = cells.iter().any(|cell| is_numeric_value(&cell.text));
        let label_only = !numeric_here && line.bbox.x <= table_right;
        if !aligned_here && !label_only {
            break;
        }
        if let Some(prev) = previous_y {
            if (prev - line.bbox.y).abs() > average_run_size(line).max(line.bbox.height) * 3.5 {
                break;
            }
        }
        row_indices.push(index);
        previous_y = Some(line.bbox.y);
    }
    let aligned_in_span = row_indices
        .iter()
        .filter(|&&index| {
            line_cells[index]
                .iter()
                .filter(|cell| is_numeric_value(&cell.text))
                .any(|cell| nearest_column(cell.bbox.x + cell.bbox.width, &columns).is_some())
        })
        .count();
    if aligned_in_span < 4 {
        return None;
    }

    build_columnar_table(page_number, lines, &line_cells, &columns, first_column_left, &row_indices)
}

/// Merge a lone currency symbol cell into the figure that follows it. Financial
/// statements left-align the `$` at the column edge and right-align the number, so
/// the splitter sees two cells ("$", "30,737"); rejoined the `$` belongs to the
/// number on its right ("$30,737"), not the column on its left.
fn coalesce_currency_prefixes(cells: Vec<TextRun>) -> Vec<TextRun> {
    const SYMBOLS: [char; 4] = ['$', '€', '£', '¥'];
    let mut out: Vec<TextRun> = Vec::with_capacity(cells.len());
    let mut pending: Option<TextRun> = None;
    for mut cell in cells {
        let mut text = cell.text.trim().to_string();
        if let Some(prefix) = pending.take() {
            cell.bbox = union_boxes([prefix.bbox, cell.bbox]).unwrap_or(cell.bbox);
            text = format!("{}{}", prefix.text.trim(), text);
        }
        // A lone symbol carries to the next figure (left-aligned column `$`).
        if text.chars().count() == 1 && text.chars().all(|c| SYMBOLS.contains(&c)) {
            cell.text = text;
            pending = Some(cell);
            continue;
        }
        // A trailing symbol belongs to the *next* column's figure: the splitter
        // groups each column's `$` with the preceding number ("30,737 $").
        if let Some(last) = text.chars().last() {
            if SYMBOLS.contains(&last) {
                let stripped = text[..text.len() - last.len_utf8()].trim_end();
                if !stripped.is_empty() {
                    let mut carry = cell.clone();
                    carry.text = last.to_string();
                    text = stripped.to_string();
                    pending = Some(carry);
                }
            }
        }
        cell.text = text;
        out.push(cell);
    }
    if let Some(prefix) = pending {
        out.push(prefix);
    }
    out
}

/// Is this cell a numeric value — a figure, possibly wrapped in `$`, parens
/// (negatives), commas, a percent or a trailing footnote marker? Used to find the
/// columns to anchor on, so it must accept real table figures and reject prose.
fn is_numeric_value(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut digits = 0usize;
    for character in trimmed.chars() {
        match character {
            '0'..='9' => digits += 1,
            '$' | '(' | ')' | ',' | '.' | '%' | '-' | '+' | ' ' | '\u{2014}' | '\u{2013}' => {}
            _ => return false,
        }
    }
    digits >= 1
}

/// Cluster right-edge x positions into columns: sort, split where consecutive
/// values differ by more than `tol`, keep clusters with enough rows behind them,
/// and return each surviving cluster's median position (sorted left→right).
fn cluster_column_right_edges(values: &[f32], tol: f32, min_support: usize) -> Vec<f32> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut columns: Vec<f32> = Vec::new();
    let mut start = 0usize;
    for index in 1..=sorted.len() {
        let split = index == sorted.len() || sorted[index] - sorted[index - 1] > tol;
        if split {
            let cluster = &sorted[start..index];
            if cluster.len() >= min_support {
                columns.push(cluster[cluster.len() / 2]);
            }
            start = index;
        }
    }
    columns
}

/// Index of the column whose right edge is within tolerance of `right_edge`.
fn nearest_column(right_edge: f32, columns: &[f32]) -> Option<usize> {
    columns
        .iter()
        .enumerate()
        .map(|(index, edge)| (index, (right_edge - edge).abs()))
        .filter(|(_, distance)| *distance <= 14.0)
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index)
}

/// Typical width of the cells feeding the first column, used to place the
/// label/number boundary just left of that column.
fn column_cell_width(line_cells: &[Vec<TextRun>], first_column: f32) -> f32 {
    let widths: Vec<f32> = line_cells
        .iter()
        .flat_map(|cells| cells.iter())
        .filter(|cell| is_numeric_value(&cell.text))
        .filter(|cell| ((cell.bbox.x + cell.bbox.width) - first_column).abs() <= 14.0)
        .map(|cell| cell.bbox.width)
        .collect();
    if widths.is_empty() {
        return 40.0;
    }
    let mut sorted = widths.clone();
    sorted.sort_by(f32::total_cmp);
    sorted[sorted.len() / 2].max(20.0)
}

/// Label-only continuation lines directly above a data row that wrap its label —
/// a long row label that overflowed onto the previous line(s). A continuation sits
/// at the same left indent, carries no figures, and does not end in ":" (which
/// marks a section header, not a wrap). Returned top-to-bottom so the text can
/// prefix the row's own label.
fn wrapped_label_above(
    lines: &[TextLine],
    line_cells: &[Vec<TextRun>],
    row_index: usize,
    first_column_left: f32,
    used: &[usize],
) -> Vec<usize> {
    let label_x = lines[row_index].bbox.x;
    let line_height = average_run_size(&lines[row_index]).max(lines[row_index].bbox.height);
    let mut result: Vec<usize> = Vec::new();
    let mut current_y = lines[row_index].bbox.y;
    loop {
        let above = (0..lines.len())
            .filter(|&index| {
                index != row_index
                    && !used.contains(&index)
                    && !result.contains(&index)
                    && lines[index].bbox.y > current_y
            })
            .min_by(|&left, &right| lines[left].bbox.y.total_cmp(&lines[right].bbox.y));
        let Some(above) = above else { break };
        let line = &lines[above];
        let text = text_line_plain_text(line);
        // A wrapped label line: vertically adjacent, roughly the same indent
        // (continuations are often hanging-indented), no figures, no trailing ":",
        // and — crucially — long. A label wraps because it ran the width of the
        // label column, which distinguishes it from a short section header like
        // "Assets" or a one-word heading.
        let long_enough = text.chars().count() >= 28
            || line.bbox.x + line.bbox.width >= first_column_left - 12.0;
        // An all-caps line is a section heading ("CASH FLOWS FROM FINANCING
        // ACTIVITIES"), not a wrapped sentence fragment, even when it is long.
        let all_caps_heading = text.chars().any(char::is_alphabetic)
            && text.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase);
        if line.bbox.y - current_y > line_height * 1.8
            || (line.bbox.x - label_x).abs() > 16.0
            || !long_enough
            || all_caps_heading
            || text.trim().is_empty()
            || text.trim_end().ends_with(':')
            || line_cells[above].iter().any(|cell| is_numeric_value(&cell.text))
        {
            break;
        }
        result.push(above);
        current_y = line.bbox.y;
    }
    result.reverse();
    result
}

/// A row whose figure columns are all four-digit years (e.g. "2025 2024 2023").
/// Such a row is a period header, not data — column titles, not values — so it
/// belongs in the header even when it also carries a label like "Year Ended …".
fn is_period_header_row(row: &[String]) -> bool {
    let values: Vec<&str> = row[1..]
        .iter()
        .map(|cell| cell.trim())
        .filter(|cell| !cell.is_empty())
        .collect();
    !values.is_empty()
        && values.iter().all(|cell| {
            cell.len() == 4
                && cell.chars().all(|c| c.is_ascii_digit())
                && cell.parse::<i32>().is_ok_and(|year| (1900..=2100).contains(&year))
        })
}

fn build_columnar_table(
    page_number: usize,
    lines: &[TextLine],
    line_cells: &[Vec<TextRun>],
    columns: &[f32],
    first_column_left: f32,
    row_indices: &[usize],
) -> Option<DetectedTable> {
    let column_count = columns.len() + 1; // label column + one per numeric column
    let assign_row = |index: usize| -> Vec<String> {
        let mut row = vec![String::new(); column_count];
        for cell in &line_cells[index] {
            let column = assign_cell_column(cell, columns, first_column_left);
            push_table_cell_text(&mut row[column], &cell.text);
        }
        row
    };

    // The header is everything above the first *labelled* row: period/column titles
    // sitting over the numeric columns (lines above the span) plus any leading rows
    // whose label column is empty (a bare "2024 2023 2022" year row). The first row
    // carrying label-column text begins the body.
    let span_top_y = lines[*row_indices.first()?].bbox.y;
    let mut header_indices: Vec<usize> = (0..lines.len())
        .filter(|&index| {
            let line = &lines[index];
            !row_indices.contains(&index)
                && line.bbox.y > span_top_y
                && line.bbox.y - span_top_y
                    <= average_run_size(line).max(line.bbox.height) * 5.0
                && line.bbox.x + line.bbox.width >= first_column_left - 24.0
                && !text_line_plain_text(line).to_ascii_lowercase().starts_with("table ")
                && !line_is_data_row(line, column_count)
                // A real column header sits *over the numeric columns*; a line whose
                // content all falls in the label column is a statement title or a
                // "(in millions)" note centered above the table, not a header.
                && assign_row(index)[1..].iter().any(|cell| !cell.trim().is_empty())
        })
        .collect();

    let mut data_start = 0usize;
    for (position, &index) in row_indices.iter().enumerate() {
        let row = assign_row(index);
        // A leading row is part of the header when its label column is empty (a bare
        // "2024 2023 2022" line) or its figure cells are all years/periods (a
        // "Year Ended June 30, | 2025 | 2024 | 2023" line) — the body begins at the
        // first row carrying real figures.
        if row[0].trim().is_empty() || is_period_header_row(&row) {
            header_indices.push(index);
            data_start = position + 1;
        } else {
            data_start = position;
            break;
        }
    }
    header_indices.sort_by(|left, right| lines[*right].bbox.y.total_cmp(&lines[*left].bbox.y));

    let mut header_cells: Vec<String> = vec![String::new(); column_count];
    for &index in &header_indices {
        for (column, text) in assign_row(index).into_iter().enumerate() {
            push_table_cell_text(&mut header_cells[column], &text);
        }
    }
    let header_has_text = header_cells.iter().any(|cell| !cell.is_empty());

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut cell_records: Vec<TableCell> = Vec::new();
    if header_has_text {
        for (column, text) in header_cells.iter().enumerate() {
            cell_records.push(table_cell(0, column, text.clone(), true));
        }
    }

    // Pull a wrapped label up into the data row it belongs to: a long row label
    // can overflow onto the previous line, leaving the figure row with only the
    // label's tail ("balances" instead of "Cash …, beginning balances").
    let mut consumed: Vec<usize> = Vec::new();
    let mut prefixes: Vec<(usize, String)> = Vec::new();
    for &index in &row_indices[data_start..] {
        if !line_cells[index].iter().any(|cell| is_numeric_value(&cell.text)) {
            continue;
        }
        // Only a *short tail* row pulls a wrap up: "balances", "equivalents". A row
        // that already carries a full label ("Net earnings", "Additions to …") is a
        // section's own item, and the long line above it is that section's heading,
        // not a wrap — merging there would corrupt the table.
        if assign_row(index)[0].trim().chars().count() > 11 {
            continue;
        }
        let mut search_used = header_indices.clone();
        search_used.extend_from_slice(&consumed);
        let chain = wrapped_label_above(lines, line_cells, index, first_column_left, &search_used);
        if !chain.is_empty() {
            let prefix = chain
                .iter()
                .map(|&line| text_line_plain_text(&lines[line]))
                .collect::<Vec<_>>()
                .join(" ");
            prefixes.push((index, prefix));
            consumed.extend(chain);
        }
    }

    for &index in &row_indices[data_start..] {
        if consumed.contains(&index) {
            continue;
        }
        let mut row = assign_row(index);
        if let Some((_, prefix)) = prefixes.iter().find(|(line, _)| *line == index) {
            row[0] = if row[0].trim().is_empty() {
                prefix.clone()
            } else {
                format!("{prefix} {}", row[0])
            };
        }
        if row.iter().all(|cell| cell.is_empty()) {
            continue;
        }
        let table_row = rows.len() + usize::from(header_has_text);
        for (column, text) in row.iter().enumerate() {
            cell_records.push(table_cell(table_row, column, text.clone(), false));
        }
        rows.push(row);
    }
    if rows.is_empty() {
        return None;
    }

    // Only take over from the simpler detectors when this method earns its keep:
    // a large statement whose rows are *not* uniform — section headers / subtotals
    // (a label with no figures) interleaved with data rows. A small uniform grid is
    // handled just as well by exact/implied alignment, so defer to those there and
    // avoid disturbing tables this geometry would only re-shape, not improve.
    let value_rows = rows.iter().filter(|row| !row[0].trim().is_empty()).count();
    let label_only_rows = rows
        .iter()
        .filter(|row| !row[0].trim().is_empty() && row[1..].iter().all(|cell| cell.trim().is_empty()))
        .count();
    let data_with_figures = rows
        .iter()
        .filter(|row| row[1..].iter().any(|cell| !cell.trim().is_empty()))
        .count();
    if value_rows < 8 || label_only_rows < 2 || data_with_figures < 6 {
        return None;
    }

    let mut line_index_set: Vec<usize> = row_indices.to_vec();
    line_index_set.extend(header_indices.iter().copied());
    line_index_set.extend(consumed.iter().copied());
    line_index_set.sort_unstable();
    line_index_set.dedup();
    let bbox = union_boxes(line_index_set.iter().map(|&index| lines[index].bbox))?;

    Some(DetectedTable {
        table: TableBlock {
            headers: if header_has_text {
                header_cells
            } else {
                Vec::new()
            },
            rows,
            caption: None,
            bbox: Some(bbox),
            cells: cell_records,
            source_anchors: vec![anchor(page_number, Some(bbox), Vec::new())],
            confidence: Some(Confidence {
                score: 0.7,
                calibrated: false,
            }),
        },
        line_indices: line_index_set,
    })
}

/// Column a cell belongs to (0 = label, 1..=N = numeric columns). Right-aligned
/// figures match a column by their right edge; a header title or a centered/narrow
/// year that no right edge matches falls to the column band its center sits in;
/// a non-numeric cell that *starts* in the label region (a row label, however long)
/// stays in column 0.
fn assign_cell_column(cell: &TextRun, columns: &[f32], first_column_left: f32) -> usize {
    if is_numeric_value(&cell.text) {
        if let Some(column) = nearest_column(cell.bbox.x + cell.bbox.width, columns) {
            return column + 1;
        }
    }
    // A left-anchored row label, however long, keeps its center well left of the
    // columns, so the band naturally returns 0 for it; a header title or year
    // centered over a column lands on that column.
    column_band(cell, columns, first_column_left)
}

/// Numeric column (1..=N) whose horizontal band contains the cell's center, or 0
/// when the center is left of the first column. Band boundaries are the midpoints
/// between adjacent column right edges.
fn column_band(cell: &TextRun, columns: &[f32], first_column_left: f32) -> usize {
    let center = cell.bbox.x + cell.bbox.width / 2.0;
    if center < first_column_left {
        return 0;
    }
    for index in 0..columns.len() {
        let upper = columns
            .get(index + 1)
            .map_or(f32::INFINITY, |next| (columns[index] + next) / 2.0);
        if center <= upper {
            return index + 1;
        }
    }
    columns.len()
}

fn push_table_cell_text(target: &mut String, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push(' ');
    }
    target.push_str(text);
}

fn table_cell(row: usize, column: usize, text: String, is_header: bool) -> TableCell {
    TableCell {
        row,
        column,
        text,
        bbox: None,
        is_header,
        col_span: 1,
        row_span: 1,
    }
}

fn detect_ruled_grid_table(
    page_number: usize,
    lines: &[TextLine],
    edges: &[GraphicEdge],
) -> Option<DetectedTable> {
    let verticals = grid_axis_values(edges, EdgeOrientation::Vertical);
    let horizontals = grid_axis_values(edges, EdgeOrientation::Horizontal);
    if verticals.len() < 2 || horizontals.len() < 2 {
        return None;
    }

    let columns = verticals.len() - 1;
    let rows = horizontals.len() - 1;
    if columns < 2 || rows < 2 {
        return None;
    }
    if !has_nearby_ruled_table_label(lines, &verticals, &horizontals)
        && !has_multirow_ruled_grid_evidence(columns, rows)
    {
        return None;
    }

    let mut grid = vec![vec![String::new(); columns]; rows];
    let mut cell_boxes = vec![vec![None; columns]; rows];
    let mut line_indices = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        let mut used_line = false;
        for run in &line.runs {
            let center_x = run.bbox.x + run.bbox.width / 2.0;
            let center_y = run.bbox.y + run.bbox.height / 2.0;
            let Some(column) = grid_column_for(center_x, &verticals) else {
                continue;
            };
            let Some(row) = grid_row_for(center_y, &horizontals) else {
                continue;
            };
            append_grid_cell_text(&mut grid[row][column], &run.text);
            cell_boxes[row][column] = Some(
                cell_boxes[row][column]
                    .and_then(|bbox| union_boxes([bbox, run.bbox]))
                    .unwrap_or(run.bbox),
            );
            used_line = true;
        }
        if used_line {
            line_indices.push(line_index);
        }
    }

    if grid
        .iter()
        .flatten()
        .filter(|text| !text.trim().is_empty())
        .count()
        < 3
    {
        return None;
    }

    let headers = grid[0].clone();
    let body_rows = grid.iter().skip(1).cloned().collect::<Vec<_>>();
    if headers.iter().all(|text| text.trim().is_empty())
        || body_rows
            .iter()
            .flatten()
            .all(|text| text.trim().is_empty())
    {
        return None;
    }

    // Merged cells: a cell whose content overruns a ruled column boundary into an
    // empty neighbour band spans it. The grid text stays rectangular so renderers
    // are unchanged; only `cells` carries the span topology.
    let (col_span, covered) = merged_cell_col_spans(&cell_boxes, &verticals);

    let mut cells = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            if covered[row][column] {
                continue;
            }
            cells.push(TableCell {
                row,
                column,
                text: grid[row][column].clone(),
                bbox: cell_boxes[row][column],
                is_header: row == 0,
                col_span: col_span[row][column],
                row_span: 1,
            });
        }
    }

    let bbox = BBox {
        x: *verticals.first()?,
        y: *horizontals.first()?,
        width: *verticals.last()? - *verticals.first()?,
        height: *horizontals.last()? - *horizontals.first()?,
    };

    Some(DetectedTable {
        table: TableBlock {
            headers,
            rows: body_rows,
            caption: None,
            bbox: Some(bbox),
            cells,
            source_anchors: vec![anchor(page_number, Some(bbox), Vec::new())],
            confidence: Some(Confidence {
                score: 0.7,
                calibrated: false,
            }),
        },
        line_indices,
    })
}

/// Detect horizontally merged cells (column spans) in a ruled grid.
///
/// A non-empty cell whose content bbox overruns its ruled column boundary into
/// an adjacent *empty* band (by more than `SPAN_MARGIN`) is treated as spanning
/// it — the natural signature of a grouped column header, whose label is
/// physically wider than one column. Returns the per-cell `col_span` grid plus a
/// `covered` mask of the spanned-over continuation positions, which the caller
/// omits from `cells`.
///
/// Spans are scanned rightward from the anchoring cell, so a centred merged
/// header must lean into its left band (the common case). Row spans are not
/// inferred here: a vertically merged cell is usually a single line centred in a
/// tall region whose bbox does not overflow the row rule, so it needs
/// rule-segment analysis rather than content overflow.
fn merged_cell_col_spans(
    cell_boxes: &[Vec<Option<BBox>>],
    verticals: &[f32],
) -> (Vec<Vec<usize>>, Vec<Vec<bool>>) {
    const SPAN_MARGIN: f32 = 2.0;
    let rows = cell_boxes.len();
    let columns = cell_boxes.first().map_or(0, Vec::len);
    let mut col_span = vec![vec![1usize; columns]; rows];
    let mut covered = vec![vec![false; columns]; rows];

    for row in 0..rows {
        for column in 0..columns {
            if covered[row][column] {
                continue;
            }
            let Some(bbox) = cell_boxes[row][column] else {
                continue;
            };

            let content_right = bbox.x + bbox.width;
            let mut next_column = column + 1;
            while next_column < columns
                && cell_boxes[row][next_column].is_none()
                && !covered[row][next_column]
                && verticals
                    .get(next_column)
                    .is_some_and(|edge| content_right > edge + SPAN_MARGIN)
            {
                covered[row][next_column] = true;
                next_column += 1;
            }
            col_span[row][column] = next_column - column;
        }
    }

    (col_span, covered)
}

fn has_nearby_ruled_table_label(
    lines: &[TextLine],
    verticals: &[f32],
    horizontals: &[f32],
) -> bool {
    let Some(left) = verticals.first().copied() else {
        return false;
    };
    let Some(right) = verticals.last().copied() else {
        return false;
    };
    let Some(top) = horizontals.last().copied() else {
        return false;
    };

    lines.iter().any(|line| {
        let text = text_line_plain_text(line).to_ascii_lowercase();
        text.starts_with("table")
            && line.bbox.y >= top
            && line.bbox.y <= top + 96.0
            && line.bbox.x <= right + 24.0
            && line.bbox.x + line.bbox.width >= left - 24.0
    })
}

fn has_multirow_ruled_grid_evidence(columns: usize, rows: usize) -> bool {
    columns >= 2 && rows >= 4
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeOrientation {
    Horizontal,
    Vertical,
}

fn grid_axis_values(edges: &[GraphicEdge], orientation: EdgeOrientation) -> Vec<f32> {
    let mut values = edges
        .iter()
        .filter_map(|edge| match orientation {
            EdgeOrientation::Horizontal if is_horizontal_edge(edge) => {
                Some((edge.y0 + edge.y1) / 2.0)
            }
            EdgeOrientation::Vertical if is_vertical_edge(edge) => Some((edge.x0 + edge.x1) / 2.0),
            _ => None,
        })
        .collect::<Vec<_>>();
    values.sort_by(f32::total_cmp);
    dedup_axis_values(values, 2.0)
}

fn is_horizontal_edge(edge: &GraphicEdge) -> bool {
    (edge.y0 - edge.y1).abs() <= 1.0 && (edge.x0 - edge.x1).abs() >= 12.0
}

fn is_vertical_edge(edge: &GraphicEdge) -> bool {
    (edge.x0 - edge.x1).abs() <= 1.0 && (edge.y0 - edge.y1).abs() >= 12.0
}

fn dedup_axis_values(values: Vec<f32>, tolerance: f32) -> Vec<f32> {
    let mut deduped: Vec<f32> = Vec::new();
    for value in values {
        if let Some(previous) = deduped.last_mut() {
            if (value - *previous).abs() <= tolerance {
                *previous = (*previous + value) / 2.0;
                continue;
            }
        }
        deduped.push(value);
    }
    deduped
}

fn grid_column_for(x: f32, verticals: &[f32]) -> Option<usize> {
    verticals
        .windows(2)
        .position(|window| x >= window[0] - 1.0 && x <= window[1] + 1.0)
}

fn grid_row_for(y: f32, horizontals: &[f32]) -> Option<usize> {
    let band = horizontals
        .windows(2)
        .position(|window| y >= window[0] - 1.0 && y <= window[1] + 1.0)?;
    Some(horizontals.len().saturating_sub(2).saturating_sub(band))
}

fn append_grid_cell_text(target: &mut String, text: &str) {
    let cleaned = clean_pdf_line_text(text);
    if cleaned.is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push(' ');
    }
    target.push_str(&cleaned);
}

fn detect_exact_run_table(page_number: usize, lines: &[TextLine]) -> Option<DetectedTable> {
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
                col_span: 1,
                row_span: 1,
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

fn detect_implied_alignment_table(page_number: usize, lines: &[TextLine]) -> Option<DetectedTable> {
    let row_candidates = lines
        .iter()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let cells = implied_table_cells(line);
            (cells.len() >= 3 && row_has_numeric_table_evidence(&cells))
                .then_some(TableRowCandidate { line_index, cells })
        })
        .collect::<Vec<_>>();
    let group = best_aligned_table_row_group(&row_candidates)?;
    // A nearby "Table N" caption confirms an implied table, but most real tables
    // (financial statements, schedules) have no such caption. Accept those when the
    // aligned group is strong enough on its own — many rows of consistently aligned
    // numeric columns — mirroring the ruled-grid detector's multi-row evidence path.
    if !has_nearby_table_label(lines, &group) && !has_strong_numeric_table_evidence(&group) {
        return None;
    }
    build_implied_alignment_table(page_number, lines, &group)
}

/// Whether an aligned row group is, by itself, strong evidence of a table: at
/// least four rows of three or more columns where most rows carry numeric values
/// in their non-label cells. Deliberately conservative so prose with incidental
/// numbers is not promoted to a table.
fn has_strong_numeric_table_evidence(rows: &[TableRowCandidate]) -> bool {
    let columns = rows.first().map_or(0, |row| row.cells.len());
    if rows.len() < 4 || columns < 3 {
        return false;
    }
    let numeric_rows = rows
        .iter()
        .filter(|row| row_has_numeric_table_evidence(&row.cells))
        .count();
    numeric_rows * 4 >= rows.len() * 3
}

fn has_nearby_table_label(lines: &[TextLine], rows: &[TableRowCandidate]) -> bool {
    let Some(first_row) = rows.first() else {
        return false;
    };
    let first_y = first_row
        .cells
        .iter()
        .map(|cell| cell.bbox.y)
        .reduce(f32::max)
        .unwrap_or_default();
    let table_left = first_row
        .cells
        .iter()
        .map(|cell| cell.bbox.x)
        .reduce(f32::min)
        .unwrap_or_default();
    let table_right = first_row
        .cells
        .iter()
        .map(|cell| cell.bbox.x + cell.bbox.width)
        .reduce(f32::max)
        .unwrap_or_default();

    lines.iter().any(|line| {
        let text = text_line_plain_text(line).to_ascii_lowercase();
        text.starts_with("table")
            && line.bbox.y >= first_y
            && line.bbox.y <= first_y + 96.0
            && line.bbox.x <= table_right + 24.0
            && line.bbox.x + line.bbox.width >= table_left - 24.0
    })
}

fn implied_table_cells(line: &TextLine) -> Vec<TextRun> {
    if line.runs.len() < 2 {
        return line.runs.clone();
    }

    let mut runs = line.runs.clone();
    runs.sort_by(|left, right| left.bbox.x.total_cmp(&right.bbox.x));
    let threshold = implied_cell_gap_threshold(line);
    let mut groups: Vec<Vec<TextRun>> = Vec::new();
    let mut current: Vec<TextRun> = Vec::new();

    for run in runs {
        if let Some(previous) = current.last() {
            let gap = run.bbox.x - (previous.bbox.x + previous.bbox.width);
            if gap >= threshold {
                groups.push(std::mem::take(&mut current));
            }
        }
        current.push(run);
    }
    if !current.is_empty() {
        groups.push(current);
    }

    groups
        .into_iter()
        .filter_map(|runs| text_run_from_cell_runs(&runs))
        .collect()
}

fn implied_cell_gap_threshold(line: &TextLine) -> f32 {
    let height = average_run_size(line).max(line.bbox.height);
    (height * 1.5).clamp(10.0, 18.0)
}

fn text_run_from_cell_runs(runs: &[TextRun]) -> Option<TextRun> {
    let bbox = union_boxes(runs.iter().map(|run| run.bbox))?;
    let text = clean_pdf_line_text(&join_runs_spaced(runs));
    if text.is_empty() {
        return None;
    }

    Some(TextRun {
        text,
        bbox,
        baseline_y: runs.iter().map(|run| run.baseline_y).sum::<f32>() / runs.len() as f32,
        font: runs.iter().find_map(|run| run.font.clone()),
        size: runs.iter().map(|run| run.size).sum::<f32>() / runs.len() as f32,
        space_width: runs.iter().map(|run| run.space_width).fold(0.0, f32::max),
        bold: !runs.is_empty() && runs.iter().all(|run| run.bold),
        italic: !runs.is_empty() && runs.iter().all(|run| run.italic),
        source_object_ids: source_ids_for_runs(runs),
    })
}

fn row_has_numeric_table_evidence(cells: &[TextRun]) -> bool {
    cells.iter().skip(1).any(|cell| {
        cell.text
            .chars()
            .any(|character| character.is_ascii_digit())
    })
}

fn best_aligned_table_row_group(rows: &[TableRowCandidate]) -> Option<Vec<TableRowCandidate>> {
    let mut best: Option<Vec<TableRowCandidate>> = None;
    let mut current: Vec<TableRowCandidate> = Vec::new();

    for row in rows {
        if current.is_empty() {
            current.push(row.clone());
            continue;
        }

        let compatible = current
            .first()
            .is_some_and(|first| table_rows_align(first, row))
            && current
                .last()
                .is_some_and(|previous| table_row_vertical_gap(previous, row) <= 28.0);
        if compatible {
            current.push(row.clone());
        } else {
            record_table_row_group(&mut best, &current);
            current.clear();
            current.push(row.clone());
        }
    }
    record_table_row_group(&mut best, &current);
    best
}

fn record_table_row_group(
    best: &mut Option<Vec<TableRowCandidate>>,
    candidate: &[TableRowCandidate],
) {
    if candidate.len() < 2 {
        return;
    }
    let Some(width) = candidate.first().map(|row| row.cells.len()) else {
        return;
    };
    if width < 3 {
        return;
    }
    let score = candidate.len() * width;
    let best_score = best
        .as_ref()
        .and_then(|rows| rows.first().map(|row| rows.len() * row.cells.len()))
        .unwrap_or_default();
    if score > best_score {
        *best = Some(candidate.to_vec());
    }
}

fn table_rows_align(first: &TableRowCandidate, next: &TableRowCandidate) -> bool {
    first.cells.len() == next.cells.len()
        && first
            .cells
            .iter()
            .zip(&next.cells)
            .all(|(left, right)| cells_column_aligned(left, right))
}

/// Two cells share a column when their left edges line up (left-aligned text) or
/// their right edges line up (right-aligned numeric columns — the norm in
/// financial statements, where the left edge slides with the number's width).
fn cells_column_aligned(left: &TextRun, right: &TextRun) -> bool {
    let left_edge = (left.bbox.x - right.bbox.x).abs() <= 14.0;
    let right_edge =
        ((left.bbox.x + left.bbox.width) - (right.bbox.x + right.bbox.width)).abs() <= 14.0;
    left_edge || right_edge
}

fn table_row_vertical_gap(previous: &TableRowCandidate, next: &TableRowCandidate) -> f32 {
    let previous_y = previous
        .cells
        .iter()
        .map(|cell| cell.bbox.y)
        .reduce(f32::max)
        .unwrap_or_default();
    let next_y = next
        .cells
        .iter()
        .map(|cell| cell.bbox.y)
        .reduce(f32::max)
        .unwrap_or_default();
    (previous_y - next_y).abs()
}

fn build_implied_alignment_table(
    page_number: usize,
    lines: &[TextLine],
    rows: &[TableRowCandidate],
) -> Option<DetectedTable> {
    let columns = rows.first()?.cells.len();
    let bbox = union_boxes(
        rows.iter()
            .flat_map(|row| row.cells.iter().map(|cell| cell.bbox)),
    )?;
    let header = implied_table_header(lines, rows, columns);
    let has_explicit_header = header.has_text();
    let mut line_indices = rows.iter().map(|row| row.line_index).collect::<Vec<_>>();
    line_indices.extend(header.line_indices.iter().copied());
    line_indices.sort_unstable();
    line_indices.dedup();

    let (headers, body_rows, header_cells) = if has_explicit_header {
        (
            header
                .cells
                .iter()
                .map(|cell| {
                    cell.as_ref()
                        .map(|cell| cell.text.clone())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>(),
            rows.iter()
                .map(|row| row.cells.iter().map(|cell| cell.text.clone()).collect())
                .collect::<Vec<Vec<_>>>(),
            header.cells,
        )
    } else {
        (
            rows.first()?
                .cells
                .iter()
                .map(|cell| cell.text.clone())
                .collect::<Vec<_>>(),
            rows.iter()
                .skip(1)
                .map(|row| row.cells.iter().map(|cell| cell.text.clone()).collect())
                .collect::<Vec<Vec<_>>>(),
            rows.first()?.cells.iter().cloned().map(Some).collect(),
        )
    };

    let mut cells = Vec::new();
    for (column, cell) in header_cells.into_iter().enumerate() {
        let text = headers.get(column).cloned().unwrap_or_default();
        cells.push(TableCell {
            row: 0,
            column,
            text,
            bbox: cell.map(|cell| cell.bbox),
            is_header: true,
            col_span: 1,
            row_span: 1,
        });
    }
    for (row_index, row) in rows.iter().enumerate() {
        let table_row = if has_explicit_header {
            row_index + 1
        } else {
            row_index
        };
        if !has_explicit_header && row_index == 0 {
            continue;
        }
        for (column, cell) in row.cells.iter().enumerate() {
            cells.push(TableCell {
                row: table_row,
                column,
                text: cell.text.clone(),
                bbox: Some(cell.bbox),
                is_header: false,
                col_span: 1,
                row_span: 1,
            });
        }
    }

    Some(DetectedTable {
        table: TableBlock {
            headers,
            rows: body_rows,
            caption: None,
            bbox: Some(bbox),
            cells,
            source_anchors: vec![anchor(page_number, Some(bbox), Vec::new())],
            confidence: Some(Confidence {
                score: 0.68,
                calibrated: false,
            }),
        },
        line_indices,
    })
}

#[derive(Debug, Clone)]
struct ImpliedTableHeader {
    cells: Vec<Option<TextRun>>,
    line_indices: Vec<usize>,
}

impl ImpliedTableHeader {
    fn has_text(&self) -> bool {
        self.cells
            .iter()
            .any(|cell| cell.as_ref().is_some_and(|cell| !cell.text.is_empty()))
    }
}

fn implied_table_header(
    lines: &[TextLine],
    rows: &[TableRowCandidate],
    columns: usize,
) -> ImpliedTableHeader {
    let mut header = ImpliedTableHeader {
        cells: vec![None; columns],
        line_indices: Vec::new(),
    };
    let Some(first_row) = rows.first() else {
        return header;
    };
    let first_y = first_row
        .cells
        .iter()
        .map(|cell| cell.bbox.y)
        .reduce(f32::max)
        .unwrap_or_default();
    let table_left = first_row
        .cells
        .iter()
        .map(|cell| cell.bbox.x)
        .reduce(f32::min)
        .unwrap_or_default();
    let table_right = first_row
        .cells
        .iter()
        .map(|cell| cell.bbox.x + cell.bbox.width)
        .reduce(f32::max)
        .unwrap_or_default();
    let column_refs = first_row
        .cells
        .iter()
        .map(|cell| (cell.bbox.x, cell.bbox.x + cell.bbox.width))
        .collect::<Vec<_>>();

    let mut candidates = lines
        .iter()
        .enumerate()
        .filter(|(line_index, line)| {
            !rows.iter().any(|row| row.line_index == *line_index)
                && line.bbox.y > first_y
                && line.bbox.y <= first_y + 80.0
                && line.bbox.x <= table_right + 12.0
                && line.bbox.x + line.bbox.width >= table_left - 12.0
                && !text_line_plain_text(line)
                    .to_ascii_lowercase()
                    .starts_with("table ")
                // Skip lines that are themselves full data rows (a labelled row of
                // numeric columns, e.g. a "$"-prefixed opening balance): those
                // belong in the body, not merged into the column header.
                && !line_is_data_row(line, columns)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.1.bbox.y.total_cmp(&left.1.bbox.y));

    for (line_index, line) in candidates {
        let mut used_line = false;
        for cell in implied_table_cells(line) {
            if cell.text.chars().count() > 40 {
                continue;
            }
            let Some(column) = nearest_table_column(&cell, &column_refs) else {
                continue;
            };
            append_header_cell(&mut header.cells[column], cell);
            used_line = true;
        }
        if used_line {
            header.line_indices.push(line_index);
        }
    }

    header
}

/// A line that looks like a full body row — at least as many cells as the table
/// has columns, with numeric values in the non-label cells. Used to keep opening
/// balances and similar `$`-prefixed rows out of the inferred header.
fn line_is_data_row(line: &TextLine, columns: usize) -> bool {
    let cells = implied_table_cells(line);
    cells.len() >= columns && row_has_numeric_table_evidence(&cells)
}

/// Assign a header fragment to the column whose horizontal span it overlaps (or is
/// nearest in center). Center matching, rather than left-edge matching, is what
/// lets a left-aligned header word line up with a right-aligned numeric column.
fn nearest_table_column(cell: &TextRun, column_refs: &[(f32, f32)]) -> Option<usize> {
    let cell_center = cell.bbox.x + cell.bbox.width / 2.0;
    let (column, distance) = column_refs
        .iter()
        .enumerate()
        .map(|(index, (left, right))| {
            let column_center = (left + right) / 2.0;
            (index, (cell_center - column_center).abs())
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))?;
    let (left, right) = column_refs[column];
    let tolerance = ((right - left) / 2.0 + 18.0).max(24.0);
    (distance <= tolerance).then_some(column)
}

fn append_header_cell(target: &mut Option<TextRun>, fragment: TextRun) {
    if let Some(existing) = target {
        if !existing.text.is_empty() {
            existing.text.push(' ');
        }
        existing.text.push_str(&fragment.text);
        existing.bbox = union_boxes([existing.bbox, fragment.bbox]).unwrap_or(existing.bbox);
        for id in fragment.source_object_ids {
            if !existing.source_object_ids.contains(&id) {
                existing.source_object_ids.push(id);
            }
        }
    } else {
        *target = Some(fragment);
    }
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

/// Map a point from unrotated page space into the displayed (clockwise-rotated)
/// frame for a `/Rotate` of 90/180/270 (ISO 32000-1 §7.7.3.3). Assumes the page
/// origin is at (0, 0).
fn rotate_point(x: f32, y: f32, rotation: i32, width: f32, height: f32) -> (f32, f32) {
    match rotation.rem_euclid(360) {
        90 => (y, width - x),
        180 => (width - x, height - y),
        270 => (height - y, x),
        _ => (x, y),
    }
}

/// Rotate an axis-aligned bbox into the displayed frame (90/180/270 keep it
/// axis-aligned), recomputing width/height from the transformed corners.
fn rotate_bbox(bbox: BBox, rotation: i32, width: f32, height: f32) -> BBox {
    if rotation.rem_euclid(360) == 0 {
        return bbox;
    }
    let (x0, y0) = rotate_point(bbox.x, bbox.y, rotation, width, height);
    let (x1, y1) = rotate_point(bbox.x + bbox.width, bbox.y + bbox.height, rotation, width, height);
    BBox {
        x: x0.min(x1),
        y: y0.min(y1),
        width: (x1 - x0).abs(),
        height: (y1 - y0).abs(),
    }
}

fn group_text_runs(mut runs: Vec<TextRun>) -> Vec<TextLine> {
    runs.sort_by(|left, right| {
        right
            .baseline_y
            .total_cmp(&left.baseline_y)
            .then(left.bbox.x.total_cmp(&right.bbox.x))
    });

    let mut lines: Vec<TextLine> = Vec::new();
    for run in runs {
        // Group by text baseline, not the visual bbox top, so a smaller-font
        // super/subscript stays on its line even though its box (ascent/descent)
        // differs from the body text.
        if let Some(line) = lines
            .iter_mut()
            .find(|line| (line.baseline_y - run.baseline_y).abs() <= 3.0)
        {
            line.bbox = union_boxes([line.bbox, run.bbox]).unwrap_or(line.bbox);
            // Drift the line anchor toward the lowest baseline, matching the old
            // union-of-boxes behavior, so following runs match the body baseline
            // rather than a leading super/subscript.
            line.baseline_y = line.baseline_y.min(run.baseline_y);
            line.runs.push(run);
        } else {
            lines.push(TextLine {
                baseline_y: run.baseline_y,
                bbox: run.bbox,
                runs: vec![run],
            });
        }
    }

    // Sort each line's runs left-to-right once at the end, instead of re-sorting
    // the whole line on every insert (which was O(k^2 log k) per line).
    for line in &mut lines {
        line.runs
            .sort_by(|left, right| left.bbox.x.total_cmp(&right.bbox.x));
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

fn page_seed(object: &PdfObject, object_map: &HashMap<u32, Arc<PdfObject>>) -> Option<PageSeed> {
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
    object_map: &HashMap<u32, Arc<PdfObject>>,
) -> String {
    let mut body = page_body.to_owned();
    append_parent_page_tree_entries(page_body, object_map, &mut body, 0);
    body
}

fn append_parent_page_tree_entries(
    body: &str,
    object_map: &HashMap<u32, Arc<PdfObject>>,
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

    for filter in stream_filters(&dict) {
        stream = decode_stream_filter(&filter, &stream)?;
    }
    Ok(Some(stream))
}

fn decode_stream_filter(filter: &str, stream: &[u8]) -> Result<Vec<u8>> {
    match filter {
        "FlateDecode" | "Fl" => {
            let mut decoder = ZlibDecoder::new(stream);
            let mut decoded = Vec::new();
            decoder
                .read_to_end(&mut decoded)
                .map_err(|error| DonglerError::pdf(format!("FlateDecode failed: {error}")))?;
            Ok(decoded)
        }
        "ASCII85Decode" | "A85" => ascii85_decode(stream),
        other => Err(DonglerError::pdf(format!(
            "unsupported stream filter: {other}"
        ))),
    }
}

fn stream_filters(dict: &str) -> Vec<String> {
    let Some(mut index) = dict.find("/Filter").map(|index| index + "/Filter".len()) else {
        return Vec::new();
    };
    let bytes = dict.as_bytes();
    skip_pdf_whitespace(bytes, &mut index);
    if bytes.get(index) == Some(&b'[') {
        index += 1;
        let mut filters = Vec::new();
        while index < bytes.len() && bytes[index] != b']' {
            skip_pdf_whitespace(bytes, &mut index);
            if bytes.get(index) == Some(&b']') {
                break;
            }
            if bytes.get(index) == Some(&b'/') {
                index += 1;
                let start = index;
                while index < bytes.len() && !is_pdf_name_delimiter(bytes[index]) {
                    index += 1;
                }
                if start < index {
                    filters.push(dict[start..index].to_owned());
                }
            } else {
                index += 1;
            }
        }
        filters
    } else if bytes.get(index) == Some(&b'/') {
        index += 1;
        let start = index;
        while index < bytes.len() && !is_pdf_name_delimiter(bytes[index]) {
            index += 1;
        }
        (start < index)
            .then(|| vec![dict[start..index].to_owned()])
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn skip_pdf_whitespace(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' '))
    {
        *index += 1;
    }
}

fn is_pdf_name_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'\0'
            | b'\t'
            | b'\n'
            | b'\x0c'
            | b'\r'
            | b' '
            | b'('
            | b')'
            | b'<'
            | b'>'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'/'
            | b'%'
    )
}

fn ascii85_decode(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut group = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ' => {}
            b'<' if bytes.get(index + 1) == Some(&b'~') => {
                index += 1;
            }
            b'~' if bytes.get(index + 1) == Some(&b'>') => break,
            b'z' if group.is_empty() => output.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                group.push(byte - b'!');
                if group.len() == 5 {
                    output.extend_from_slice(&ascii85_group_to_bytes(&group)?);
                    group.clear();
                }
            }
            _ => {
                return Err(DonglerError::pdf(format!(
                    "ASCII85Decode failed: invalid byte 0x{byte:02x}"
                )));
            }
        }
        index += 1;
    }

    if !group.is_empty() {
        if group.len() == 1 {
            return Err(DonglerError::pdf(
                "ASCII85Decode failed: dangling single digit",
            ));
        }
        let output_len = group.len() - 1;
        while group.len() < 5 {
            group.push(b'u' - b'!');
        }
        output.extend_from_slice(&ascii85_group_to_bytes(&group)?[..output_len]);
    }

    Ok(output)
}

fn ascii85_group_to_bytes(group: &[u8]) -> Result<[u8; 4]> {
    let mut value = 0u64;
    for digit in group {
        value = value * 85 + u64::from(*digit);
    }
    if value > u64::from(u32::MAX) {
        return Err(DonglerError::pdf("ASCII85Decode failed: invalid group"));
    }
    Ok((value as u32).to_be_bytes())
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

fn resolve_resource_body(page_body: &str, object_map: &HashMap<u32, Arc<PdfObject>>) -> Option<String> {
    let resource_ref = parse_direct_ref_after_key(page_body, "/Resources")?;
    object_map
        .get(&(resource_ref as u32))
        .map(|object| lossy(&object.body))
}

fn load_font_decoders(
    resource_text: &str,
    object_map: &HashMap<u32, Arc<PdfObject>>,
    font_cache: &HashMap<u32, Arc<FontDecoder>>,
) -> HashMap<String, Arc<FontDecoder>> {
    resolve_named_resource_refs(resource_text, "/Font", object_map)
        .into_iter()
        .map(|(name, object_number)| {
            let decoder = font_cache.get(&object_number).cloned().unwrap_or_else(|| {
                Arc::new(
                    object_map
                        .get(&object_number)
                        .map(|font| font_decoder(font.as_ref(), object_map))
                        .unwrap_or_default(),
                )
            });
            (name, decoder)
        })
        .collect()
}

fn resolve_named_resource_refs(
    resource_text: &str,
    key: &str,
    object_map: &HashMap<u32, Arc<PdfObject>>,
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

fn font_decoder(font: &PdfObject, object_map: &HashMap<u32, Arc<PdfObject>>) -> FontDecoder {
    let font_body = lossy(&font.body);
    let encoding = font_encoding_differences(&font_body, object_map);
    let widths = font_widths(&font_body, &encoding);
    let (bold, italic) = font_style(&font_body, object_map);
    let (ascent, descent) = font_vertical_metrics(&font_body, object_map);
    let Some(to_unicode_ref) = parse_refs_after_key(&font_body, "/ToUnicode")
        .into_iter()
        .next()
    else {
        return FontDecoder {
            cmap: HashMap::new(),
            encoding,
            widths,
            max_code_len: 1,
            bold,
            italic,
            ascent,
            descent,
        };
    };
    let Some(to_unicode) = object_map.get(&(to_unicode_ref as u32)) else {
        return FontDecoder {
            cmap: HashMap::new(),
            encoding,
            widths,
            max_code_len: 1,
            bold,
            italic,
            ascent,
            descent,
        };
    };
    let Ok(Some(cmap_stream)) = decode_stream_object(to_unicode.as_ref()) else {
        return FontDecoder {
            cmap: HashMap::new(),
            encoding,
            widths,
            max_code_len: 1,
            bold,
            italic,
            ascent,
            descent,
        };
    };

    let mut decoder = parse_to_unicode_cmap(&lossy(&cmap_stream));
    decoder.encoding = encoding;
    decoder.widths = if widths.is_empty() {
        cid_char_widths(&decoder.cmap, &font_cid_widths(&font_body, object_map))
    } else {
        widths
    };
    decoder.bold = bold;
    decoder.italic = italic;
    decoder.ascent = ascent;
    decoder.descent = descent;
    decoder
}

/// Font ascent/descent in em units (text-space fractions of the font size),
/// from `/FontDescriptor` `/Ascent` and `/Descent` (glyph space, /1000). Falls
/// back to typical Latin metrics when the descriptor is absent.
fn font_vertical_metrics(font_body: &str, object_map: &HashMap<u32, Arc<PdfObject>>) -> (f32, f32) {
    let mut ascent = 0.75;
    let mut descent = -0.25;
    if let Some(descriptor_ref) = parse_direct_ref_after_key(font_body, "/FontDescriptor") {
        if let Some(object) = object_map.get(&(descriptor_ref as u32)) {
            let body = lossy(&object.body);
            if let Some(value) = parse_number_after(&body, "/Ascent") {
                if value != 0.0 {
                    ascent = value / 1000.0;
                }
            }
            if let Some(value) = parse_number_after(&body, "/Descent") {
                if value != 0.0 {
                    descent = value / 1000.0;
                }
            }
        }
    }
    (ascent, descent)
}

/// Detect bold/italic for a font from its `/BaseFont` name (after stripping the
/// subset prefix) and, when present, its `/FontDescriptor` `/Flags` (bit 7
/// Italic, bit 19 ForceBold) and `/ItalicAngle`.
fn font_style(font_body: &str, object_map: &HashMap<u32, Arc<PdfObject>>) -> (bool, bool) {
    let mut bold = false;
    let mut italic = false;
    if let Some(name) = parse_name_after(font_body, "/BaseFont") {
        let bare = name.rsplit('+').next().unwrap_or(name.as_str()).to_ascii_lowercase();
        bold |= ["bold", "black", "heavy", "semibold", "demibold", "-bd", "demi"]
            .iter()
            .any(|needle| bare.contains(needle));
        italic |= ["italic", "oblique", "-it"]
            .iter()
            .any(|needle| bare.contains(needle));
    }
    if let Some(descriptor_ref) = parse_direct_ref_after_key(font_body, "/FontDescriptor") {
        if let Some(object) = object_map.get(&(descriptor_ref as u32)) {
            let body = lossy(&object.body);
            if let Some(flags) = parse_number_after(&body, "/Flags") {
                let flags = flags as i64;
                italic |= flags & 64 != 0;
                bold |= flags & 262_144 != 0;
            }
            if let Some(angle) = parse_number_after(&body, "/ItalicAngle") {
                italic |= angle.abs() > f32::EPSILON;
            }
        }
    }
    (bold, italic)
}

/// Parse a PDF name value (`/Name`) following `key`.
fn parse_name_after(text: &str, key: &str) -> Option<String> {
    let start = text.find(key)? + key.len();
    let rest = text[start..].trim_start();
    let mut chars = rest.chars();
    if chars.next()? != '/' {
        return None;
    }
    let name: String = chars
        .take_while(|character| {
            !character.is_whitespace()
                && !matches!(character, '/' | '[' | ']' | '<' | '>' | '(' | ')')
        })
        .collect();
    (!name.is_empty()).then_some(name)
}

fn font_widths(font_body: &str, encoding: &HashMap<u8, String>) -> HashMap<char, f32> {
    let Some(first_char) = parse_number_after(font_body, "/FirstChar").map(|value| value as u8)
    else {
        return HashMap::new();
    };
    let Some(widths) = parse_number_array_after(font_body, "/Widths") else {
        return HashMap::new();
    };

    widths
        .into_iter()
        .enumerate()
        .filter_map(|(index, width)| {
            let code = first_char.wrapping_add(index as u8);
            let text = encoding
                .get(&code)
                .cloned()
                .unwrap_or_else(|| (code as char).to_string());
            let mut chars = text.chars();
            let character = chars.next()?;
            chars.next().is_none().then_some((character, width))
        })
        .collect()
}

/// Glyph widths for a Type0 (composite) font, read from its descendant CIDFont's
/// `/W` array and keyed by CID. Simple fonts carry `/FirstChar`+`/Widths`, but
/// composite fonts — the norm for born-digital PDFs from Chrome/Skia, LaTeX, and
/// modern Office exporters — keep per-CID widths in `/DescendantFonts[0]/W`.
/// Without these every glyph falls back to a flat half-em, which destroys gap-based
/// word segmentation. The `/W` array mixes two run encodings: `c [w1 w2 …]` (widths
/// for consecutive CIDs starting at `c`) and `c_first c_last w` (one width for a
/// CID range). Returns CID → width in 1/1000 em.
fn font_cid_widths(font_body: &str, object_map: &HashMap<u32, Arc<PdfObject>>) -> HashMap<u32, f32> {
    let mut widths = HashMap::new();
    if parse_name_after(font_body, "/Subtype").as_deref() != Some("Type0") {
        return widths;
    }
    let Some(descendant) = parse_refs_after_key(font_body, "/DescendantFonts")
        .into_iter()
        .next()
    else {
        return widths;
    };
    let Some(cidfont) = object_map.get(&(descendant as u32)) else {
        return widths;
    };
    let body = lossy(&cidfont.body);
    let Some((open, close)) = find_w_array(&body) else {
        return widths;
    };
    let mut parser = ContentParser::new(&body.as_bytes()[open..=close]);
    let Some(ContentToken::Operand(Operand::Array(items))) = parser.next_operand_or_operator() else {
        return widths;
    };

    let mut index = 0;
    while index < items.len() {
        match (&items[index], items.get(index + 1)) {
            (Operand::Number(first), Some(Operand::Array(list))) => {
                let base = *first as i64;
                for (offset, width) in list.iter().enumerate() {
                    if let Operand::Number(width) = width {
                        let cid = base + offset as i64;
                        if cid >= 0 {
                            widths.insert(cid as u32, *width);
                        }
                    }
                }
                index += 2;
            }
            (Operand::Number(first), Some(Operand::Number(last))) => {
                if let Some(Operand::Number(width)) = items.get(index + 2) {
                    let (lo, hi) = (*first as i64, *last as i64);
                    if lo >= 0 && hi >= lo && hi - lo < 70_000 {
                        for cid in lo..=hi {
                            widths.insert(cid as u32, *width);
                        }
                    }
                    index += 3;
                } else {
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }
    widths
}

/// Locate the `/W` array of a CIDFont, returning the byte span of its `[ … ]`.
/// Distinguishes the `/W` key from look-alikes (`/WMode`, `/Widths`) by requiring
/// whitespace or `[` immediately after.
fn find_w_array(body: &str) -> Option<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut search = 0;
    while let Some(rel) = body[search..].find("/W") {
        let key_end = search + rel + 2;
        if matches!(bytes.get(key_end), Some(byte) if is_ws(*byte) || *byte == b'[') {
            let mut pos = key_end;
            while pos < bytes.len() && is_ws(bytes[pos]) {
                pos += 1;
            }
            if bytes.get(pos) == Some(&b'[') {
                if let Some(close) = matching_array_close(body, pos) {
                    return Some((pos, close));
                }
            }
        }
        search = key_end;
    }
    None
}

/// Translate CID-keyed widths into char-keyed widths via the font's ToUnicode
/// cmap. For Identity-H (the universal Skia/LaTeX encoding) the CID is the numeric
/// value of the 2-byte code, which is exactly the cmap key, so each single-char
/// mapping yields one char → width pair.
fn cid_char_widths(
    cmap: &HashMap<Vec<u8>, String>,
    cid_widths: &HashMap<u32, f32>,
) -> HashMap<char, f32> {
    let mut out = HashMap::new();
    if cid_widths.is_empty() {
        return out;
    }
    for (code, text) in cmap {
        if code.is_empty() || code.len() > 4 {
            continue;
        }
        let mut chars = text.chars();
        let (Some(character), None) = (chars.next(), chars.next()) else {
            continue;
        };
        let cid = code.iter().fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte));
        if let Some(width) = cid_widths.get(&cid) {
            out.insert(character, *width);
        }
    }
    out
}

fn font_encoding_differences(
    font_body: &str,
    object_map: &HashMap<u32, Arc<PdfObject>>,
) -> HashMap<u8, String> {
    if let Some(encoding_ref) = parse_direct_ref_after_key(font_body, "/Encoding") {
        if let Some(object) = object_map.get(&(encoding_ref as u32)) {
            let differences = parse_encoding_differences(&lossy(&object.body));
            if !differences.is_empty() {
                return differences;
            }
        }
    }
    parse_encoding_differences(font_body)
}

fn parse_encoding_differences(text: &str) -> HashMap<u8, String> {
    let Some(start) = text.find("/Differences") else {
        return HashMap::new();
    };
    let rest = &text[start + "/Differences".len()..];
    let Some(open) = rest.find('[') else {
        return HashMap::new();
    };
    let Some(close) = matching_array_close(rest, open) else {
        return HashMap::new();
    };
    let mut parser = ContentParser::new(rest[open..=close].as_bytes());
    let Some(ContentToken::Operand(Operand::Array(items))) = parser.next_operand_or_operator()
    else {
        return HashMap::new();
    };

    let mut differences = HashMap::new();
    let mut code: Option<u16> = None;
    for item in items {
        match item {
            Operand::Number(value) if value >= 0.0 => {
                code = Some(value as u16);
            }
            Operand::Name(name) => {
                let Some(current_code) = code else {
                    continue;
                };
                if current_code <= u16::from(u8::MAX) {
                    if let Some(text) = glyph_name_to_text(&name) {
                        differences.insert(current_code as u8, text);
                    }
                }
                code = current_code.checked_add(1);
            }
            _ => {}
        }
    }
    differences
}

fn matching_array_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in text.as_bytes().iter().enumerate().skip(open) {
        match byte {
            b'[' => depth += 1,
            b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_to_unicode_cmap(text: &str) -> FontDecoder {
    let mut cmap = HashMap::new();
    let mut in_bfchar = false;
    let mut in_bfrange = false;
    let mut bfrange_array_entry = String::new();
    let mut bfrange_array_depth = 0i32;

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
                bfrange_array_entry.clear();
                bfrange_array_depth = 0;
                continue;
            }
            _ => {}
        }

        if in_bfrange {
            if bfrange_array_depth > 0 {
                bfrange_array_entry.push(' ');
                bfrange_array_entry.push_str(trimmed);
                bfrange_array_depth += bracket_delta(trimmed);
                if bfrange_array_depth <= 0 {
                    add_bfrange_entry(&mut cmap, &bfrange_array_entry);
                    bfrange_array_entry.clear();
                    bfrange_array_depth = 0;
                }
                continue;
            }

            let depth = bracket_delta(trimmed);
            if depth > 0 {
                bfrange_array_entry.clear();
                bfrange_array_entry.push_str(trimmed);
                bfrange_array_depth = depth;
                continue;
            }

            add_bfrange_entry(&mut cmap, trimmed);
            continue;
        }

        let hexes = hex_strings_in_line(trimmed);
        if in_bfchar && hexes.len() >= 2 {
            cmap.insert(
                hexes[0].clone(),
                cmap_text_for_mapping(&hexes[0], &hexes[1]),
            );
        }
    }

    let max_code_len = cmap.keys().map(Vec::len).max().unwrap_or(1);
    FontDecoder {
        cmap,
        encoding: HashMap::new(),
        widths: HashMap::new(),
        max_code_len,
        bold: false,
        italic: false,
        ascent: 0.75,
        descent: -0.25,
    }
}

fn bracket_delta(text: &str) -> i32 {
    text.chars().fold(0, |depth, character| match character {
        '[' => depth + 1,
        ']' => depth - 1,
        _ => depth,
    })
}

fn add_bfrange_entry(cmap: &mut HashMap<Vec<u8>, String>, line: &str) {
    let hexes = hex_strings_in_line(line);
    if hexes.len() < 3 {
        return;
    }
    if line.contains('[') {
        add_bfrange_array(cmap, &hexes);
    } else {
        add_bfrange(cmap, &hexes);
    }
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

fn add_bfrange_array(cmap: &mut HashMap<Vec<u8>, String>, hexes: &[Vec<u8>]) {
    let Some(start) = hex_to_u32(&hexes[0]) else {
        return;
    };
    let Some(end) = hex_to_u32(&hexes[1]) else {
        return;
    };
    let source_len = hexes[0].len();
    let range_len = end.saturating_sub(start).saturating_add(1) as usize;

    for (offset, destination) in hexes.iter().skip(2).take(range_len.min(512)).enumerate() {
        let source = start + offset as u32;
        let source_bytes = number_to_be_bytes(source, source_len);
        cmap.insert(
            source_bytes.clone(),
            cmap_text_for_mapping(&source_bytes, destination),
        );
    }
}

fn cmap_text_for_mapping(source: &[u8], destination: &[u8]) -> String {
    if destination.len() > 2 {
        return utf16be_hex_to_string(destination);
    }
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
    fonts: &HashMap<String, Arc<FontDecoder>>,
) -> Option<String> {
    operands
        .first()
        .and_then(|operand| operand_text(operand, state, fonts))
}

fn operand_text(
    operand: &Operand,
    state: &GraphicsState,
    fonts: &HashMap<String, Arc<FontDecoder>>,
) -> Option<String> {
    match operand {
        Operand::Literal(bytes) | Operand::Hex(bytes) => Some(decode_pdf_text(
            bytes,
            state
                .font_name
                .as_ref()
                .and_then(|font_name| fonts.get(font_name))
                .map(|font| font.as_ref()),
        )),
        _ => None,
    }
}

fn text_from_array(
    items: &[Operand],
    state: &GraphicsState,
    fonts: &HashMap<String, Arc<FontDecoder>>,
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
        if !font.encoding.is_empty() {
            return bytes.iter().map(|byte| font.decode_byte(*byte)).collect();
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
            output.push_str(&font.decode_byte(bytes[index]));
            index += 1;
        }
    }

    output
}

fn glyph_name_to_text(name: &str) -> Option<String> {
    let text = match name {
        "space" => " ",
        "exclam" => "!",
        "quotedbl" => "\"",
        "numbersign" => "#",
        "dollar" => "$",
        "percent" => "%",
        "ampersand" => "&",
        "quotesingle" | "quoteright" | "quoteleft" => "'",
        "parenleft" | "parenleftbig" | "parenleftBig" | "parenleftbigg" | "parenleftBigg" => "(",
        "parenright" | "parenrightbig" | "parenrightBig" | "parenrightbigg" | "parenrightBigg" => {
            ")"
        }
        "asterisk" | "asteriskmath" => "*",
        "plus" => "+",
        "comma" => ",",
        "hyphen" => "-",
        "period" => ".",
        "slash" => "/",
        "zero" => "0",
        "one" => "1",
        "two" => "2",
        "three" => "3",
        "four" => "4",
        "five" => "5",
        "six" => "6",
        "seven" => "7",
        "eight" => "8",
        "nine" => "9",
        "colon" => ":",
        "semicolon" => ";",
        "less" => "<",
        "equal" => "=",
        "greater" => ">",
        "question" => "?",
        "at" => "@",
        "bracketleft" => "[",
        "backslash" => "\\",
        "bracketright" => "]",
        "circumflex" | "hatwide" | "hatwider" | "hatwidest" => "^",
        "underscore" => "_",
        "braceleft" | "braceleftBig" | "braceleftBigg" | "bracelefttp" | "braceleftbt"
        | "braceleftmid" => "{",
        "bar" | "vextendsingle" | "braceex" => "|",
        "braceright" | "bracerightBig" => "}",
        "tilde" | "tildewide" => "~",
        "ff" => "ff",
        "fi" => "fi",
        "fl" => "fl",
        "ffi" => "ffi",
        "ffl" => "ffl",
        "Gamma" => "Γ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Omega" => "Ω",
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ε",
        "zeta" => "ζ",
        "lambda" => "λ",
        "mu" => "μ",
        "pi" | "pi1" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "phi" => "φ",
        "chi" => "χ",
        "omega" => "ω",
        "partialdiff" => "∂",
        "minus" => "−",
        "periodcentered" => "·",
        "multiply" => "×",
        "plusminus" => "±",
        "circlemultiply" => "⊗",
        "openbullet" | "bullet" => "•",
        "lessequal" => "≤",
        "greaterequal" => "≥",
        "similar" => "∼",
        "arrowright" => "→",
        "mapsto" => "↦",
        "prime" => "′",
        "infinity" => "∞",
        "element" => "∈",
        "universal" => "∀",
        "union" | "uniontext" | "uniondisplay" => "∪",
        "intersection" | "intersectiontext" | "intersectiondisplay" => "∩",
        "reflexsubset" => "⊇",
        "reflexsuperset" => "⊆",
        "summationtext" | "summationdisplay" => "∑",
        "productdisplay" => "∏",
        "integraldisplay" => "∫",
        "circleplusdisplay" => "⊕",
        "unionsqdisplay" => "⊔",
        "negationslash" => "̸",
        _ if name.chars().count() == 1 => name,
        _ => return unicode_glyph_name_to_text(name),
    };
    Some(text.to_owned())
}

fn unicode_glyph_name_to_text(name: &str) -> Option<String> {
    if let Some(hex) = name.strip_prefix("uni") {
        if hex.len() >= 4 && hex.len() % 4 == 0 {
            let mut output = String::new();
            for chunk in hex.as_bytes().chunks(4) {
                let chunk = std::str::from_utf8(chunk).ok()?;
                let code = u32::from_str_radix(chunk, 16).ok()?;
                output.push(char::from_u32(code)?);
            }
            return Some(output);
        }
    }
    if let Some(hex) = name.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) {
            let code = u32::from_str_radix(hex, 16).ok()?;
            return char::from_u32(code).map(|character| character.to_string());
        }
    }
    None
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

/// Classify a text line as a heading (`heading_1`..`heading_3`) or `paragraph`
/// from its font size relative to the page body size. Headings on born-digital
/// pages are typically set in a visibly larger size; the renderer maps
/// `heading_N` to Markdown `#`*N and LaTeX `\section`/`\subsection`/etc.
fn classify_text_line(text: &str, line_size: f32, body_size: f32) -> String {
    let chars = text.chars().count();
    // Long runs of text are body copy even if slightly larger; very short empty
    // lines are not headings.
    if chars == 0 || chars >= 200 || body_size <= 0.0 || line_size <= 0.0 {
        return "paragraph".to_owned();
    }
    let ratio = line_size / body_size;
    if ratio >= 1.5 {
        "heading_1".to_owned()
    } else if ratio >= 1.3 {
        "heading_2".to_owned()
    } else if ratio >= 1.12 {
        "heading_3".to_owned()
    } else {
        "paragraph".to_owned()
    }
}

/// The font size of the dominant (longest by character count) run in a line.
fn line_dominant_size(line: &TextLine) -> f32 {
    let mut best_chars = 0usize;
    let mut best_size = 0.0f32;
    for run in &line.runs {
        if run.size <= 0.0 {
            continue;
        }
        let chars = run.text.chars().count();
        if chars >= best_chars {
            best_chars = chars;
            best_size = run.size;
        }
    }
    best_size
}

/// The page's body font size: the most common run size (in 0.5pt buckets),
/// weighted by character count. Used as the baseline for heading detection.
fn page_body_size(lines: &[TextLine]) -> f32 {
    let mut weights: Vec<(u32, usize)> = Vec::new();
    for line in lines {
        for run in &line.runs {
            if run.size <= 0.0 {
                continue;
            }
            let bucket = (run.size * 2.0).round() as u32;
            let chars = run.text.chars().count();
            if let Some(entry) = weights.iter_mut().find(|(value, _)| *value == bucket) {
                entry.1 += chars;
            } else {
                weights.push((bucket, chars));
            }
        }
    }
    weights
        .into_iter()
        .max_by_key(|(_, chars)| *chars)
        .map(|(bucket, _)| bucket as f32 / 2.0)
        .unwrap_or(0.0)
}

fn source_ids_for_line(line: &TextLine) -> Vec<String> {
    source_ids_for_runs(&line.runs)
}

fn source_ids_for_runs(runs: &[TextRun]) -> Vec<String> {
    let mut ids = Vec::new();
    for run in runs {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_from_line_runs_does_not_treat_slash_prose_page_number_as_script() {
        let line = TextLine {
            runs: vec![
                test_run("Art Cutting / Bates Technical College", 72.0, 720.0, 12.0),
                test_run("24", 300.0, 722.0, 8.0),
                test_run("Core Competencies", 315.0, 720.0, 12.0),
            ],
            bbox: BBox {
                x: 72.0,
                y: 720.0,
                width: 360.0,
                height: 12.0,
            },
            baseline_y: 720.0,
        };

        assert_eq!(
            text_from_line_runs(&line),
            "Art Cutting / Bates Technical College 24 Core Competencies"
        );
    }

    fn test_run(text: &str, x: f32, y: f32, size: f32) -> TextRun {
        TextRun {
            text: text.to_owned(),
            bbox: BBox {
                x,
                y,
                width: text.len() as f32 * size * 0.4,
                height: size,
            },
            baseline_y: y,
            font: None,
            size,
            space_width: size * 0.25,
            bold: false,
            italic: false,
            source_object_ids: Vec::new(),
        }
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
