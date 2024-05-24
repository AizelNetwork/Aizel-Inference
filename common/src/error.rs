use thiserror::Error;
use url::Url;
#[derive(Error, Debug)]
pub enum Error {
    #[error("NetworkError '{url}': {message}")]
    NetworkError { url: Url, message: String },
    #[error("SerDeError: {message}")]
    SerDeError { message: String },
    #[error("KidNotFoundError {kid}")]
    KidNotFoundError { kid: String },
    #[error("SigAlgMismatchError {algorithm}")]
    SigAlgMismatchError { algorithm: String },
    #[error("ValidateTokenError {msg}")]
    ValidateTokenError { msg: String },
    #[error("GoldenValueMismatchError {value} expect: {expect} get: {get}")]
    GoldenValueMismatchError {
        value: String,
        expect: String,
        get: String,
    },
}
