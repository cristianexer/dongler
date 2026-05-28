use std::collections::HashMap;
use std::io::Read;

use flate2::read::DeflateDecoder;

use crate::engine::{text_document_from_paragraphs, ExtractionEngine};
use crate::error::{DonglerError, Result};
use crate::ir::Document;
use crate::source::Source;
use crate::textual::html_to_text;

#[derive(Debug, Default, Clone, Copy)]
pub struct OpenXmlEngine;

#[derive(Debug)]
struct ZipEntry {
    name: String,
    compression_method: u16,
    compressed_size: usize,
    local_header_offset: usize,
}

impl ExtractionEngine for OpenXmlEngine {
    fn name(&self) -> &'static str {
        "openxml-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        let bytes = source.bytes.as_deref().unwrap_or(source.content.as_bytes());
        let files = read_zip_files(bytes)?;
        let paragraphs = match source.format.as_str() {
            "word" => extract_docx_paragraphs(&files)?,
            "excel" => extract_xlsx_rows(&files)?,
            "presentation" => extract_pptx_slide_text(&files)?,
            "opendocument" => extract_opendocument_text(&files)?,
            _ => Vec::new(),
        };

        text_document_from_paragraphs(source, self.name(), paragraphs, None)
    }
}

fn extract_pptx_slide_text(files: &HashMap<String, String>) -> Result<Vec<String>> {
    let mut slide_names = files
        .keys()
        .filter(|name| name.starts_with("ppt/slides/") && name.ends_with(".xml"))
        .cloned()
        .collect::<Vec<_>>();
    slide_names.sort_by_key(|name| slide_sort_key(name));
    if slide_names.is_empty() {
        return Err(DonglerError::archive("PPTX missing ppt/slides/*.xml"));
    }

    let mut paragraphs = Vec::new();
    for slide_name in slide_names {
        let Some(slide) = files.get(&slide_name) else {
            continue;
        };
        for paragraph_xml in tagged_ranges(slide, "a:p") {
            let text = xml_text_contents(paragraph_xml, "a:t")
                .into_iter()
                .collect::<Vec<_>>()
                .join("");
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !text.is_empty() {
                paragraphs.push(text);
            }
        }
    }

    Ok(paragraphs)
}

fn slide_sort_key(name: &str) -> usize {
    let file_name = name.rsplit('/').next().unwrap_or(name);
    let digits = file_name
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    digits.parse::<usize>().unwrap_or(usize::MAX)
}

pub(crate) fn read_zip_files(bytes: &[u8]) -> Result<HashMap<String, String>> {
    let entries = read_zip_entries(bytes)?;
    let mut files = HashMap::new();

    for entry in entries {
        let data = read_zip_entry(bytes, &entry)?;
        let text = String::from_utf8_lossy(&data).into_owned();
        files.insert(entry.name, text);
    }

    Ok(files)
}

fn read_zip_entries(bytes: &[u8]) -> Result<Vec<ZipEntry>> {
    let eocd = find_eocd(bytes).ok_or_else(|| DonglerError::archive("missing ZIP directory"))?;
    if eocd + 22 > bytes.len() {
        return Err(DonglerError::archive("truncated ZIP directory"));
    }

    let entry_count = read_u16_le(bytes, eocd + 10)? as usize;
    let central_size = read_u32_le(bytes, eocd + 12)? as usize;
    let central_offset = read_u32_le(bytes, eocd + 16)? as usize;
    if central_offset + central_size > bytes.len() {
        return Err(DonglerError::archive("ZIP directory exceeds file size"));
    }

    let mut entries = Vec::with_capacity(entry_count);
    let mut pos = central_offset;
    for _ in 0..entry_count {
        if pos + 46 > bytes.len() || read_u32_le(bytes, pos)? != 0x0201_4b50 {
            return Err(DonglerError::archive("malformed ZIP central header"));
        }

        let compression_method = read_u16_le(bytes, pos + 10)?;
        let compressed_size = read_u32_le(bytes, pos + 20)? as usize;
        let name_len = read_u16_le(bytes, pos + 28)? as usize;
        let extra_len = read_u16_le(bytes, pos + 30)? as usize;
        let comment_len = read_u16_le(bytes, pos + 32)? as usize;
        let local_header_offset = read_u32_le(bytes, pos + 42)? as usize;
        let name_start = pos + 46;
        let name_end = name_start + name_len;
        if name_end > bytes.len() {
            return Err(DonglerError::archive("truncated ZIP entry name"));
        }

        entries.push(ZipEntry {
            name: String::from_utf8_lossy(&bytes[name_start..name_end]).into_owned(),
            compression_method,
            compressed_size,
            local_header_offset,
        });
        pos = name_end + extra_len + comment_len;
    }

    Ok(entries)
}

fn read_zip_entry(bytes: &[u8], entry: &ZipEntry) -> Result<Vec<u8>> {
    let pos = entry.local_header_offset;
    if pos + 30 > bytes.len() || read_u32_le(bytes, pos)? != 0x0403_4b50 {
        return Err(DonglerError::archive("malformed ZIP local header"));
    }

    let name_len = read_u16_le(bytes, pos + 26)? as usize;
    let extra_len = read_u16_le(bytes, pos + 28)? as usize;
    let data_start = pos + 30 + name_len + extra_len;
    let data_end = data_start + entry.compressed_size;
    if data_end > bytes.len() {
        return Err(DonglerError::archive("truncated ZIP entry data"));
    }
    let data = &bytes[data_start..data_end];

    match entry.compression_method {
        0 => Ok(data.to_vec()),
        8 => {
            let mut decoder = DeflateDecoder::new(data);
            let mut decoded = Vec::new();
            decoder
                .read_to_end(&mut decoded)
                .map_err(|error| DonglerError::archive(format!("Deflate failed: {error}")))?;
            Ok(decoded)
        }
        method => Err(DonglerError::archive(format!(
            "unsupported ZIP compression method {method}"
        ))),
    }
}

fn extract_docx_paragraphs(files: &HashMap<String, String>) -> Result<Vec<String>> {
    let document = files
        .get("word/document.xml")
        .ok_or_else(|| DonglerError::archive("DOCX missing word/document.xml"))?;
    let mut paragraphs = Vec::new();

    for paragraph_xml in tagged_ranges(document, "w:p") {
        let mut text = xml_text_contents(paragraph_xml, "w:t").join("");
        if text.is_empty() {
            text = xml_text_contents(paragraph_xml, "t").join("");
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() {
            paragraphs.push(text);
        }
    }

    Ok(paragraphs)
}

fn extract_xlsx_rows(files: &HashMap<String, String>) -> Result<Vec<String>> {
    let shared_strings = files
        .get("xl/sharedStrings.xml")
        .map(|xml| {
            tagged_ranges(xml, "si")
                .into_iter()
                .map(|item| {
                    let text = xml_text_contents(item, "t").join("");
                    text.split_whitespace().collect::<Vec<_>>().join(" ")
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut rows = Vec::new();

    let mut sheet_names = files
        .keys()
        .filter(|name| name.starts_with("xl/worksheets/") && name.ends_with(".xml"))
        .cloned()
        .collect::<Vec<_>>();
    sheet_names.sort();

    for sheet_name in sheet_names {
        let Some(sheet) = files.get(&sheet_name) else {
            continue;
        };
        for row_xml in tagged_ranges(sheet, "row") {
            let cells = tagged_elements(row_xml, "c")
                .into_iter()
                .filter_map(|(tag, cell)| xlsx_cell_text(tag, cell, &shared_strings))
                .collect::<Vec<_>>();
            if !cells.is_empty() {
                rows.push(cells.join(" "));
            }
        }
    }

    Ok(rows)
}

fn xlsx_cell_text(cell_tag: &str, cell_xml: &str, shared_strings: &[String]) -> Option<String> {
    let value = xml_text_contents(cell_xml, "v").into_iter().next()?;
    if cell_tag.contains("t=\"s\"") || cell_tag.contains("t='s'") {
        let index = value.trim().parse::<usize>().ok()?;
        shared_strings.get(index).cloned()
    } else {
        Some(value.trim().to_owned())
    }
    .filter(|text| !text.is_empty())
}

fn extract_opendocument_text(files: &HashMap<String, String>) -> Result<Vec<String>> {
    let content = files
        .get("content.xml")
        .ok_or_else(|| DonglerError::archive("OpenDocument missing content.xml"))?;

    let rows = extract_opendocument_rows(content);
    if !rows.is_empty() {
        return Ok(rows);
    }

    Ok(extract_opendocument_paragraphs(content))
}

fn extract_opendocument_rows(content: &str) -> Vec<String> {
    tagged_ranges(content, "table:table-row")
        .into_iter()
        .filter_map(|row_xml| {
            let cells = tagged_ranges(row_xml, "table:table-cell")
                .into_iter()
                .filter_map(|cell_xml| {
                    let paragraphs = tagged_ranges(cell_xml, "text:p")
                        .into_iter()
                        .filter_map(clean_xml_text)
                        .collect::<Vec<_>>();
                    (!paragraphs.is_empty()).then(|| paragraphs.join(" "))
                })
                .collect::<Vec<_>>();
            (!cells.is_empty()).then(|| cells.join(" "))
        })
        .collect()
}

fn extract_opendocument_paragraphs(content: &str) -> Vec<String> {
    tagged_ranges(content, "text:p")
        .into_iter()
        .filter_map(clean_xml_text)
        .collect()
}

fn clean_xml_text(xml: &str) -> Option<String> {
    let text = html_to_text(&xml_unescape(xml))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then_some(text)
}

fn tagged_ranges<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let mut ranges = Vec::new();
    let mut pos = 0;
    let open_prefix = format!("<{tag}");
    let close = format!("</{tag}>");

    while let Some(relative_start) = xml[pos..].find(&open_prefix) {
        let start = pos + relative_start;
        let Some(open_end) = xml[start..].find('>') else {
            break;
        };
        let content_start = start + open_end + 1;
        let Some(relative_end) = xml[content_start..].find(&close) else {
            break;
        };
        let content_end = content_start + relative_end;
        ranges.push(&xml[content_start..content_end]);
        pos = content_end + close.len();
    }

    ranges
}

fn tagged_elements<'a>(xml: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let mut ranges = Vec::new();
    let mut pos = 0;
    let open_prefix = format!("<{tag}");
    let close = format!("</{tag}>");

    while let Some(relative_start) = xml[pos..].find(&open_prefix) {
        let start = pos + relative_start;
        let Some(open_end) = xml[start..].find('>') else {
            break;
        };
        let content_start = start + open_end + 1;
        let Some(relative_end) = xml[content_start..].find(&close) else {
            break;
        };
        let content_end = content_start + relative_end;
        ranges.push((&xml[start..content_start], &xml[content_start..content_end]));
        pos = content_end + close.len();
    }

    ranges
}

fn xml_text_contents(xml: &str, tag: &str) -> Vec<String> {
    tagged_ranges(xml, tag)
        .into_iter()
        .map(xml_unescape)
        .collect()
}

fn xml_unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    let max_comment = 65_535usize.min(bytes.len());
    let start = bytes.len().saturating_sub(22 + max_comment);
    (start..=bytes.len().saturating_sub(22))
        .rev()
        .find(|pos| bytes.get(*pos..*pos + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))
}

fn read_u16_le(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    let slice = bytes
        .get(pos..end)
        .ok_or_else(|| DonglerError::archive("unexpected end of ZIP data"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    let slice = bytes
        .get(pos..end)
        .ok_or_else(|| DonglerError::archive("unexpected end of ZIP data"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}
