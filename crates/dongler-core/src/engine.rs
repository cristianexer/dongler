use crate::error::Result;
use crate::ir::{Block, Document, Metadata, Page, TextBlock, SCHEMA_VERSION};
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
        let paragraphs = split_paragraphs(&source.content);
        let blocks = paragraphs
            .into_iter()
            .map(|text| {
                Block::Text(TextBlock {
                    text,
                    kind: "paragraph".to_owned(),
                    bbox: None,
                    lines: Vec::new(),
                    source_anchors: Vec::new(),
                    confidence: None,
                })
            })
            .collect::<Vec<_>>();

        Ok(Document {
            schema_version: SCHEMA_VERSION.to_owned(),
            metadata: Metadata {
                format: source.format.clone(),
                engine: self.name().to_owned(),
                source: source.path.clone(),
                title: None,
                character_count: source.content.chars().count(),
                word_count: source.content.split_whitespace().count(),
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
}

fn split_paragraphs(text: &str) -> Vec<String> {
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
