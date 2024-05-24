use common::error::{AttestationError, Error};
use common::tee::{provider::TEEProvider, TEEType};
use std::fs;
const CONTAINER_RUNTIME_MOUNT_PATH: &'static str = "/run/container_launcher/";
const ATTESTATION_VERIFIER_TOKEN_FILENAME: &'static str = "attestation_verifier_claims_token";

#[derive(Debug)]
pub struct GCP {}

impl TEEProvider for GCP {
    fn get_report(&self) -> Result<String, Error> {
        let gcp_report = fs::read_to_string(format!(
            "{}{}",
            CONTAINER_RUNTIME_MOUNT_PATH, ATTESTATION_VERIFIER_TOKEN_FILENAME
        ))
        .map_err(|e| Error::AttestationError {
            teetype: TEEType::GCP,
            error: AttestationError::ReportError {
                message: e.to_string(),
            },
        })?;

        Ok(gcp_report)
    }

    fn get_type(&self) -> Result<TEEType, Error> {
        Ok(TEEType::GCP)
    }
}
