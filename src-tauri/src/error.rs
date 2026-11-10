use serde::Serialize;

/// Error returned to the frontend, serialized as `{ kind, message }`.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AppError {
    /// No archive folder chosen yet: the UI shows the first-run screen.
    #[error("{message}")]
    NoLibrary { message: String },
    #[error("{message}")]
    Other { message: String },
}

impl AppError {
    pub fn other(msg: impl Into<String>) -> Self {
        AppError::Other { message: msg.into() }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        match e.downcast::<AppError>() {
            Ok(app) => app,
            Err(e) => AppError::other(format!("{e:#}")),
        }
    }
}

pub type CmdResult<T> = Result<T, AppError>;
