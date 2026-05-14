use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("capture error: {0}")]
    Capture(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("config error: {0}")]
    Config(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
