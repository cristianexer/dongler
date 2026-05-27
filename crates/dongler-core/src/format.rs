use std::fmt;
use std::path::Path;

use crate::error::{DonglerError, Result};

/// File formats Dongler can identify from a path.
///
/// V1 only extracts text files. The remaining variants are intentionally
/// modelled now so loaders and engines can be added without changing the
/// public format names returned by `detect_format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Text,
    Pdf,
    Excel,
    Word,
    Html,
    Image,
    Email,
}

impl InputFormat {
    pub fn detect_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .ok_or_else(|| DonglerError::UnknownFormat {
                path: path.display().to_string(),
            })?;

        match extension.as_str() {
            "txt" | "text" => Ok(Self::Text),
            "pdf" => Ok(Self::Pdf),
            "xls" | "xlsx" => Ok(Self::Excel),
            "doc" | "docx" => Ok(Self::Word),
            "html" | "htm" => Ok(Self::Html),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff" | "webp" => Ok(Self::Image),
            "eml" | "msg" => Ok(Self::Email),
            _ => Err(DonglerError::UnknownFormat {
                path: path.display().to_string(),
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Pdf => "pdf",
            Self::Excel => "excel",
            Self::Word => "word",
            Self::Html => "html",
            Self::Image => "image",
            Self::Email => "email",
        }
    }

    pub fn extraction_status(self) -> ExtractionStatus {
        match self {
            Self::Text | Self::Pdf => ExtractionStatus::Supported,
            Self::Excel | Self::Word | Self::Html | Self::Image | Self::Email => {
                ExtractionStatus::Planned
            }
        }
    }
}

impl fmt::Display for InputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionStatus {
    Supported,
    Planned,
}
