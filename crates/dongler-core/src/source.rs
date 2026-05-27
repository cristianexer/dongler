use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::format::InputFormat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub content: String,
    pub bytes: Option<Vec<u8>>,
    pub format: String,
    pub path: Option<String>,
}

impl Source {
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            content: text.into(),
            bytes: None,
            format: InputFormat::Text.as_str().to_owned(),
            path: None,
        }
    }

    pub fn from_path(path: impl AsRef<Path>, format: impl Into<String>) -> Result<Self> {
        let path = path.as_ref();

        let content = fs::read_to_string(path)?;

        Ok(Self {
            bytes: Some(content.as_bytes().to_vec()),
            content,
            format: format.into(),
            path: Some(path.display().to_string()),
        })
    }

    pub fn from_pdf_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path)?;

        Ok(Self {
            content: String::from_utf8_lossy(&bytes).into_owned(),
            bytes: Some(bytes),
            format: InputFormat::Pdf.as_str().to_owned(),
            path: Some(path.display().to_string()),
        })
    }
}

pub trait SourceLoader {
    fn load(&self, path: &Path) -> Result<Source>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TextSourceLoader;

impl SourceLoader for TextSourceLoader {
    fn load(&self, path: &Path) -> Result<Source> {
        Source::from_path(path, InputFormat::Text.as_str())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PdfSourceLoader;

impl SourceLoader for PdfSourceLoader {
    fn load(&self, path: &Path) -> Result<Source> {
        Source::from_pdf_path(path)
    }
}
