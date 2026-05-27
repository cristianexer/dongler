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
}

impl DonglerError {
    pub fn planned_format(format: impl Into<String>) -> Self {
        Self::PlannedFormat {
            format: format.into(),
        }
    }
}
