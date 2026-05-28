use crate::engine::{text_document_from_text, ExtractionEngine};
use crate::error::Result;
use crate::ir::{
    BBox, Block, Confidence, Document, Line, Metadata, Page, SourceAnchor, Span, TextBlock,
    SCHEMA_VERSION,
};
use crate::source::Source;

#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlEngine;

#[derive(Debug, Default, Clone, Copy)]
pub struct EmailEngine;

#[derive(Debug, Default, Clone, Copy)]
pub struct XmlEngine;

impl ExtractionEngine for HtmlEngine {
    fn name(&self) -> &'static str {
        "html-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        if let Some(document) = hocr_document(source, self.name()) {
            return Ok(document);
        }
        text_document_from_text(source, self.name(), &html_to_text(&source.content), None)
    }
}

impl ExtractionEngine for EmailEngine {
    fn name(&self) -> &'static str {
        "email-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        let email = parse_email(&source.content);
        let text = match (&email.subject, email.body.trim()) {
            (Some(subject), body) if !body.is_empty() => format!("{subject}\n\n{body}"),
            (Some(subject), _) => subject.clone(),
            (None, body) => body.to_owned(),
        };
        text_document_from_text(source, self.name(), &text, email.subject)
    }
}

impl ExtractionEngine for XmlEngine {
    fn name(&self) -> &'static str {
        "xml-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        if let Some(document) = page_xml_document(source, self.name()) {
            return Ok(document);
        }
        if let Some(document) = alto_document(source, self.name()) {
            return Ok(document);
        }
        if let Some(document) = pascal_voc_document(source, self.name()) {
            return Ok(document);
        }
        text_document_from_text(source, self.name(), &html_to_text(&source.content), None)
    }
}

#[derive(Debug, Default)]
struct EmailParts {
    subject: Option<String>,
    body: String,
}

#[derive(Debug)]
struct PascalVocObject {
    name: String,
    bbox: BBox,
}

#[derive(Debug, Clone, Copy)]
struct XmlElement<'a> {
    start_tag: &'a str,
    content: &'a str,
}

#[derive(Debug, Clone)]
struct AltoWord {
    text: String,
    bbox: Option<BBox>,
    confidence: Option<f32>,
}

#[derive(Debug, Clone)]
struct HocrWord {
    text: String,
    bbox: Option<BBox>,
    confidence: Option<f32>,
}

#[derive(Debug, Clone)]
struct PageXmlWord {
    text: String,
    bbox: Option<BBox>,
    confidence: Option<f32>,
}

fn hocr_document(source: &Source, engine_name: &str) -> Option<Document> {
    if !source.content.contains("ocr_page")
        && !source.content.contains("ocr_line")
        && !source.content.contains("ocrx_word")
    {
        return None;
    }

    let page_element = hocr_elements_with_class(&source.content, "ocr_page")
        .into_iter()
        .next();
    let page_content = page_element
        .as_ref()
        .map(|element| element.content)
        .unwrap_or(source.content.as_str());
    let page_bbox = page_element
        .as_ref()
        .and_then(|element| hocr_bbox_from_tag(element.start_tag));
    let mut blocks = hocr_elements_with_any_class(page_content, &["ocr_line", "ocrx_line"])
        .into_iter()
        .filter_map(|line| hocr_line_block(line, 1))
        .collect::<Vec<_>>();

    if blocks.is_empty() {
        blocks = hocr_elements_with_class(page_content, "ocrx_word")
            .into_iter()
            .filter_map(hocr_word_from_element)
            .map(|word| hocr_word_block(word, 1))
            .collect();
    }
    if blocks.is_empty() {
        return None;
    }

    let page_bbox = page_bbox.or_else(|| inferred_block_bbox(&blocks));
    let text = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title: None,
            character_count: text.chars().count(),
            word_count: text.split_whitespace().count(),
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

fn hocr_line_block(line: XmlElement<'_>, page_number: usize) -> Option<Block> {
    let words = hocr_elements_with_class(line.content, "ocrx_word")
        .into_iter()
        .filter_map(hocr_word_from_element)
        .collect::<Vec<_>>();
    if words.is_empty() {
        let text = html_to_text(line.content);
        if text.trim().is_empty() {
            return None;
        }
        let bbox = hocr_bbox_from_tag(line.start_tag);
        return Some(Block::Text(TextBlock {
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            kind: "ocr_line".to_owned(),
            bbox,
            lines: Vec::new(),
            source_anchors: vec![html_source_anchor(page_number, bbox)],
            confidence: Some(Confidence {
                score: 0.9,
                calibrated: false,
            }),
        }));
    }

    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let bbox = hocr_bbox_from_tag(line.start_tag).or_else(|| inferred_hocr_word_bbox(&words));
    let confidence = mean_confidence(words.iter().filter_map(|word| word.confidence));
    let spans = words
        .iter()
        .map(|word| Span {
            text: word.text.clone(),
            bbox: word.bbox,
            font: None,
            size: None,
        })
        .collect::<Vec<_>>();

    Some(Block::Text(TextBlock {
        text: text.clone(),
        kind: "ocr_line".to_owned(),
        bbox,
        lines: vec![Line { text, bbox, spans }],
        source_anchors: vec![html_source_anchor(page_number, bbox)],
        confidence: Some(Confidence {
            score: confidence.unwrap_or(0.9),
            calibrated: false,
        }),
    }))
}

fn hocr_word_block(word: HocrWord, page_number: usize) -> Block {
    Block::Text(TextBlock {
        text: word.text.clone(),
        kind: "ocr_word".to_owned(),
        bbox: word.bbox,
        lines: Vec::new(),
        source_anchors: vec![html_source_anchor(page_number, word.bbox)],
        confidence: Some(Confidence {
            score: word.confidence.unwrap_or(0.9),
            calibrated: false,
        }),
    })
}

fn hocr_word_from_element(element: XmlElement<'_>) -> Option<HocrWord> {
    let text = html_to_text(element.content)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        return None;
    }
    Some(HocrWord {
        text,
        bbox: hocr_bbox_from_tag(element.start_tag),
        confidence: hocr_confidence_from_tag(element.start_tag),
    })
}

fn page_xml_document(source: &Source, engine_name: &str) -> Option<Document> {
    if !source.content.contains("PcGts") && !source.content.contains("TextRegion") {
        return None;
    }

    let page_element = xml_elements_by_local_name(&source.content, "Page")
        .into_iter()
        .next()?;
    let width = first_xml_attr_f32(
        page_element.start_tag,
        &["imageWidth", "image_width", "WIDTH", "width"],
    );
    let height = first_xml_attr_f32(
        page_element.start_tag,
        &["imageHeight", "image_height", "HEIGHT", "height"],
    );
    let blocks = xml_elements_by_local_name(page_element.content, "TextLine")
        .into_iter()
        .filter_map(|line| page_xml_line_block(line, 1))
        .collect::<Vec<_>>();
    if blocks.is_empty() {
        return None;
    }

    let page_bbox = page_bbox(width, height).or_else(|| inferred_block_bbox(&blocks));
    let text = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title: None,
            character_count: text.chars().count(),
            word_count: text.split_whitespace().count(),
            block_count: blocks.len(),
            file_size_bytes: source.bytes.as_ref().map(|bytes| bytes.len() as u64),
            pdf_version: None,
            encrypted: false,
        },
        pages: vec![Page {
            number: 1,
            width: width.or_else(|| page_bbox.map(|bbox| bbox.width)),
            height: height.or_else(|| page_bbox.map(|bbox| bbox.height)),
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

fn page_xml_line_block(line: XmlElement<'_>, page_number: usize) -> Option<Block> {
    let words = xml_elements_by_local_name(line.content, "Word")
        .into_iter()
        .filter_map(page_xml_word_from_element)
        .collect::<Vec<_>>();
    let bbox =
        page_xml_bbox_from_content(line.content).or_else(|| inferred_page_xml_word_bbox(&words));

    if words.is_empty() {
        let text = page_xml_text_from_content(line.content)?;
        if text.is_empty() {
            return None;
        }
        return Some(Block::Text(TextBlock {
            text,
            kind: "ocr_line".to_owned(),
            bbox,
            lines: Vec::new(),
            source_anchors: vec![xml_source_anchor(page_number, bbox)],
            confidence: Some(Confidence {
                score: page_xml_confidence_from_content(line.content).unwrap_or(0.9),
                calibrated: false,
            }),
        }));
    }

    let text = page_xml_text_from_content(line.content).unwrap_or_else(|| {
        words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    });
    let confidence = mean_confidence(words.iter().filter_map(|word| word.confidence));
    let spans = words
        .iter()
        .map(|word| Span {
            text: word.text.clone(),
            bbox: word.bbox,
            font: None,
            size: None,
        })
        .collect::<Vec<_>>();

    Some(Block::Text(TextBlock {
        text: text.clone(),
        kind: "ocr_line".to_owned(),
        bbox,
        lines: vec![Line { text, bbox, spans }],
        source_anchors: vec![xml_source_anchor(page_number, bbox)],
        confidence: Some(Confidence {
            score: confidence.unwrap_or(0.9),
            calibrated: false,
        }),
    }))
}

fn page_xml_word_from_element(element: XmlElement<'_>) -> Option<PageXmlWord> {
    let text = page_xml_text_from_content(element.content)?;
    if text.is_empty() {
        return None;
    }
    Some(PageXmlWord {
        text,
        bbox: page_xml_bbox_from_content(element.content),
        confidence: page_xml_confidence_from_content(element.content),
    })
}

fn alto_document(source: &Source, engine_name: &str) -> Option<Document> {
    let page_element = xml_elements_by_local_name(&source.content, "Page")
        .into_iter()
        .next()?;
    let width = xml_attr_f32(page_element.start_tag, "WIDTH");
    let height = xml_attr_f32(page_element.start_tag, "HEIGHT");
    let mut blocks = xml_elements_by_local_name(page_element.content, "TextLine")
        .into_iter()
        .filter_map(|line| alto_line_block(line, 1))
        .collect::<Vec<_>>();

    if blocks.is_empty() {
        blocks = xml_start_tags_by_local_name(page_element.content, "String")
            .into_iter()
            .filter_map(|tag| alto_word_from_tag(tag))
            .map(|word| alto_word_block(word, 1))
            .collect();
    }
    if blocks.is_empty() {
        return None;
    }

    let page_bbox = page_bbox(width, height).or_else(|| inferred_block_bbox(&blocks));
    let text = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title: None,
            character_count: text.chars().count(),
            word_count: text.split_whitespace().count(),
            block_count: blocks.len(),
            file_size_bytes: source.bytes.as_ref().map(|bytes| bytes.len() as u64),
            pdf_version: None,
            encrypted: false,
        },
        pages: vec![Page {
            number: 1,
            width: width.or_else(|| page_bbox.map(|bbox| bbox.width)),
            height: height.or_else(|| page_bbox.map(|bbox| bbox.height)),
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

fn alto_line_block(line: XmlElement<'_>, page_number: usize) -> Option<Block> {
    let words = xml_start_tags_by_local_name(line.content, "String")
        .into_iter()
        .filter_map(alto_word_from_tag)
        .collect::<Vec<_>>();
    if words.is_empty() {
        return None;
    }
    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let bbox = alto_bbox_from_tag(line.start_tag).or_else(|| inferred_word_bbox(&words));
    let confidence = mean_confidence(words.iter().filter_map(|word| word.confidence));
    let spans = words
        .iter()
        .map(|word| Span {
            text: word.text.clone(),
            bbox: word.bbox,
            font: None,
            size: None,
        })
        .collect::<Vec<_>>();

    Some(Block::Text(TextBlock {
        text,
        kind: "ocr_line".to_owned(),
        bbox,
        lines: vec![Line {
            text: words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            bbox,
            spans,
        }],
        source_anchors: vec![xml_source_anchor(page_number, bbox)],
        confidence: Some(Confidence {
            score: confidence.unwrap_or(0.9),
            calibrated: false,
        }),
    }))
}

fn alto_word_block(word: AltoWord, page_number: usize) -> Block {
    Block::Text(TextBlock {
        text: word.text.clone(),
        kind: "ocr_word".to_owned(),
        bbox: word.bbox,
        lines: Vec::new(),
        source_anchors: vec![xml_source_anchor(page_number, word.bbox)],
        confidence: Some(Confidence {
            score: word.confidence.unwrap_or(0.9),
            calibrated: false,
        }),
    })
}

fn alto_word_from_tag(tag: &str) -> Option<AltoWord> {
    let text = xml_attr_value(tag, "CONTENT")
        .map(|value| html_to_text(&value))
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))?;
    if text.is_empty() {
        return None;
    }
    Some(AltoWord {
        text,
        bbox: alto_bbox_from_tag(tag),
        confidence: xml_attr_f32(tag, "WC"),
    })
}

fn alto_bbox_from_tag(tag: &str) -> Option<BBox> {
    Some(BBox {
        x: xml_attr_f32(tag, "HPOS")?,
        y: xml_attr_f32(tag, "VPOS")?,
        width: xml_attr_f32(tag, "WIDTH")?,
        height: xml_attr_f32(tag, "HEIGHT")?,
    })
}

fn hocr_bbox_from_tag(tag: &str) -> Option<BBox> {
    let title = xml_attr_value(tag, "title")?;
    let mut parts = title
        .split(';')
        .find_map(|part| part.trim().strip_prefix("bbox "))?
        .split_whitespace();
    let left = parts.next()?.parse::<f32>().ok()?;
    let top = parts.next()?.parse::<f32>().ok()?;
    let right = parts.next()?.parse::<f32>().ok()?;
    let bottom = parts.next()?.parse::<f32>().ok()?;
    Some(BBox {
        x: left.min(right),
        y: top.min(bottom),
        width: (right - left).abs(),
        height: (bottom - top).abs(),
    })
}

fn hocr_confidence_from_tag(tag: &str) -> Option<f32> {
    let title = xml_attr_value(tag, "title")?;
    let value = title
        .split(';')
        .find_map(|part| part.trim().strip_prefix("x_wconf "))?
        .split_whitespace()
        .next()?
        .parse::<f32>()
        .ok()?;
    Some((value / 100.0).clamp(0.0, 1.0))
}

fn page_xml_text_from_content(content: &str) -> Option<String> {
    xml_elements_by_local_name(content, "Unicode")
        .into_iter()
        .last()
        .map(|unicode| html_to_text(unicode.content))
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|text| !text.is_empty())
}

fn page_xml_confidence_from_content(content: &str) -> Option<f32> {
    xml_elements_by_local_name(content, "TextEquiv")
        .into_iter()
        .last()
        .and_then(|element| xml_attr_f32(element.start_tag, "conf"))
}

fn page_xml_bbox_from_content(content: &str) -> Option<BBox> {
    let coords = xml_start_tags_by_local_name(content, "Coords")
        .into_iter()
        .next()?;
    let points = xml_attr_value(coords, "points")?;
    bbox_from_points(&points)
}

fn bbox_from_points(points: &str) -> Option<BBox> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut count = 0usize;

    for point in points.split_whitespace() {
        let Some((x, y)) = point.split_once(',') else {
            continue;
        };
        let x = x.parse::<f32>().ok()?;
        let y = y.parse::<f32>().ok()?;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        count += 1;
    }

    (count > 0).then_some(BBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn inferred_page_xml_word_bbox(words: &[PageXmlWord]) -> Option<BBox> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut has_bbox = false;
    for word in words {
        let Some(bbox) = word.bbox else {
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

fn inferred_hocr_word_bbox(words: &[HocrWord]) -> Option<BBox> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut has_bbox = false;
    for word in words {
        let Some(bbox) = word.bbox else {
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

fn inferred_word_bbox(words: &[AltoWord]) -> Option<BBox> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut has_bbox = false;
    for word in words {
        let Some(bbox) = word.bbox else {
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

fn inferred_block_bbox(blocks: &[Block]) -> Option<BBox> {
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

fn page_bbox(width: Option<f32>, height: Option<f32>) -> Option<BBox> {
    Some(BBox {
        x: 0.0,
        y: 0.0,
        width: width?,
        height: height?,
    })
}

fn xml_source_anchor(page_number: usize, bbox: Option<BBox>) -> SourceAnchor {
    SourceAnchor {
        page_number,
        pdf_object_ids: Vec::new(),
        bbox,
        extraction_method: "xml_native".to_owned(),
    }
}

fn html_source_anchor(page_number: usize, bbox: Option<BBox>) -> SourceAnchor {
    SourceAnchor {
        page_number,
        pdf_object_ids: Vec::new(),
        bbox,
        extraction_method: "html_native".to_owned(),
    }
}

fn mean_confidence(values: impl Iterator<Item = f32>) -> Option<f32> {
    let mut total = 0.0f32;
    let mut count = 0usize;
    for value in values {
        total += value;
        count += 1;
    }
    (count > 0).then_some(total / count as f32)
}

fn pascal_voc_document(source: &Source, engine_name: &str) -> Option<Document> {
    let width = tag_text(&source.content, "width")?.parse::<f32>().ok()?;
    let height = tag_text(&source.content, "height")?.parse::<f32>().ok()?;
    let objects = pascal_voc_objects(&source.content);
    if objects.is_empty() {
        return None;
    }

    let blocks = objects
        .into_iter()
        .map(|object| {
            Block::Text(TextBlock {
                kind: object.name.clone(),
                text: object.name,
                bbox: Some(object.bbox),
                lines: Vec::new(),
                source_anchors: vec![SourceAnchor {
                    page_number: 1,
                    pdf_object_ids: Vec::new(),
                    bbox: Some(object.bbox),
                    extraction_method: "xml_native".to_owned(),
                }],
                confidence: Some(Confidence {
                    score: 0.9,
                    calibrated: false,
                }),
            })
        })
        .collect::<Vec<_>>();
    let text = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(Document {
        schema_version: SCHEMA_VERSION.to_owned(),
        metadata: Metadata {
            format: source.format.clone(),
            engine: engine_name.to_owned(),
            source: source.path.clone(),
            title: None,
            character_count: text.chars().count(),
            word_count: text.split_whitespace().count(),
            block_count: blocks.len(),
            file_size_bytes: source.bytes.as_ref().map(|bytes| bytes.len() as u64),
            pdf_version: None,
            encrypted: false,
        },
        pages: vec![Page {
            number: 1,
            width: Some(width),
            height: Some(height),
            rotation: None,
            bbox: Some(BBox {
                x: 0.0,
                y: 0.0,
                width,
                height,
            }),
            blocks,
            images: Vec::new(),
            assets: Vec::new(),
            warnings: Vec::new(),
        }],
        assets: Vec::new(),
        warnings: Vec::new(),
    })
}

fn pascal_voc_objects(xml: &str) -> Vec<PascalVocObject> {
    tag_ranges(xml, "object")
        .into_iter()
        .filter_map(|range| {
            let object_xml = &xml[range.0..range.1];
            let name = tag_text(object_xml, "name")?;
            let xmin = tag_text(object_xml, "xmin")?.parse::<f32>().ok()?;
            let ymin = tag_text(object_xml, "ymin")?.parse::<f32>().ok()?;
            let xmax = tag_text(object_xml, "xmax")?.parse::<f32>().ok()?;
            let ymax = tag_text(object_xml, "ymax")?.parse::<f32>().ok()?;
            Some(PascalVocObject {
                name,
                bbox: BBox {
                    x: xmin.min(xmax),
                    y: ymin.min(ymax),
                    width: (xmax - xmin).abs(),
                    height: (ymax - ymin).abs(),
                },
            })
        })
        .collect()
}

fn tag_text(xml: &str, tag: &str) -> Option<String> {
    let range = tag_ranges(xml, tag).into_iter().next()?;
    Some(html_to_text(&xml[range.0..range.1]).trim().to_owned())
}

fn tag_ranges(xml: &str, tag: &str) -> Vec<(usize, usize)> {
    let lower = xml.to_ascii_lowercase();
    let mut ranges = Vec::new();
    let mut search_start = 0;
    let open = format!("<{tag}");
    let close = format!("</{tag}>");

    while let Some(offset) = lower[search_start..].find(&open) {
        let open_start = search_start + offset;
        let Some(open_end_offset) = lower[open_start..].find('>') else {
            break;
        };
        let content_start = open_start + open_end_offset + 1;
        let Some(close_offset) = lower[content_start..].find(&close) else {
            break;
        };
        let content_end = content_start + close_offset;
        ranges.push((content_start, content_end));
        search_start = content_end + close.len();
    }

    ranges
}

fn xml_elements_by_local_name<'a>(xml: &'a str, local_name: &str) -> Vec<XmlElement<'a>> {
    let mut elements = Vec::new();
    let mut pos = 0usize;
    while let Some(relative_start) = xml[pos..].find('<') {
        let start = pos + relative_start;
        let Some(relative_end) = xml[start..].find('>') else {
            break;
        };
        let tag_end = start + relative_end;
        let start_tag = &xml[start..=tag_end];
        let Some(tag_name) = opening_tag_name(start_tag) else {
            pos = tag_end + 1;
            continue;
        };
        if tag_local_name(tag_name).eq_ignore_ascii_case(local_name)
            && !start_tag.trim_end().ends_with("/>")
        {
            let close = format!("</{tag_name}>");
            let content_start = tag_end + 1;
            if let Some(relative_close) = xml[content_start..].find(&close) {
                let content_end = content_start + relative_close;
                elements.push(XmlElement {
                    start_tag,
                    content: &xml[content_start..content_end],
                });
                pos = content_end + close.len();
                continue;
            }
        }
        pos = tag_end + 1;
    }
    elements
}

fn hocr_elements_with_class<'a>(html: &'a str, class_name: &str) -> Vec<XmlElement<'a>> {
    hocr_elements_with_any_class(html, &[class_name])
}

fn hocr_elements_with_any_class<'a>(html: &'a str, class_names: &[&str]) -> Vec<XmlElement<'a>> {
    let mut elements = Vec::new();
    let mut pos = 0usize;
    while let Some(relative_start) = html[pos..].find('<') {
        let start = pos + relative_start;
        let Some(relative_end) = html[start..].find('>') else {
            break;
        };
        let tag_end = start + relative_end;
        let start_tag = &html[start..=tag_end];
        let Some(tag_name) = opening_tag_name(start_tag) else {
            pos = tag_end + 1;
            continue;
        };
        if tag_has_any_class(start_tag, class_names) && !start_tag.trim_end().ends_with("/>") {
            let content_start = tag_end + 1;
            if let Some(content_end) = matching_element_content_end(html, tag_name, content_start) {
                elements.push(XmlElement {
                    start_tag,
                    content: &html[content_start..content_end],
                });
                pos = content_end + closing_tag_len(tag_name);
                continue;
            }
        }
        pos = tag_end + 1;
    }
    elements
}

fn tag_has_any_class(tag: &str, class_names: &[&str]) -> bool {
    let Some(classes) = xml_attr_value(tag, "class") else {
        return false;
    };
    classes.split_whitespace().any(|class| {
        class_names
            .iter()
            .any(|name| class.eq_ignore_ascii_case(name))
    })
}

fn matching_element_content_end(
    input: &str,
    tag_name: &str,
    content_start: usize,
) -> Option<usize> {
    let lower = input.to_ascii_lowercase();
    let tag = tag_name.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut pos = content_start;
    let mut depth = 1usize;

    loop {
        let next_open = lower[pos..].find(&open).map(|offset| pos + offset);
        let next_close = lower[pos..].find(&close).map(|offset| pos + offset)?;

        if next_open
            .map(|open_pos| open_pos < next_close)
            .unwrap_or(false)
        {
            let open_pos = next_open.unwrap();
            let after_name = open_pos + open.len();
            if is_tag_name_boundary(lower.as_bytes().get(after_name).copied()) {
                let Some(open_end_offset) = lower[open_pos..].find('>') else {
                    return None;
                };
                let open_end = open_pos + open_end_offset;
                if !lower[open_pos..=open_end].trim_end().ends_with("/>") {
                    depth += 1;
                }
                pos = open_end + 1;
            } else {
                pos = after_name;
            }
            continue;
        }

        depth -= 1;
        if depth == 0 {
            return Some(next_close);
        }
        pos = next_close + close.len();
    }
}

fn closing_tag_len(tag_name: &str) -> usize {
    tag_name.len() + 3
}

fn is_tag_name_boundary(byte: Option<u8>) -> bool {
    byte.map(|byte| matches!(byte, b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r'))
        .unwrap_or(false)
}

fn xml_start_tags_by_local_name<'a>(xml: &'a str, local_name: &str) -> Vec<&'a str> {
    let mut tags = Vec::new();
    let mut pos = 0usize;
    while let Some(relative_start) = xml[pos..].find('<') {
        let start = pos + relative_start;
        let Some(relative_end) = xml[start..].find('>') else {
            break;
        };
        let tag_end = start + relative_end;
        let start_tag = &xml[start..=tag_end];
        if opening_tag_name(start_tag)
            .map(|name| tag_local_name(name).eq_ignore_ascii_case(local_name))
            .unwrap_or(false)
        {
            tags.push(start_tag);
        }
        pos = tag_end + 1;
    }
    tags
}

fn opening_tag_name(tag: &str) -> Option<&str> {
    let inner = tag.trim().strip_prefix('<')?.trim_start();
    if inner.starts_with('/') || inner.starts_with('!') || inner.starts_with('?') {
        return None;
    }
    inner
        .split_whitespace()
        .next()
        .map(|name| name.trim_end_matches('/').trim_end_matches('>'))
        .filter(|name| !name.is_empty())
}

fn tag_local_name(name: &str) -> &str {
    name.rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(name)
}

fn xml_attr_f32(tag: &str, name: &str) -> Option<f32> {
    xml_attr_value(tag, name)?.parse::<f32>().ok()
}

fn first_xml_attr_f32(tag: &str, names: &[&str]) -> Option<f32> {
    names.iter().find_map(|name| xml_attr_f32(tag, name))
}

fn xml_attr_value(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() {
        while pos < bytes.len() && !is_xml_name_start(bytes[pos]) {
            pos += 1;
        }
        let key_start = pos;
        while pos < bytes.len() && is_xml_name_continue(bytes[pos]) {
            pos += 1;
        }
        if key_start == pos {
            break;
        }
        let key = &tag[key_start..pos];
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if bytes.get(pos) != Some(&b'=') {
            continue;
        }
        pos += 1;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let quote = *bytes.get(pos)?;
        if quote != b'"' && quote != b'\'' {
            continue;
        }
        pos += 1;
        let value_start = pos;
        while pos < bytes.len() && bytes[pos] != quote {
            pos += 1;
        }
        let value = &tag[value_start..pos];
        if key.eq_ignore_ascii_case(name) || tag_local_name(key).eq_ignore_ascii_case(name) {
            return Some(value.to_owned());
        }
        pos += 1;
    }
    None
}

fn is_xml_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':'
}

fn is_xml_name_continue(byte: u8) -> bool {
    is_xml_name_start(byte) || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
}

pub(crate) fn html_to_text(html: &str) -> String {
    let without_ignored = remove_html_ranges(html, &["script", "style", "title", "head"]);
    let mut output = String::new();
    let bytes = without_ignored.as_bytes();
    let mut pos = 0;
    let mut pending_space = false;

    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            if let Some(end) = without_ignored[pos..].find('>') {
                let tag = without_ignored[pos + 1..pos + end].trim();
                if is_block_tag(tag) {
                    push_newline(&mut output);
                }
                pos += end + 1;
                pending_space = false;
                continue;
            }
        }

        let Some(character) = without_ignored[pos..].chars().next() else {
            break;
        };
        if character == '&' {
            if let Some((decoded, consumed)) = decode_entity(&without_ignored[pos..]) {
                if pending_space {
                    output.push(' ');
                }
                output.push_str(&decoded);
                pos += consumed;
                pending_space = false;
                continue;
            }
        }
        if character.is_whitespace() {
            pending_space = !output.ends_with('\n') && !output.is_empty();
        } else {
            if pending_space {
                output.push(' ');
            }
            output.push(character);
            pending_space = false;
        }
        pos += character.len_utf8();
    }

    normalize_text_lines(&output)
}

fn remove_html_ranges(input: &str, tags: &[&str]) -> String {
    let mut output = String::new();
    let mut pos = 0;
    while pos < input.len() {
        let lower_rest = input[pos..].to_ascii_lowercase();
        let Some((tag, start)) = find_ignored_tag_start(&lower_rest, tags) else {
            output.push_str(&input[pos..]);
            break;
        };

        output.push_str(&input[pos..pos + start]);
        let after_open = pos + start;
        let close = format!("</{tag}>");
        let lower_after_open = input[after_open..].to_ascii_lowercase();
        if let Some(end) = lower_after_open.find(&close) {
            pos = after_open + end + close.len();
        } else {
            break;
        }
    }
    output
}

fn find_ignored_tag_start<'a>(lower_input: &str, tags: &[&'a str]) -> Option<(&'a str, usize)> {
    tags.iter()
        .filter_map(|tag| find_tag_start(lower_input, tag).map(|start| (*tag, start)))
        .min_by_key(|(_, start)| *start)
}

fn find_tag_start(input: &str, tag: &str) -> Option<usize> {
    let open = format!("<{tag}");
    let mut search_start = 0;
    while let Some(offset) = input[search_start..].find(&open) {
        let start = search_start + offset;
        let after_name = start + open.len();
        if input
            .as_bytes()
            .get(after_name)
            .map(|byte| matches!(byte, b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r'))
            .unwrap_or(false)
        {
            return Some(start);
        }
        search_start = after_name;
    }
    None
}

fn is_block_tag(tag: &str) -> bool {
    let name = tag
        .trim_start_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    matches!(
        name.to_ascii_lowercase().as_str(),
        "address"
            | "article"
            | "article-title"
            | "aside"
            | "abstract"
            | "back"
            | "blockquote"
            | "body"
            | "br"
            | "caption"
            | "div"
            | "footer"
            | "front"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "item"
            | "li"
            | "list"
            | "main"
            | "mixed-citation"
            | "p"
            | "ref"
            | "sec"
            | "section"
            | "table-wrap"
            | "tr"
    )
}

fn decode_entity(input: &str) -> Option<(String, usize)> {
    let end = input.find(';')?.min(16);
    let entity = &input[1..end];
    let decoded = match entity {
        "amp" => "&".to_owned(),
        "lt" => "<".to_owned(),
        "gt" => ">".to_owned(),
        "quot" => "\"".to_owned(),
        "apos" => "'".to_owned(),
        "nbsp" => " ".to_owned(),
        value if value.starts_with("#x") || value.starts_with("#X") => {
            char::from_u32(u32::from_str_radix(&value[2..], 16).ok()?)?.to_string()
        }
        value if value.starts_with('#') => {
            char::from_u32(value[1..].parse::<u32>().ok()?)?.to_string()
        }
        _ => return None,
    };
    Some((decoded, end + 1))
}

fn parse_email(raw: &str) -> EmailParts {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let (headers, body) = normalized
        .split_once("\n\n")
        .unwrap_or((normalized.as_str(), ""));
    let mut subject_lines = Vec::new();
    let mut active_header = String::new();

    for line in headers.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if active_header.eq_ignore_ascii_case("subject") {
                subject_lines.push(line.trim().to_owned());
            }
            continue;
        }

        let Some((name, value)) = line.split_once(':') else {
            active_header.clear();
            continue;
        };
        active_header = name.trim().to_owned();
        if active_header.eq_ignore_ascii_case("subject") {
            subject_lines.push(value.trim().to_owned());
        }
    }

    EmailParts {
        subject: (!subject_lines.is_empty())
            .then(|| decode_rfc2047_words(&subject_lines.join(" "))),
        body: normalize_text_lines(body),
    }
}

fn decode_rfc2047_words(value: &str) -> String {
    // Keep this deliberately conservative: most benchmark and archive emails
    // carry plain ASCII/UTF-8 subjects, and undecodable words are safer intact.
    value.to_owned()
}

fn push_newline(output: &mut String) {
    while output.ends_with(' ') {
        output.pop();
    }
    if !output.ends_with("\n\n") {
        if output.ends_with('\n') {
            output.push('\n');
        } else if !output.is_empty() {
            output.push_str("\n\n");
        }
    }
}

fn normalize_text_lines(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            if !lines
                .last()
                .map(|line: &String| line.is_empty())
                .unwrap_or(true)
            {
                lines.push(String::new());
            }
        } else {
            lines.push(trimmed);
        }
    }
    while lines.last().map(|line| line.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines.join("\n")
}
