use crate::error::Error;
use crate::tee::TEEType;
use async_trait::async_trait;

#[async_trait]
pub trait TEEVerifier: Send + Sync {
    async fn verify(&self, report: String, skip_verify_image_digest: bool) -> Result<bool, Error>;
    fn get_type(&self) -> Result<TEEType, Error>;
}
