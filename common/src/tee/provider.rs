use crate::error::Error;
use crate::tee::TEEType;
use std::future::Future;
use std::pin::Pin;
pub trait TEEProvider {
    fn get_report(
        &self,
        nonce: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, Error>> + Send>>;
    fn get_type(&self) -> Result<TEEType, Error>;
}
