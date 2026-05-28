use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;

use crate::engine::{split_paragraphs, text_document_from_paragraphs, ExtractionEngine};
use crate::error::{DonglerError, Result};
use crate::ir::Document;
use crate::openxml::read_zip_files;
use crate::source::Source;
use crate::textual::html_to_text;
use serde_json::Value;

#[derive(Debug, Default, Clone, Copy)]
pub struct ArchiveEngine;

#[derive(Debug)]
struct TarEntry<'a> {
    name: String,
    bytes: &'a [u8],
}

impl ExtractionEngine for ArchiveEngine {
    fn name(&self) -> &'static str {
        "archive-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        let bytes = source.bytes.as_deref().unwrap_or(source.content.as_bytes());
        if source.path.as_deref().map(is_zip_path).unwrap_or(false)
            || bytes.starts_with(b"PK\x03\x04")
        {
            return extract_zip_archive(source, self.name(), bytes);
        }

        let archive_bytes = decode_archive_bytes(bytes, source.path.as_deref())?;
        let entries = match read_tar_entries(&archive_bytes) {
            Ok(entries) if !entries.is_empty() => entries,
            Ok(entries) => {
                if let Some(text) = single_file_text_payload(&archive_bytes) {
                    let paragraphs = split_paragraphs(&text);
                    return text_document_from_paragraphs(source, self.name(), paragraphs, None);
                }
                entries
            }
            Err(error) => {
                if let Some(text) = single_file_text_payload(&archive_bytes) {
                    let paragraphs = split_paragraphs(&text);
                    return text_document_from_paragraphs(source, self.name(), paragraphs, None);
                }
                return Err(error);
            }
        };
        let mut paragraphs = Vec::new();

        for entry in entries {
            if !is_supported_archive_text_entry(&entry.name) {
                continue;
            }
            let text = String::from_utf8_lossy(entry.bytes);
            let normalized = archive_entry_text(&entry.name, &text);
            paragraphs.extend(split_paragraphs(&normalized));
        }

        text_document_from_paragraphs(source, self.name(), paragraphs, None)
    }
}

fn single_file_text_payload(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn extract_zip_archive(source: &Source, engine_name: &str, bytes: &[u8]) -> Result<Document> {
    let mut files = read_zip_files(bytes)?.into_iter().collect::<Vec<_>>();
    files.sort_by(|left, right| {
        archive_entry_sort_key(&left.0).cmp(&archive_entry_sort_key(&right.0))
    });
    let mut paragraphs = Vec::new();

    for (name, text) in files {
        if !is_supported_archive_text_entry(&name) {
            continue;
        }
        let normalized = archive_entry_text(&name, &text);
        paragraphs.extend(split_paragraphs(&normalized));
    }

    text_document_from_paragraphs(source, engine_name, paragraphs, None)
}

fn archive_entry_sort_key(name: &str) -> (u8, String) {
    let priority = if is_json_entry(name) {
        0
    } else if is_xml_entry(name) {
        1
    } else {
        2
    };
    (priority, name.to_owned())
}

fn archive_entry_text(name: &str, text: &str) -> String {
    if is_xml_entry(name) {
        html_to_text(text)
    } else if is_json_entry(name) {
        json_text(text)
    } else {
        text.to_owned()
    }
}

fn json_text(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => {
            let mut parts = Vec::new();
            collect_json_text(&value, &mut parts);
            parts.join("\n\n")
        }
        Err(_) => {
            let mut parts = Vec::new();
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                    collect_json_text(&value, &mut parts);
                }
            }
            if parts.is_empty() {
                text.to_owned()
            } else {
                parts.join("\n\n")
            }
        }
    }
}

fn collect_json_text(value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            let before = parts.len();
            for key in ["title", "abstract", "body_text", "text", "content"] {
                if let Some(child) = object.get(key) {
                    collect_json_text(child, parts);
                }
            }
            if parts.len() != before {
                return;
            }
            for child in object.values() {
                collect_json_text(child, parts);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_json_text(item, parts);
            }
        }
        Value::String(text) => {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !text.is_empty() {
                parts.push(text);
            }
        }
        _ => {}
    }
}

fn decode_archive_bytes(bytes: &[u8], path: Option<&str>) -> Result<Vec<u8>> {
    if path.map(is_gzip_path).unwrap_or(false) || bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes);
        let mut decoded = Vec::new();
        decoder
            .read_to_end(&mut decoded)
            .map_err(|error| DonglerError::archive(format!("gzip decode failed: {error}")))?;
        Ok(decoded)
    } else {
        Ok(bytes.to_vec())
    }
}

fn read_tar_entries(bytes: &[u8]) -> Result<Vec<TarEntry<'_>>> {
    let mut entries = Vec::new();
    let mut pos = 0usize;

    while pos + 512 <= bytes.len() {
        let header = &bytes[pos..pos + 512];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }

        let name = tar_name(header)?;
        let size = tar_octal(&header[124..136])? as usize;
        let typeflag = header[156];
        let data_start = pos + 512;
        let data_end = data_start + size;
        if data_end > bytes.len() {
            return Err(DonglerError::archive("truncated TAR entry data"));
        }
        if typeflag == 0 || typeflag == b'0' {
            entries.push(TarEntry {
                name,
                bytes: &bytes[data_start..data_end],
            });
        }
        pos = data_start + size.div_ceil(512) * 512;
    }

    Ok(entries)
}

fn tar_name(header: &[u8]) -> Result<String> {
    let name = trim_nul_string(&header[0..100]);
    let prefix = trim_nul_string(&header[345..500]);
    let full_name = if prefix.is_empty() {
        name
    } else {
        format!("{prefix}/{name}")
    };
    if full_name.is_empty() {
        Err(DonglerError::archive("TAR entry missing name"))
    } else {
        Ok(full_name)
    }
}

fn tar_octal(bytes: &[u8]) -> Result<u64> {
    let text = trim_nul_string(bytes);
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| DonglerError::archive("invalid TAR octal field"))
}

fn trim_nul_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

fn is_supported_archive_text_entry(name: &str) -> bool {
    matches!(
        logical_extension(name).as_deref(),
        Some(
            "txt"
                | "text"
                | "md"
                | "markdown"
                | "tex"
                | "latex"
                | "ltx"
                | "xml"
                | "nxml"
                | "tei"
                | "html"
                | "htm"
                | "json"
                | "jsonl"
                | "ndjson"
                | "csv"
                | "tsv"
        )
    )
}

fn is_xml_entry(name: &str) -> bool {
    matches!(
        logical_extension(name).as_deref(),
        Some("xml" | "nxml" | "tei" | "html" | "htm")
    )
}

fn is_json_entry(name: &str) -> bool {
    matches!(
        logical_extension(name).as_deref(),
        Some("json" | "jsonl" | "ndjson")
    )
}

fn logical_extension(path: &str) -> Option<String> {
    let path = Path::new(path);
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if extension != "gz" {
        return Some(extension);
    }
    Path::new(path.file_stem()?.to_str()?)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn is_gzip_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".gz") || path.to_ascii_lowercase().ends_with(".tgz")
}

fn is_zip_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".zip")
}
