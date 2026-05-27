use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::format::InputFormat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub content: String,
    pub format: String,
    pub path: Option<String>,
}

impl Source {
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            content: text.into(),
            format: InputFormat::Text.as_str().to_owned(),
            path: None,
        }
    }

    pub fn from_path(path: impl AsRef<Path>, format: impl Into<String>) -> Result<Self> {
        let path = path.as_ref();

        Ok(Self {
            content: fs::read_to_string(path)?,
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
        Source::from_path(path, InputFormat::Text.as_str())
    }
}
