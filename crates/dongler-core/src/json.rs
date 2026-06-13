use std::cmp::Ordering;
use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::engine::ExtractionEngine;
use crate::error::Result;
use crate::ir::{
    BBox, Block, Confidence, Document, FigureBlock, Metadata, Page, SourceAnchor, TableBlock,
    TableCell, TextBlock, SCHEMA_VERSION,
};
use crate::source::Source;
use crate::textual::html_to_text;

const EXTRACTION_METHOD: &str = "json_native";

#[derive(Debug, Default, Clone, Copy)]
pub struct JsonEngine;

impl ExtractionEngine for JsonEngine {
    fn name(&self) -> &'static str {
        "json-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        let pages = parse_json_pages(&source.content)?;
        Ok(build_document(source, self.name(), pages))
    }
}

#[derive(Debug)]
struct TextRecord {
    kind: String,
    text: String,
}

fn parse_json_pages(content: &str) -> Result<Vec<Page>> {
    match serde_json::from_str::<Value>(content) {
        Ok(value) => Ok(pages_from_json_value(&value)),
        Err(json_error) => {
            let mut pages = Vec::new();
            for (index, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let value = serde_json::from_str::<Value>(trimmed)?;
                let mut value_pages = pages_from_json_value(&value);
                renumber_pages(&mut value_pages, index + 1);
                pages.extend(value_pages);
            }
            if pages.is_empty() {
                Err(json_error.into())
            } else {
                Ok(pages)
            }
        }
    }
}

fn pages_from_json_value(value: &Value) -> Vec<Page> {
    if let Some(pages) = omnidocbench_pages(value) {
        return pages;
    }
    if let Some(page) = funsd_page(value) {
        return vec![page];
    }
    if let Some(pages) = coco_pages(value) {
        return pages;
    }
    if let Some(page) = pubtabnet_page(value, 1) {
        return vec![page];
    }
    if let Some(page) = word_boxes_page(value, 1) {
        return vec![page];
    }
    if let Some(page) = grid_cells_page(value, 1) {
        return vec![page];
    }

    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| generic_page_from_value(item, index + 1))
            .collect(),
        Value::Object(object) => {
            if let Some(Value::Array(pages)) = object.get("pages") {
                return pages
                    .iter()
                    .enumerate()
                    .map(|(index, item)| generic_page_from_value(item, index + 1))
                    .collect();
            }
            vec![generic_page_from_value(value, 1)]
        }
        _ => vec![generic_page_from_value(value, 1)],
    }
}

fn grid_cells_page(value: &Value, page_number: usize) -> Option<Page> {
    let object = value.as_object()?;
    let cell_rows = object.get("cells")?.as_array()?;
    let mut rows = Vec::new();
    let mut table_cells = Vec::new();

    for (row_index, row) in cell_rows.iter().enumerate() {
        let Some(row_cells) = row.as_array() else {
            continue;
        };
        let mut text_row = Vec::new();
        for (column_index, cell) in row_cells.iter().enumerate() {
            let text = pubtabnet_cell_text(Some(cell));
            text_row.push(text.clone());
            table_cells.push(TableCell {
                row: row_index,
                column: column_index,
                text,
                bbox: cell.get("bbox").and_then(bbox_from_rect),
                is_header: row_index == 0,
                col_span: 1,
                row_span: 1,
            });
        }
        if !text_row.is_empty() {
            rows.push(text_row);
        }
    }

    if rows.is_empty() {
        return None;
    }
    let bbox = object
        .get("table_bbox")
        .and_then(bbox_from_rect)
        .or_else(|| inferred_table_cell_bbox(&table_cells));
    let (headers, rows) = split_table_rows(rows);

    Some(Page {
        number: page_number,
        width: bbox.map(|bbox| bbox.x + bbox.width),
        height: bbox.map(|bbox| bbox.y + bbox.height),
        rotation: None,
        route: None,
        bbox: bbox.map(|bbox| BBox {
            x: 0.0,
            y: 0.0,
            width: bbox.x + bbox.width,
            height: bbox.y + bbox.height,
        }),
        blocks: vec![Block::Table(TableBlock {
            headers,
            rows,
            caption: None,
            bbox,
            cells: table_cells,
            source_anchors: vec![source_anchor(page_number, bbox)],
            confidence: Some(confidence()), ..Default::default()
        })],
        images: Vec::new(),
        assets: Vec::new(),
        warnings: Vec::new(),
    })
}

fn inferred_table_cell_bbox(cells: &[TableCell]) -> Option<BBox> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut has_bbox = false;
    for cell in cells {
        let Some(bbox) = cell.bbox else {
            continue;
        };
        has_bbox = true;
        min_x = min_x.min(bbox.x);
        min_y = min_y.min(bbox.y);
        max_x = max_x.max(bbox.x + bbox.width);
        max_y = max_y.max(bbox.y + bbox.height);
    }
    has_bbox.then_some(BBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn renumber_pages(pages: &mut [Page], first_page_number: usize) {
    for (offset, page) in pages.iter_mut().enumerate() {
        let page_number = first_page_number + offset;
        page.number = page_number;
        for block in &mut page.blocks {
            match block {
                Block::Text(text) => {
                    for anchor in &mut text.source_anchors {
                        anchor.page_number = page_number;
                    }
                }
                Block::Table(table) => {
                    for anchor in &mut table.source_anchors {
                        anchor.page_number = page_number;
                    }
                }
                Block::Figure(figure) => {
                    for anchor in &mut figure.source_anchors {
                        anchor.page_number = page_number;
                    }
                }
            }
        }
    }
}

fn pubtabnet_page(value: &Value, page_number: usize) -> Option<Page> {
    let object = value.as_object()?;
    let html = object.get("html")?.as_object()?;
    let structure = html
        .get("structure")
        .and_then(Value::as_object)?
        .get("tokens")
        .and_then(Value::as_array)?;
    let cells = html.get("cells").or_else(|| html.get("cell"))?.as_array()?;
    let rows = pubtabnet_rows(structure, cells);
    if rows.is_empty() {
        return None;
    }
    let table_cells = pubtabnet_table_cells(&rows);
    let bbox = inferred_table_cell_bbox(&table_cells);
    let (headers, rows) = split_table_rows(
        rows.iter()
            .map(|row| row.cells.iter().map(|cell| cell.text.clone()).collect())
            .collect(),
    );

    Some(Page {
        number: page_number,
        width: bbox.map(|bbox| bbox.x + bbox.width),
        height: bbox.map(|bbox| bbox.y + bbox.height),
        rotation: None,
        route: None,
        bbox: bbox.map(|bbox| BBox {
            x: 0.0,
            y: 0.0,
            width: bbox.x + bbox.width,
            height: bbox.y + bbox.height,
        }),
        blocks: vec![Block::Table(TableBlock {
            headers,
            rows,
            caption: None,
            bbox,
            cells: table_cells,
            source_anchors: vec![source_anchor(page_number, bbox)],
            confidence: Some(confidence()), ..Default::default()
        })],
        images: Vec::new(),
        assets: Vec::new(),
        warnings: Vec::new(),
    })
}

#[derive(Debug)]
struct PubTabNetRow {
    cells: Vec<PubTabNetCell>,
}

#[derive(Debug)]
struct PubTabNetCell {
    text: String,
    bbox: Option<BBox>,
}

fn pubtabnet_rows(structure: &[Value], cells: &[Value]) -> Vec<PubTabNetRow> {
    let mut rows = Vec::new();
    let mut current_row: Option<PubTabNetRow> = None;
    let mut cell_index = 0usize;

    for token in structure.iter().filter_map(Value::as_str) {
        let normalized = token.trim().to_ascii_lowercase();
        if normalized.starts_with("<tr") && !normalized.starts_with("</") {
            current_row = Some(PubTabNetRow { cells: Vec::new() });
        } else if normalized.starts_with("</tr") {
            if let Some(row) = current_row.take() {
                if !row.cells.is_empty() {
                    rows.push(row);
                }
            }
        } else if is_pubtabnet_cell_open(&normalized) {
            let Some(row) = current_row.as_mut() else {
                continue;
            };
            row.cells.push(pubtabnet_cell(cells.get(cell_index)));
            cell_index += 1;
        }
    }

    rows
}

fn pubtabnet_table_cells(rows: &[PubTabNetRow]) -> Vec<TableCell> {
    rows.iter()
        .enumerate()
        .flat_map(|(row_index, row)| {
            row.cells
                .iter()
                .enumerate()
                .map(move |(column_index, cell)| TableCell {
                    row: row_index,
                    column: column_index,
                    text: cell.text.clone(),
                    bbox: cell.bbox,
                    is_header: row_index == 0,
                    col_span: 1,
                    row_span: 1,
                })
        })
        .collect()
}

fn is_pubtabnet_cell_open(token: &str) -> bool {
    (token.starts_with("<td") || token.starts_with("<th")) && !token.starts_with("</")
}

fn pubtabnet_cell_text(cell: Option<&Value>) -> String {
    let Some(cell) = cell.and_then(Value::as_object) else {
        return String::new();
    };
    let text = cell
        .get("tokens")
        .and_then(Value::as_array)
        .map(|tokens| {
            tokens
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("")
        })
        .or_else(|| cell.get("text").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default();
    clean_text(&html_to_text(&text))
}

fn pubtabnet_cell(cell: Option<&Value>) -> PubTabNetCell {
    PubTabNetCell {
        text: pubtabnet_cell_text(cell),
        bbox: cell
            .and_then(Value::as_object)
            .and_then(|cell| cell.get("bbox"))
            .and_then(bbox_from_rect),
    }
}

fn word_boxes_page(value: &Value, page_number: usize) -> Option<Page> {
    let object = value.as_object()?;
    let words = object.get("words")?.as_array()?;
    let mut blocks = words
        .iter()
        .filter_map(|word| word.as_object())
        .filter_map(|word| word_box_block(word, page_number))
        .collect::<Vec<_>>();
    if blocks.is_empty() {
        return None;
    }
    blocks.sort_by(|left, right| {
        let left_bbox = block_bbox(left);
        let right_bbox = block_bbox(right);
        match (left_bbox, right_bbox) {
            (Some(left), Some(right)) => left
                .y
                .partial_cmp(&right.y)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.x.partial_cmp(&right.x).unwrap_or(Ordering::Equal)),
            _ => Ordering::Equal,
        }
    });

    let width = first_numeric_field(object, &["image_width", "page_width", "width"]);
    let height = first_numeric_field(object, &["image_height", "page_height", "height"]);
    let bbox = page_bbox(width, height).or_else(|| inferred_page_bbox(&blocks));

    Some(Page {
        number: page_number,
        width: width.or_else(|| bbox.map(|bbox| bbox.width)),
        height: height.or_else(|| bbox.map(|bbox| bbox.height)),
        rotation: None,
        bbox,
        blocks,
        images: Vec::new(),
        assets: Vec::new(),
        warnings: Vec::new(), ..Default::default()
    })
}

fn word_box_block(word: &Map<String, Value>, page_number: usize) -> Option<Block> {
    let text = first_string_field(word, &["text", "word", "value"]).map(clean_text)?;
    if text.is_empty() {
        return None;
    }
    let bbox = first_bbox_field(
        word,
        &[
            "bbox",
            "box",
            "image_bbox",
            "pdf_bbox",
            "rect",
            "bounds",
            "bounding_box",
        ],
    );

    Some(Block::Text(TextBlock {
        text,
        kind: "word".to_owned(),
        bbox,
        lines: Vec::new(),
        source_anchors: vec![source_anchor(page_number, bbox)],
        confidence: Some(confidence()), ..Default::default()
    }))
}

fn coco_pages(value: &Value) -> Option<Vec<Page>> {
    let object = value.as_object()?;
    let images = object.get("images")?.as_array()?;
    let annotations = object.get("annotations")?.as_array()?;
    let categories = coco_categories(object.get("categories").and_then(Value::as_array));
    if images.is_empty() {
        return None;
    }

    let mut annotations_by_image: HashMap<String, Vec<&Map<String, Value>>> = HashMap::new();
    for annotation in annotations.iter().filter_map(Value::as_object) {
        let Some(image_id) = annotation.get("image_id").map(value_key) else {
            continue;
        };
        annotations_by_image
            .entry(image_id)
            .or_default()
            .push(annotation);
    }

    let mut pages = Vec::new();
    for (index, image) in images.iter().filter_map(Value::as_object).enumerate() {
        let Some(image_id) = image.get("id").map(value_key) else {
            continue;
        };
        let width = numeric_field(image, "width");
        let height = numeric_field(image, "height");
        let page_number = index + 1;
        let mut page_annotations = annotations_by_image.remove(&image_id).unwrap_or_default();
        page_annotations.sort_by(|left, right| {
            let left_bbox = left.get("bbox").and_then(bbox_from_coco_rect);
            let right_bbox = right.get("bbox").and_then(bbox_from_coco_rect);
            match (left_bbox, right_bbox) {
                (Some(left), Some(right)) => left
                    .y
                    .partial_cmp(&right.y)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| left.x.partial_cmp(&right.x).unwrap_or(Ordering::Equal)),
                _ => Ordering::Equal,
            }
        });

        let blocks = page_annotations
            .into_iter()
            .filter_map(|annotation| coco_block(annotation, &categories, page_number))
            .collect::<Vec<_>>();
        pages.push(Page {
            number: page_number,
            width,
            height,
            rotation: None,
            bbox: page_bbox(width, height),
            blocks,
            images: Vec::new(),
            assets: Vec::new(),
            warnings: Vec::new(), ..Default::default()
        });
    }

    (!pages.is_empty()).then_some(pages)
}

fn coco_categories(categories: Option<&Vec<Value>>) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for category in categories
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
    {
        let Some(id) = category.get("id").map(value_key) else {
            continue;
        };
        let name = category
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("layout")
            .to_owned();
        names.insert(id, name);
    }
    names
}

fn coco_block(
    annotation: &Map<String, Value>,
    categories: &HashMap<String, String>,
    page_number: usize,
) -> Option<Block> {
    let bbox = annotation.get("bbox").and_then(bbox_from_coco_rect)?;
    let category_id = annotation.get("category_id").map(value_key);
    let kind = category_id
        .as_ref()
        .and_then(|id| categories.get(id))
        .cloned()
        .unwrap_or_else(|| "layout".to_owned());

    Some(Block::Text(TextBlock {
        text: kind.clone(),
        kind,
        bbox: Some(bbox),
        lines: Vec::new(),
        source_anchors: vec![source_anchor(page_number, Some(bbox))],
        confidence: Some(confidence()), ..Default::default()
    }))
}

fn funsd_page(value: &Value) -> Option<Page> {
    let form = value.as_object()?.get("form")?.as_array()?;
    let mut fields = form.iter().filter_map(Value::as_object).collect::<Vec<_>>();
    if fields.is_empty() {
        return None;
    }
    fields.sort_by(|left, right| {
        let left_bbox = left.get("box").and_then(bbox_from_rect);
        let right_bbox = right.get("box").and_then(bbox_from_rect);
        match (left_bbox, right_bbox) {
            (Some(left), Some(right)) => left
                .y
                .partial_cmp(&right.y)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.x.partial_cmp(&right.x).unwrap_or(Ordering::Equal)),
            _ => Ordering::Equal,
        }
    });

    let blocks = fields
        .into_iter()
        .filter_map(funsd_block)
        .collect::<Vec<_>>();
    if blocks.is_empty() {
        return None;
    }
    let bbox = inferred_page_bbox(&blocks);

    Some(Page {
        number: 1,
        width: bbox.map(|bbox| bbox.width),
        height: bbox.map(|bbox| bbox.height),
        rotation: None,
        bbox,
        blocks,
        images: Vec::new(),
        assets: Vec::new(),
        warnings: Vec::new(), ..Default::default()
    })
}

fn funsd_block(field: &Map<String, Value>) -> Option<Block> {
    let text = field.get("text").and_then(Value::as_str).map(clean_text)?;
    if text.is_empty() {
        return None;
    }
    let bbox = field.get("box").and_then(bbox_from_rect);
    let kind = field
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or("field")
        .to_owned();

    Some(Block::Text(TextBlock {
        text,
        kind,
        bbox,
        lines: Vec::new(),
        source_anchors: vec![source_anchor(1, bbox)],
        confidence: Some(confidence()), ..Default::default()
    }))
}

fn omnidocbench_pages(value: &Value) -> Option<Vec<Page>> {
    let items = match value {
        Value::Array(items) => items.as_slice(),
        Value::Object(object) => object.get("pages")?.as_array()?.as_slice(),
        _ => return None,
    };
    if items
        .iter()
        .all(|item| item.get("layout_dets").and_then(Value::as_array).is_none())
    {
        return None;
    }

    let mut pages = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(layout_dets) = object.get("layout_dets").and_then(Value::as_array) else {
            continue;
        };
        let page_info = object.get("page_info").and_then(Value::as_object);
        let width = page_info.and_then(|info| numeric_field(info, "width"));
        let height = page_info.and_then(|info| numeric_field(info, "height"));
        let page_number = index + 1;
        let mut detections = layout_dets
            .iter()
            .filter_map(Value::as_object)
            .collect::<Vec<_>>();
        detections.sort_by(|left, right| {
            order_value(left)
                .partial_cmp(&order_value(right))
                .unwrap_or(Ordering::Equal)
        });

        let blocks = detections
            .into_iter()
            .filter(|detection| !bool_field(detection, "ignore"))
            .filter_map(|detection| block_from_layout_detection(detection, page_number))
            .collect::<Vec<_>>();

        pages.push(Page {
            number: page_number,
            width,
            height,
            rotation: None,
            bbox: page_bbox(width, height),
            blocks,
            images: Vec::new(),
            assets: Vec::new(),
            warnings: Vec::new(), ..Default::default()
        });
    }

    (!pages.is_empty()).then_some(pages)
}

fn block_from_layout_detection(
    detection: &Map<String, Value>,
    page_number: usize,
) -> Option<Block> {
    let category = detection
        .get("category_type")
        .and_then(Value::as_str)
        .unwrap_or("annotation");
    let bbox = detection.get("poly").and_then(bbox_from_poly);

    if category == "table" {
        if let Some(html) = first_string_field(detection, &["html", "html_2", "html_3"]) {
            let rows = html_table_rows(html);
            if !rows.is_empty() {
                let (headers, rows) = split_table_rows(rows);
                return Some(Block::Table(TableBlock {
                    headers,
                    rows,
                    caption: None,
                    bbox,
                    cells: Vec::new(),
                    source_anchors: vec![source_anchor(page_number, bbox)],
                    confidence: Some(confidence()), ..Default::default()
                }));
            }
        }
    }

    if let Some(text) = layout_detection_text(detection) {
        return Some(Block::Text(TextBlock {
            text,
            kind: category.to_owned(),
            bbox,
            lines: Vec::new(),
            source_anchors: vec![source_anchor(page_number, bbox)],
            confidence: Some(confidence()), ..Default::default()
        }));
    }

    if category == "figure" || category == "chart_mask" {
        return Some(Block::Figure(FigureBlock {
            alt_text: None,
            caption: None,
            bbox,
            image_ref: None,
            source_anchors: vec![source_anchor(page_number, bbox)],
            confidence: Some(confidence()), ..Default::default()
        }));
    }

    None
}

fn layout_detection_text(detection: &Map<String, Value>) -> Option<String> {
    first_string_field(detection, &["text", "latex"])
        .map(clean_text)
        .filter(|text| !text.is_empty())
        .or_else(|| {
            first_string_field(detection, &["html", "html_2", "html_3"])
                .map(html_to_text)
                .map(|text| clean_text(&text))
                .filter(|text| !text.is_empty())
        })
}

fn generic_page_from_value(value: &Value, page_number: usize) -> Page {
    let mut records = Vec::new();
    collect_generic_text_records(value, &mut records);
    if records.is_empty() {
        if let Some(text) = scalar_text(value) {
            records.push(TextRecord {
                kind: "value".to_owned(),
                text,
            });
        }
    }

    let blocks = records
        .into_iter()
        .filter(|record| !record.text.is_empty())
        .map(|record| {
            Block::Text(TextBlock {
                text: record.text,
                kind: record.kind,
                bbox: None,
                lines: Vec::new(),
                source_anchors: vec![source_anchor(page_number, None)],
                confidence: Some(confidence()), ..Default::default()
            })
        })
        .collect();

    Page {
        number: page_number,
        width: None,
        height: None,
        rotation: None,
        bbox: None,
        blocks,
        images: Vec::new(),
        assets: Vec::new(),
        warnings: Vec::new(), ..Default::default()
    }
}

fn collect_generic_text_records(value: &Value, records: &mut Vec<TextRecord>) {
    match value {
        Value::Object(object) => {
            let before = records.len();
            for key in [
                "title",
                "abstract",
                "body_text",
                "full_text",
                "paragraphs",
                "sections",
                "content",
                "body",
                "text",
                "latex",
                "html",
                "caption",
            ] {
                if let Some(child) = object.get(key) {
                    collect_value_for_text_key(key, child, records);
                }
            }
            if records.len() != before {
                return;
            }

            for (key, child) in object {
                if should_recurse_generic_key(key) {
                    collect_generic_text_records(child, records);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_generic_text_records(item, records);
            }
        }
        Value::String(text) => push_record(records, "text", text),
        _ => {}
    }
}

fn collect_value_for_text_key(key: &str, value: &Value, records: &mut Vec<TextRecord>) {
    match value {
        Value::String(text) => {
            let text = if key == "html" {
                html_to_text(text)
            } else {
                text.clone()
            };
            push_record(records, normalized_kind(key), &text);
        }
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(text) => push_record(records, normalized_kind(key), text),
                    Value::Object(_) => collect_generic_text_records(item, records),
                    _ => {}
                }
            }
        }
        Value::Object(_) => collect_generic_text_records(value, records),
        _ => {}
    }
}

fn push_record(records: &mut Vec<TextRecord>, kind: &str, text: &str) {
    let text = clean_text(text);
    if !text.is_empty() {
        records.push(TextRecord {
            kind: kind.to_owned(),
            text,
        });
    }
}

fn build_document(source: &Source, engine_name: &str, mut pages: Vec<Page>) -> Document {
    if pages.is_empty() {
        pages.push(Page {
            number: 1,
            width: None,
            height: None,
            rotation: None,
            bbox: None,
            blocks: Vec::new(),
            images: Vec::new(),
            assets: Vec::new(),
            warnings: Vec::new(), ..Default::default()
        });
    }

    let (character_count, word_count, block_count) = document_counts(&pages);
    let title = first_title(&pages);
    Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title,
            character_count,
            word_count,
            block_count,
            file_size_bytes: source.bytes.as_ref().map(|bytes| bytes.len() as u64),
            pdf_version: None,
            encrypted: false,
        },
        pages,
        assets: Vec::new(),
        warnings: Vec::new(),
    }
}

fn document_counts(pages: &[Page]) -> (usize, usize, usize) {
    let mut character_count = 0;
    let mut word_count = 0;
    let mut block_count = 0;
    for page in pages {
        for block in &page.blocks {
            let text = block_text(block);
            character_count += text.chars().count();
            word_count += text.split_whitespace().count();
            block_count += 1;
        }
    }
    (character_count, word_count, block_count)
}

fn first_title(pages: &[Page]) -> Option<String> {
    pages.iter().find_map(|page| {
        page.blocks.iter().find_map(|block| match block {
            Block::Text(text) if text.kind == "title" => Some(text.text.clone()),
            _ => None,
        })
    })
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

fn block_bbox(block: &Block) -> Option<BBox> {
    match block {
        Block::Text(text) => text.bbox,
        Block::Table(table) => table.bbox,
        Block::Figure(figure) => figure.bbox,
    }
}

fn html_table_rows(html: &str) -> Vec<Vec<String>> {
    let lower = html.to_ascii_lowercase();
    let mut rows = Vec::new();
    let mut pos = 0;

    while let Some(row_start_offset) = lower[pos..].find("<tr") {
        let row_start = pos + row_start_offset;
        let Some(open_end_offset) = lower[row_start..].find('>') else {
            break;
        };
        let content_start = row_start + open_end_offset + 1;
        let Some(close_offset) = lower[content_start..].find("</tr>") else {
            break;
        };
        let content_end = content_start + close_offset;
        let row = html_row_cells(&html[content_start..content_end]);
        if !row.is_empty() {
            rows.push(row);
        }
        pos = content_end + "</tr>".len();
    }

    rows
}

fn html_row_cells(row_html: &str) -> Vec<String> {
    let lower = row_html.to_ascii_lowercase();
    let mut cells = Vec::new();
    let mut pos = 0;

    while let Some((tag, cell_start_offset)) = next_cell_tag(&lower[pos..]) {
        let cell_start = pos + cell_start_offset;
        let Some(open_end_offset) = lower[cell_start..].find('>') else {
            break;
        };
        let content_start = cell_start + open_end_offset + 1;
        let close_tag = format!("</{tag}>");
        let Some(close_offset) = lower[content_start..].find(&close_tag) else {
            break;
        };
        let content_end = content_start + close_offset;
        let text = clean_text(&html_to_text(&row_html[content_start..content_end]));
        cells.push(text);
        pos = content_end + close_tag.len();
    }

    cells
}

fn next_cell_tag(input: &str) -> Option<(&'static str, usize)> {
    let td = input.find("<td").map(|index| ("td", index));
    let th = input.find("<th").map(|index| ("th", index));
    match (td, th) {
        (Some(left), Some(right)) => Some(if left.1 <= right.1 { left } else { right }),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn split_table_rows(mut rows: Vec<Vec<String>>) -> (Vec<String>, Vec<Vec<String>>) {
    if rows.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let headers = rows.remove(0);
    (headers, rows)
}

fn source_anchor(page_number: usize, bbox: Option<BBox>) -> SourceAnchor {
    SourceAnchor {
        page_number,
        pdf_object_ids: Vec::new(),
        bbox,
        extraction_method: EXTRACTION_METHOD.to_owned(),
    }
}

fn confidence() -> Confidence {
    Confidence {
        score: 0.9,
        calibrated: false,
    }
}

fn bbox_from_poly(value: &Value) -> Option<BBox> {
    let points = value.as_array()?;
    if points.len() < 4 {
        return None;
    }

    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for pair in points.chunks(2) {
        if pair.len() != 2 {
            continue;
        }
        xs.push(pair[0].as_f64()? as f32);
        ys.push(pair[1].as_f64()? as f32);
    }
    if xs.is_empty() || ys.is_empty() {
        return None;
    }
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    Some(BBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn bbox_from_rect(value: &Value) -> Option<BBox> {
    let coordinates = value.as_array()?;
    if coordinates.len() < 4 {
        return None;
    }
    let left = coordinates[0].as_f64()? as f32;
    let top = coordinates[1].as_f64()? as f32;
    let right = coordinates[2].as_f64()? as f32;
    let bottom = coordinates[3].as_f64()? as f32;
    Some(BBox {
        x: left.min(right),
        y: top.min(bottom),
        width: (right - left).abs(),
        height: (bottom - top).abs(),
    })
}

fn bbox_from_coco_rect(value: &Value) -> Option<BBox> {
    let coordinates = value.as_array()?;
    if coordinates.len() != 4 {
        return None;
    }
    Some(BBox {
        x: coordinates[0].as_f64()? as f32,
        y: coordinates[1].as_f64()? as f32,
        width: coordinates[2].as_f64()? as f32,
        height: coordinates[3].as_f64()? as f32,
    })
}

fn inferred_page_bbox(blocks: &[Block]) -> Option<BBox> {
    let mut max_x = 0.0f32;
    let mut max_y = 0.0f32;
    let mut has_bbox = false;
    for block in blocks {
        let bbox = match block {
            Block::Text(text) => text.bbox,
            Block::Table(table) => table.bbox,
            Block::Figure(figure) => figure.bbox,
        };
        let Some(bbox) = bbox else {
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

fn page_bbox(width: Option<f32>, height: Option<f32>) -> Option<BBox> {
    Some(BBox {
        x: 0.0,
        y: 0.0,
        width: width?,
        height: height?,
    })
}

fn order_value(object: &Map<String, Value>) -> f64 {
    object
        .get("order")
        .and_then(Value::as_f64)
        .unwrap_or(f64::INFINITY)
}

fn numeric_field(object: &Map<String, Value>, key: &str) -> Option<f32> {
    object.get(key)?.as_f64().map(|value| value as f32)
}

fn bool_field(object: &Map<String, Value>, key: &str) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn first_string_field<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

fn first_numeric_field(object: &Map<String, Value>, keys: &[&str]) -> Option<f32> {
    keys.iter().find_map(|key| numeric_field(object, key))
}

fn first_bbox_field(object: &Map<String, Value>, keys: &[&str]) -> Option<BBox> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(|value| bbox_from_rect(value).or_else(|| bbox_from_object(value)))
    })
}

fn bbox_from_object(value: &Value) -> Option<BBox> {
    let object = value.as_object()?;
    if let (Some(left), Some(top), Some(right), Some(bottom)) = (
        first_numeric_field(object, &["x1", "left", "l"]),
        first_numeric_field(object, &["y1", "top", "t"]),
        first_numeric_field(object, &["x2", "right", "r"]),
        first_numeric_field(object, &["y2", "bottom", "b"]),
    ) {
        return Some(BBox {
            x: left.min(right),
            y: top.min(bottom),
            width: (right - left).abs(),
            height: (bottom - top).abs(),
        });
    }

    let x = first_numeric_field(object, &["x", "left"])?;
    let y = first_numeric_field(object, &["y", "top"])?;
    let width = first_numeric_field(object, &["width", "w"])?;
    let height = first_numeric_field(object, &["height", "h"])?;
    Some(BBox {
        x,
        y,
        width,
        height,
    })
}

fn value_key(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        _ => value.to_string(),
    }
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(clean_text(text)),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        _ => None,
    }
    .filter(|text| !text.is_empty())
}

fn clean_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalized_kind(key: &str) -> &str {
    match key {
        "body_text" | "full_text" | "content" | "body" => "paragraph",
        "paragraphs" | "sections" => "paragraph",
        other => other,
    }
}

fn should_recurse_generic_key(key: &str) -> bool {
    !matches!(
        key,
        "id" | "anno_id"
            | "image"
            | "image_path"
            | "pdf"
            | "pdf_path"
            | "path"
            | "url"
            | "source"
            | "metadata"
            | "page_info"
            | "category_type"
            | "attribute"
    )
}
