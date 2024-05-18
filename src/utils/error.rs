use std::path::PathBuf;
use thiserror::Error;
#[derive(Error, Debug)]
pub enum AizelError {
    #[error("FileError '{path}': {message}")]
    FileError { path: PathBuf, message: String },
    #[error("AttestationReportError: {message}")]
    AttestationReportError { message: String },
}
