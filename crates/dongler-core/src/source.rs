use std::fs;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;

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

    pub fn from_text_or_gzip_path(
        path: impl AsRef<Path>,
        format: impl Into<String>,
    ) -> Result<Self> {
        let path = path.as_ref();
        if !is_gzip_path(path) {
            return Self::from_path(path, format);
        }

        let bytes = fs::read(path)?;
        let mut decoder = GzDecoder::new(bytes.as_slice());
        let mut content = String::new();
        decoder.read_to_string(&mut content)?;

        Ok(Self {
            bytes: Some(bytes),
            content,
            format: format.into(),
            path: Some(path.display().to_string()),
        })
    }

    pub fn from_pdf_path(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_binary_path(path, InputFormat::Pdf.as_str())
    }

    pub fn from_binary_path(path: impl AsRef<Path>, format: impl Into<String>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path)?;

        Ok(Self {
            content: String::from_utf8_lossy(&bytes).into_owned(),
            bytes: Some(bytes),
            format: format.into(),
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
        Source::from_text_or_gzip_path(path, InputFormat::Text.as_str())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PdfSourceLoader;

impl SourceLoader for PdfSourceLoader {
    fn load(&self, path: &Path) -> Result<Source> {
        Source::from_pdf_path(path)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImageSourceLoader;

impl SourceLoader for ImageSourceLoader {
    fn load(&self, path: &Path) -> Result<Source> {
        Source::from_binary_path(path, InputFormat::Image.as_str())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FormatSourceLoader {
    format: InputFormat,
}

impl FormatSourceLoader {
    pub fn new(format: InputFormat) -> Self {
        Self { format }
    }
}

impl SourceLoader for FormatSourceLoader {
    fn load(&self, path: &Path) -> Result<Source> {
        match self.format {
            InputFormat::Text
            | InputFormat::Html
            | InputFormat::Email
            | InputFormat::Json
            | InputFormat::Csv
            | InputFormat::Xml => Source::from_text_or_gzip_path(path, self.format.as_str()),
            InputFormat::Pdf
            | InputFormat::Image
            | InputFormat::Word
            | InputFormat::Excel
            | InputFormat::Presentation
            | InputFormat::OpenDocument
            | InputFormat::Archive => Source::from_binary_path(path, self.format.as_str()),
            InputFormat::LegacyWord
            | InputFormat::LegacyExcel
            | InputFormat::LegacyPresentation
            | InputFormat::LegacyEmail => Source::from_binary_path(path, self.format.as_str()),
        }
    }
}

fn is_gzip_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("gz"))
        .unwrap_or(false)
}
