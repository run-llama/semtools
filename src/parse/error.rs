#[derive(Debug)]
pub enum JobError {
    HttpError(reqwest::Error),
    IoError(std::io::Error),
    TimeoutError,
    InvalidResponse(String),
    JoinError(tokio::task::JoinError),
    SerializationError(serde_json::Error),
    RetryExhausted(String),
}

impl From<reqwest::Error> for JobError {
    fn from(err: reqwest::Error) -> Self {
        JobError::HttpError(err)
    }
}

impl From<std::io::Error> for JobError {
    fn from(err: std::io::Error) -> Self {
        JobError::IoError(err)
    }
}

impl From<tokio::task::JoinError> for JobError {
    fn from(err: tokio::task::JoinError) -> Self {
        JobError::JoinError(err)
    }
}

impl From<serde_json::Error> for JobError {
    fn from(err: serde_json::Error) -> Self {
        JobError::SerializationError(err)
    }
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::HttpError(err) => write!(f, "HTTP error: {err}"),
            JobError::IoError(err) => write!(f, "IO error: {err}"),
            JobError::TimeoutError => write!(f, "Operation timed out"),
            JobError::InvalidResponse(msg) => write!(f, "Invalid response: {msg}"),
            JobError::JoinError(err) => write!(f, "Task join error: {err}"),
            JobError::SerializationError(err) => write!(f, "Serialization error: {err}"),
            JobError::RetryExhausted(msg) => write!(f, "Retry attempts exhausted: {msg}"),
        }
    }
}

impl std::error::Error for JobError {} 