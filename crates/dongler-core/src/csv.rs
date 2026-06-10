use crate::engine::ExtractionEngine;
use crate::error::Result;
use crate::ir::{
    BBox, Block, Confidence, Document, Line, Metadata, Page, SourceAnchor, Span, TextBlock,
    SCHEMA_VERSION,
};
use crate::source::Source;

const EXTRACTION_METHOD: &str = "csv_native";

#[derive(Debug, Default, Clone, Copy)]
pub struct CsvEngine;

impl ExtractionEngine for CsvEngine {
    fn name(&self) -> &'static str {
        "csv-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        let delimiter = delimiter_for_source(source);
        Ok(build_document(
            source,
            self.name(),
            parse_rows(source, delimiter),
        ))
    }
}

fn parse_rows(source: &Source, delimiter: char) -> Vec<Block> {
    if let Some(blocks) = tesseract_tsv_blocks(source, delimiter) {
        return blocks;
    }
    if let Some(blocks) = ckorzen_tsv_blocks(source, delimiter) {
        return blocks;
    }

    source
        .content
        .lines()
        .filter_map(|line| block_from_line(line, delimiter))
        .collect()
}

fn block_from_line(line: &str, delimiter: char) -> Option<Block> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let cells = trimmed
        .split(delimiter)
        .map(|cell| cell.trim().trim_matches('"').to_owned())
        .collect::<Vec<_>>();
    let (bbox, text) = if let Some((bbox, text)) = ocr_box_row(&cells, delimiter) {
        (Some(bbox), text)
    } else {
        (
            None,
            cells
                .iter()
                .filter(|cell| !cell.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" "),
        )
    };
    let text = clean_text(&text);
    if text.is_empty() {
        return None;
    }

    Some(Block::Text(TextBlock {
        text,
        kind: "row".to_owned(),
        bbox,
        lines: Vec::new(),
        source_anchors: vec![SourceAnchor {
            page_number: 1,
            pdf_object_ids: Vec::new(),
            bbox,
            extraction_method: EXTRACTION_METHOD.to_owned(),
        }],
        confidence: Some(Confidence {
            score: 0.9,
            calibrated: false,
        }),
    }))
}

fn ckorzen_tsv_blocks(source: &Source, delimiter: char) -> Option<Vec<Block>> {
    if delimiter != '\t' {
        return None;
    }

    let mut lines = source.content.lines();
    let header_line = lines.find(|line| !line.trim().is_empty())?;
    let headers = split_delimited_cells(header_line, delimiter);
    let feature_column = header_index(&headers, "feature")?;
    let boxes_column = header_index(&headers, "bounding boxes")?;
    let text_column = header_index(&headers, "text")?;
    let required_max_index = feature_column.max(boxes_column).max(text_column);
    let mut blocks = Vec::new();

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells = split_delimited_cells(line, delimiter);
        if cells.len() <= required_max_index {
            continue;
        }

        let text = clean_text(&cells[text_column..].join("\t"));
        if text.is_empty() {
            continue;
        }

        let kind = clean_text(&cells[feature_column]);
        let anchors = ckorzen_bounding_boxes(&cells[boxes_column]);
        let bbox = bbox_union(anchors.iter().map(|(_, bbox)| *bbox));
        let source_anchors = if anchors.is_empty() {
            vec![SourceAnchor {
                page_number: 1,
                pdf_object_ids: Vec::new(),
                bbox: None,
                extraction_method: EXTRACTION_METHOD.to_owned(),
            }]
        } else {
            anchors
                .iter()
                .map(|(page_number, bbox)| SourceAnchor {
                    page_number: *page_number,
                    pdf_object_ids: Vec::new(),
                    bbox: Some(*bbox),
                    extraction_method: EXTRACTION_METHOD.to_owned(),
                })
                .collect()
        };

        blocks.push(Block::Text(TextBlock {
            text,
            kind: if kind.is_empty() {
                "row".to_owned()
            } else {
                kind
            },
            bbox,
            lines: Vec::new(),
            source_anchors,
            confidence: Some(Confidence {
                score: 0.9,
                calibrated: false,
            }),
        }));
    }

    (!blocks.is_empty()).then_some(blocks)
}

fn ckorzen_bounding_boxes(cell: &str) -> Vec<(usize, BBox)> {
    cell.split("),")
        .filter_map(|part| {
            let part = part.trim().trim_start_matches('(').trim_end_matches(')');
            let (page_number, coordinates) = part.split_once(";[")?;
            let page_number = page_number.parse::<usize>().ok()?.max(1);
            let coordinates = coordinates.trim_end_matches(']');
            let coordinates = coordinates
                .split(';')
                .map(str::parse::<f32>)
                .collect::<std::result::Result<Vec<_>, _>>()
                .ok()?;
            if coordinates.len() != 4 {
                return None;
            }
            let x = coordinates[0];
            let y = coordinates[1];
            let width = coordinates[2] - coordinates[0];
            let height = coordinates[3] - coordinates[1];
            if width <= 0.0 || height <= 0.0 {
                return None;
            }
            Some((
                page_number,
                BBox {
                    x,
                    y,
                    width,
                    height,
                },
            ))
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct TesseractTsvColumns {
    level: usize,
    page_num: usize,
    block_num: usize,
    par_num: usize,
    line_num: usize,
    word_num: usize,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    conf: usize,
    text: usize,
}

#[derive(Debug)]
struct TesseractWord {
    text: String,
    bbox: BBox,
    confidence: Option<f32>,
}

fn tesseract_tsv_blocks(source: &Source, delimiter: char) -> Option<Vec<Block>> {
    if delimiter != '\t' {
        return None;
    }

    let mut lines = source.content.lines();
    let header_line = lines.find(|line| !line.trim().is_empty())?;
    let columns = TesseractTsvColumns::from_header(&split_delimited_cells(header_line, delimiter))?;
    let required_max_index = columns.required_max_index();
    let mut groups: Vec<((usize, usize, usize, usize), Vec<TesseractWord>)> = Vec::new();

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells = split_delimited_cells(line, delimiter);
        if cells.len() <= required_max_index || cells.len() <= columns.text {
            continue;
        }
        if parse_usize_cell(&cells, columns.level) != Some(5) {
            continue;
        }

        let text = clean_text(&cells[columns.text..].join("\t"));
        if text.is_empty() {
            continue;
        }

        let Some(bbox) = tesseract_bbox(&cells, columns) else {
            continue;
        };
        let page_number = parse_usize_cell(&cells, columns.page_num)
            .unwrap_or(1)
            .max(1);
        let key = (
            page_number,
            parse_usize_cell(&cells, columns.block_num).unwrap_or(0),
            parse_usize_cell(&cells, columns.par_num).unwrap_or(0),
            parse_usize_cell(&cells, columns.line_num).unwrap_or(0),
        );
        let word = TesseractWord {
            text,
            bbox,
            confidence: parse_confidence_cell(&cells, columns.conf),
        };

        if let Some((_, words)) = groups
            .iter_mut()
            .find(|(existing_key, _)| *existing_key == key)
        {
            words.push(word);
        } else {
            groups.push((key, vec![word]));
        }
    }

    if groups.is_empty() {
        return None;
    }

    Some(
        groups
            .into_iter()
            .filter_map(tesseract_line_block)
            .collect(),
    )
}

impl TesseractTsvColumns {
    fn from_header(headers: &[String]) -> Option<Self> {
        Some(Self {
            level: header_index(headers, "level")?,
            page_num: header_index(headers, "page_num")?,
            block_num: header_index(headers, "block_num")?,
            par_num: header_index(headers, "par_num")?,
            line_num: header_index(headers, "line_num")?,
            word_num: header_index(headers, "word_num")?,
            left: header_index(headers, "left")?,
            top: header_index(headers, "top")?,
            width: header_index(headers, "width")?,
            height: header_index(headers, "height")?,
            conf: header_index(headers, "conf")?,
            text: header_index(headers, "text")?,
        })
    }

    fn required_max_index(self) -> usize {
        [
            self.level,
            self.page_num,
            self.block_num,
            self.par_num,
            self.line_num,
            self.word_num,
            self.left,
            self.top,
            self.width,
            self.height,
            self.conf,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }
}

fn tesseract_line_block(
    ((page_number, _, _, _), words): ((usize, usize, usize, usize), Vec<TesseractWord>),
) -> Option<Block> {
    if words.is_empty() {
        return None;
    }

    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let bbox = bbox_union(words.iter().map(|word| word.bbox))?;
    let spans = words
        .iter()
        .map(|word| Span {
            text: word.text.clone(),
            bbox: Some(word.bbox),
            font: None,
            size: None,
            bold: false,
            italic: false,
        })
        .collect::<Vec<_>>();
    let confidence = average_confidence(words.iter().filter_map(|word| word.confidence));

    Some(Block::Text(TextBlock {
        text: text.clone(),
        kind: "ocr_line".to_owned(),
        bbox: Some(bbox),
        lines: vec![Line {
            text,
            bbox: Some(bbox),
            spans,
        }],
        source_anchors: vec![SourceAnchor {
            page_number,
            pdf_object_ids: Vec::new(),
            bbox: Some(bbox),
            extraction_method: EXTRACTION_METHOD.to_owned(),
        }],
        confidence: Some(Confidence {
            score: confidence.unwrap_or(0.9),
            calibrated: false,
        }),
    }))
}

fn split_delimited_cells(line: &str, delimiter: char) -> Vec<String> {
    line.trim_end()
        .split(delimiter)
        .map(|cell| cell.trim().trim_matches('"').to_owned())
        .collect()
}

fn header_index(headers: &[String], name: &str) -> Option<usize> {
    headers
        .iter()
        .position(|header| normalize_header(header) == name)
}

fn normalize_header(header: &str) -> String {
    header
        .trim_start_matches('\u{feff}')
        .trim()
        .to_ascii_lowercase()
}

fn tesseract_bbox(cells: &[String], columns: TesseractTsvColumns) -> Option<BBox> {
    let x = parse_f32_cell(cells, columns.left)?;
    let y = parse_f32_cell(cells, columns.top)?;
    let width = parse_f32_cell(cells, columns.width)?;
    let height = parse_f32_cell(cells, columns.height)?;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(BBox {
        x,
        y,
        width,
        height,
    })
}

fn parse_usize_cell(cells: &[String], index: usize) -> Option<usize> {
    cells.get(index)?.parse::<usize>().ok()
}

fn parse_f32_cell(cells: &[String], index: usize) -> Option<f32> {
    cells.get(index)?.parse::<f32>().ok()
}

fn parse_confidence_cell(cells: &[String], index: usize) -> Option<f32> {
    let confidence = parse_f32_cell(cells, index)?;
    if confidence < 0.0 {
        return None;
    }
    if confidence > 1.0 {
        Some((confidence / 100.0).clamp(0.0, 1.0))
    } else {
        Some(confidence)
    }
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

fn average_confidence(confidences: impl Iterator<Item = f32>) -> Option<f32> {
    let mut total = 0.0;
    let mut count = 0usize;
    for confidence in confidences {
        total += confidence;
        count += 1;
    }
    (count > 0).then_some(total / count as f32)
}

fn ocr_box_row(cells: &[String], delimiter: char) -> Option<(BBox, String)> {
    if cells.len() < 9 {
        return None;
    }
    let mut coordinates = [0.0f32; 8];
    for (index, coordinate) in coordinates.iter_mut().enumerate() {
        *coordinate = cells[index].parse::<f32>().ok()?;
    }
    let xs = [
        coordinates[0],
        coordinates[2],
        coordinates[4],
        coordinates[6],
    ];
    let ys = [
        coordinates[1],
        coordinates[3],
        coordinates[5],
        coordinates[7],
    ];
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let separator = if delimiter == ',' { ", " } else { "\t" };
    let text = cells[8..].join(&separator);
    Some((
        BBox {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        },
        text,
    ))
}

fn build_document(source: &Source, engine_name: &str, blocks: Vec<Block>) -> Document {
    let page_bbox = inferred_page_bbox(&blocks);
    let (character_count, word_count) = text_counts(&blocks);
    let block_count = blocks.len();
    Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title: None,
            character_count,
            word_count,
            block_count,
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
    }
}

fn inferred_page_bbox(blocks: &[Block]) -> Option<BBox> {
    let mut max_x = 0.0f32;
    let mut max_y = 0.0f32;
    let mut has_bbox = false;
    for block in blocks {
        let Some(bbox) = block_bbox(block) else {
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

fn block_bbox(block: &Block) -> Option<BBox> {
    match block {
        Block::Text(text) => text.bbox,
        Block::Table(table) => table.bbox,
        Block::Figure(figure) => figure.bbox,
    }
}

fn text_counts(blocks: &[Block]) -> (usize, usize) {
    let mut character_count = 0;
    let mut word_count = 0;
    for block in blocks {
        let text = match block {
            Block::Text(text) => text.text.as_str(),
            _ => "",
        };
        character_count += text.chars().count();
        word_count += text.split_whitespace().count();
    }
    (character_count, word_count)
}

fn delimiter_for_source(source: &Source) -> char {
    if source
        .path
        .as_deref()
        .map(|path| path.to_ascii_lowercase().ends_with(".tsv"))
        .unwrap_or(false)
    {
        '\t'
    } else {
        ','
    }
}

fn clean_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
