use thiserror::Error;

pub type Result<T> = std::result::Result<T, DonglerError>;

#[derive(Debug, Error)]
pub enum DonglerError {
    #[error("unsupported or unknown document format for path: {path}")]
    UnknownFormat { path: String },

    #[error("{format} extraction is planned but not implemented yet")]
    PlannedFormat { format: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("PDF error: {0}")]
    Pdf(String),

    #[error("image error: {0}")]
    Image(String),

    #[error("archive error: {0}")]
    Archive(String),
}

impl DonglerError {
    pub fn planned_format(format: impl Into<String>) -> Self {
        Self::PlannedFormat {
            format: format.into(),
        }
    }

    pub fn pdf(message: impl Into<String>) -> Self {
        Self::Pdf(message.into())
    }

    pub fn image(message: impl Into<String>) -> Self {
        Self::Image(message.into())
    }

    pub fn archive(message: impl Into<String>) -> Self {
        Self::Archive(message.into())
    }
}
