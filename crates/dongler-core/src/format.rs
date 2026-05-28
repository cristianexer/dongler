use std::fmt;
use std::path::Path;

use crate::error::{DonglerError, Result};

/// File formats Dongler can identify from a path.
///
/// File formats are modelled explicitly so loaders and engines can be added
/// without changing the public format names returned by `detect_format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Text,
    Pdf,
    Excel,
    LegacyExcel,
    Word,
    LegacyWord,
    Presentation,
    LegacyPresentation,
    OpenDocument,
    Archive,
    Html,
    Image,
    Email,
    Json,
    Csv,
    Xml,
    LegacyEmail,
}

impl InputFormat {
    pub fn detect_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let extension = effective_extension(path).ok_or_else(|| DonglerError::UnknownFormat {
            path: path.display().to_string(),
        })?;

        match extension.as_str() {
            "txt" | "text" | "md" | "markdown" => Ok(Self::Text),
            "pdf" => Ok(Self::Pdf),
            "xlsx" => Ok(Self::Excel),
            "xls" => Ok(Self::LegacyExcel),
            "docx" => Ok(Self::Word),
            "doc" => Ok(Self::LegacyWord),
            "pptx" => Ok(Self::Presentation),
            "ppt" => Ok(Self::LegacyPresentation),
            "odt" | "ods" | "odp" => Ok(Self::OpenDocument),
            "tar" | "tgz" | "gz" | "zip" => Ok(Self::Archive),
            "html" | "htm" => Ok(Self::Html),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff" | "webp" => Ok(Self::Image),
            "eml" => Ok(Self::Email),
            "json" | "jsonl" | "ndjson" => Ok(Self::Json),
            "csv" | "tsv" => Ok(Self::Csv),
            "xml" | "nxml" | "tei" => Ok(Self::Xml),
            "tex" | "latex" | "ltx" => Ok(Self::Text),
            "msg" => Ok(Self::LegacyEmail),
            _ => Err(DonglerError::UnknownFormat {
                path: path.display().to_string(),
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Pdf => "pdf",
            Self::Excel | Self::LegacyExcel => "excel",
            Self::Word | Self::LegacyWord => "word",
            Self::Presentation | Self::LegacyPresentation => "presentation",
            Self::OpenDocument => "opendocument",
            Self::Archive => "archive",
            Self::Html => "html",
            Self::Image => "image",
            Self::Email | Self::LegacyEmail => "email",
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Xml => "xml",
        }
    }

    pub fn extraction_status(self) -> ExtractionStatus {
        match self {
            Self::Text
            | Self::Pdf
            | Self::Excel
            | Self::Word
            | Self::Presentation
            | Self::OpenDocument
            | Self::Archive
            | Self::Html
            | Self::Image
            | Self::Email
            | Self::Json
            | Self::Csv
            | Self::Xml => ExtractionStatus::Supported,
            Self::LegacyExcel | Self::LegacyWord | Self::LegacyPresentation | Self::LegacyEmail => {
                ExtractionStatus::Planned
            }
        }
    }
}

fn effective_extension(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    if extension == "tgz" {
        return Some("tgz".to_owned());
    }
    if extension != "gz" {
        return Some(extension);
    }

    let stem = path.file_stem().and_then(|stem| stem.to_str())?;
    let inner_extension = Path::new(stem)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    match inner_extension.as_deref() {
        Some(
            "txt" | "text" | "md" | "markdown" | "json" | "jsonl" | "ndjson" | "csv" | "tsv"
            | "xml" | "nxml" | "tei" | "tex" | "latex" | "ltx" | "tar",
        ) => inner_extension,
        _ => Some("gz".to_owned()),
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
