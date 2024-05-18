use crate::utils::error::AizelError;
pub trait TEEProvider {
    fn get_report(&self) -> Result<String, AizelError>;
    fn get_type(&self) -> Result<TEEProviderType, AizelError>;
}

pub enum TEEProviderType {
    GCP,
    Unkown,
}
