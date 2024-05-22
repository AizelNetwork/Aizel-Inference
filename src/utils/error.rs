use std::path::PathBuf;
use thiserror::Error;
use url::Url;
#[derive(Error, Debug)]
pub enum AizelError {
    #[error("FileError '{path}': {message}")]
    FileError { path: PathBuf, message: String },
    #[error("JWTTokenNotFoundError: {message}")]
    JWTTokenNotFoundError { message: String },
    #[error("UnkownTEEProviderERROR: {message}")]
    UnkownTEEProviderERROR { message: String },
    #[error("NetworkError '{url}': {message}")]
    NetworkError { url: Url, message: String },
    #[error("SerDeError: {message}")]
    SerDeError { message: String },
    #[error("KidNotFoundError {kid}")]
    KidNotFoundError { kid: String },
}
