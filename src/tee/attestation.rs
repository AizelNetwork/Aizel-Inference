use super::gcp::GCP;
use super::provider::{TEEProvider, TEEProviderType};
use crate::utils::error::AizelError;
pub struct Attestation {
    provider: Box<dyn TEEProvider>,
}

impl Attestation {
    pub fn new() -> Result<Attestation, AizelError> {
        let tee_type = get_current_tee_type()?;
        let provider = match tee_type {
            TEEProviderType::GCP => Ok(Box::new(GCP {})),
            TEEProviderType::Unkown => Err(AizelError::AttestationReportError {
                message: format!("Unkown TEE provider"),
            }),
        }?;
        Ok(Attestation { provider })
    }

    pub fn get_attestation_report(&self) -> Result<String, AizelError> {
        self.provider.get_report()
    }
}

pub fn get_current_tee_type() -> Result<TEEProviderType, AizelError> {
    // TODO: support more tee types, demo stage only supports GCP
    return Ok(TEEProviderType::GCP);
}
