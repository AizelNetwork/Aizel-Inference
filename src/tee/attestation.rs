use super::gcp::GCP;
use common::error::Error;
use common::tee::{provider::TEEProvider, TEEType};

pub struct AttestationAgent {
    provider: Box<dyn TEEProvider>,
}

impl AttestationAgent {
    pub fn new() -> Result<AttestationAgent, Error> {
        let tee_type = get_current_tee_type()?;
        let provider = match tee_type {
            TEEType::GCP => Ok(Box::new(GCP {})),
            TEEType::Unkown => Err(Error::UnkownTEETypeERROR {
                message: format!("Unkown TEE provider"),
            }),
        }?;
        Ok(AttestationAgent { provider })
    }

    pub fn get_attestation_report(&self) -> Result<String, Error> {
        self.provider.get_report()
    }

    pub fn get_tee_type(&self) -> Result<i32, Error> {
        Ok(self.provider.get_type()?.into())
    }
}

pub fn get_current_tee_type() -> Result<TEEType, Error> {
    // TODO: support more tee types, demo stage only supports GCP
    return Ok(TEEType::GCP);
}
