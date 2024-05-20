use crate::utils::error::AizelError as Error;
pub trait TEEProvider {
    fn get_report(&self) -> Result<String, Error>;
    fn get_type(&self) -> Result<TEEProviderType, Error>;
}

pub enum TEEProviderType {
    GCP,
    Unkown,
}
