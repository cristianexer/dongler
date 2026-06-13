use crate::engine::ExtractionEngine;
use crate::error::{DonglerError, Result};
use crate::ir::{
    Asset, BBox, Block, Confidence, Document, FigureBlock, ImageObject, Metadata, Page,
    SourceAnchor, Warning, SCHEMA_VERSION,
};
use crate::source::Source;

const EXTRACTION_METHOD: &str = "image_native";

#[derive(Debug, Default, Clone, Copy)]
pub struct ImageEngine;

#[derive(Debug, Clone, Copy)]
struct ImageInfo {
    width: u32,
    height: u32,
}

impl ExtractionEngine for ImageEngine {
    fn name(&self) -> &'static str {
        "image-native"
    }

    fn extract(&self, source: &Source) -> Result<Document> {
        let bytes = source.bytes.as_deref().unwrap_or(source.content.as_bytes());
        let info = image_info(bytes)
            .ok_or_else(|| DonglerError::image("unsupported or malformed image header"))?;
        let bbox = BBox {
            x: 0.0,
            y: 0.0,
            width: info.width as f32,
            height: info.height as f32,
        };
        let image = ImageObject {
            id: "image-1".to_owned(),
            object_id: None,
            bbox: Some(bbox),
            width: Some(info.width),
            height: Some(info.height),
        };
        let asset = Asset {
            id: image.id.clone(),
            kind: "image".to_owned(),
            object_id: None,
            bbox: Some(bbox),
            width: Some(info.width),
            height: Some(info.height),
        };
        let figure = Block::Figure(FigureBlock {
            alt_text: source
                .path
                .as_deref()
                .and_then(|path| std::path::Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            caption: None,
            bbox: Some(bbox),
            image_ref: Some(image.id.clone()),
            source_anchors: vec![SourceAnchor {
                page_number: 1,
                pdf_object_ids: Vec::new(),
                bbox: Some(bbox),
                extraction_method: EXTRACTION_METHOD.to_owned(),
            }],
            confidence: Some(Confidence {
                score: 0.9,
                calibrated: false,
            }), ..Default::default()
        });

        Ok(Document {
            schema_version: SCHEMA_VERSION.to_owned(),
            metadata: Metadata {
                format: source.format.clone(),
                engine: self.name().to_owned(),
                source: source.path.clone(),
                title: None,
                character_count: 0,
                word_count: 0,
                block_count: 1,
                file_size_bytes: Some(bytes.len() as u64),
                pdf_version: None,
                encrypted: false,
            },
            pages: vec![Page {
                number: 1,
                width: Some(info.width as f32),
                height: Some(info.height as f32),
                rotation: None,
                bbox: Some(bbox),
                blocks: vec![figure],
                images: vec![image],
                assets: vec![asset.clone()],
                warnings: Vec::new(), ..Default::default()
            }],
            assets: vec![asset],
            warnings: Vec::<Warning>::new(),
        })
    }
}

fn image_info(bytes: &[u8]) -> Option<ImageInfo> {
    parse_png(bytes)
        .or_else(|| parse_jpeg(bytes))
        .or_else(|| parse_gif(bytes))
        .or_else(|| parse_bmp(bytes))
        .or_else(|| parse_tiff(bytes))
        .or_else(|| parse_webp(bytes))
}

fn parse_png(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some(ImageInfo {
        width: u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        height: u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    })
}

fn parse_jpeg(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.len() < 4 || !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }

    let mut pos = 2;
    while pos + 4 <= bytes.len() {
        while pos < bytes.len() && bytes[pos] == 0xff {
            pos += 1;
        }
        if pos >= bytes.len() {
            return None;
        }

        let marker = bytes[pos];
        pos += 1;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if pos + 2 > bytes.len() {
            return None;
        }
        let segment_len = u16::from_be_bytes(bytes[pos..pos + 2].try_into().ok()?) as usize;
        if segment_len < 2 || pos + segment_len > bytes.len() {
            return None;
        }
        let data_start = pos + 2;
        if is_jpeg_sof(marker) && data_start + 5 <= bytes.len() {
            return Some(ImageInfo {
                height: u16::from_be_bytes(bytes[data_start + 1..data_start + 3].try_into().ok()?)
                    as u32,
                width: u16::from_be_bytes(bytes[data_start + 3..data_start + 5].try_into().ok()?)
                    as u32,
            });
        }
        pos += segment_len;
    }

    None
}

fn is_jpeg_sof(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn parse_gif(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.len() < 10 || !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return None;
    }
    Some(ImageInfo {
        width: u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
        height: u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
    })
}

fn parse_bmp(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.len() < 26 || !bytes.starts_with(b"BM") {
        return None;
    }
    Some(ImageInfo {
        width: i32::from_le_bytes(bytes[18..22].try_into().ok()?).unsigned_abs(),
        height: i32::from_le_bytes(bytes[22..26].try_into().ok()?).unsigned_abs(),
    })
}

fn parse_tiff(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.len() < 8 {
        return None;
    }
    let endian = TiffEndian::from_header(bytes)?;
    if endian.read_u16(&bytes[2..4])? != 42 {
        return None;
    }
    let ifd_offset = endian.read_u32(&bytes[4..8])? as usize;
    if ifd_offset + 2 > bytes.len() {
        return None;
    }

    let entry_count = endian.read_u16(&bytes[ifd_offset..ifd_offset + 2])? as usize;
    let mut width = None;
    let mut height = None;
    let mut entry_pos = ifd_offset + 2;
    for _ in 0..entry_count {
        if entry_pos + 12 > bytes.len() {
            return None;
        }
        let tag = endian.read_u16(&bytes[entry_pos..entry_pos + 2])?;
        let field_type = endian.read_u16(&bytes[entry_pos + 2..entry_pos + 4])?;
        let count = endian.read_u32(&bytes[entry_pos + 4..entry_pos + 8])?;
        let value = tiff_inline_value(
            endian,
            field_type,
            count,
            &bytes[entry_pos + 8..entry_pos + 12],
        )?;
        match tag {
            256 => width = Some(value),
            257 => height = Some(value),
            _ => {}
        }
        entry_pos += 12;
    }

    Some(ImageInfo {
        width: width?,
        height: height?,
    })
}

fn tiff_inline_value(endian: TiffEndian, field_type: u16, count: u32, bytes: &[u8]) -> Option<u32> {
    if count != 1 {
        return None;
    }
    match field_type {
        3 => endian.read_u16(&bytes[..2]).map(u32::from),
        4 => endian.read_u32(bytes),
        _ => None,
    }
}

fn parse_webp(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.len() < 30 || !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WEBP" {
        return None;
    }
    if &bytes[12..16] != b"VP8X" {
        return None;
    }

    Some(ImageInfo {
        width: 1 + read_u24_le(&bytes[24..27])?,
        height: 1 + read_u24_le(&bytes[27..30])?,
    })
}

#[derive(Debug, Clone, Copy)]
enum TiffEndian {
    Little,
    Big,
}

impl TiffEndian {
    fn from_header(bytes: &[u8]) -> Option<Self> {
        match bytes.get(..2)? {
            b"II" => Some(Self::Little),
            b"MM" => Some(Self::Big),
            _ => None,
        }
    }

    fn read_u16(self, bytes: &[u8]) -> Option<u16> {
        let bytes = bytes.get(..2)?;
        match self {
            Self::Little => Some(u16::from_le_bytes(bytes.try_into().ok()?)),
            Self::Big => Some(u16::from_be_bytes(bytes.try_into().ok()?)),
        }
    }

    fn read_u32(self, bytes: &[u8]) -> Option<u32> {
        let bytes = bytes.get(..4)?;
        match self {
            Self::Little => Some(u32::from_le_bytes(bytes.try_into().ok()?)),
            Self::Big => Some(u32::from_be_bytes(bytes.try_into().ok()?)),
        }
    }
}

fn read_u24_le(bytes: &[u8]) -> Option<u32> {
    Some(
        (bytes.first().copied()? as u32)
            | ((bytes.get(1).copied()? as u32) << 8)
            | ((bytes.get(2).copied()? as u32) << 16),
    )
}
