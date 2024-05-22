use super::gcp::GCP;
use super::provider::{TEEProvider, TEEProviderType};
use crate::utils::error::AizelError as Error;

pub struct Attestation {
    provider: Box<dyn TEEProvider>,
}

impl Attestation {
    pub fn new() -> Result<Attestation, Error> {
        let tee_type = get_current_tee_type()?;
        let provider = match tee_type {
            TEEProviderType::GCP => Ok(Box::new(GCP {})),
            TEEProviderType::Unkown => Err(Error::UnkownTEEProviderERROR {
                message: format!("Unkown TEE provider"),
            }),
        }?;
        Ok(Attestation { provider })
    }

    pub fn get_attestation_report(&self) -> Result<String, Error> {
        self.provider.get_report()
    }
}

pub fn get_current_tee_type() -> Result<TEEProviderType, Error> {
    // TODO: support more tee types, demo stage only supports GCP
    return Ok(TEEProviderType::GCP);
}
