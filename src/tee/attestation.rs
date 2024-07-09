use super::alicloud::AliCloud;
use super::gcp::GCP;
use common::error::Error;
use common::tee::{provider::TEEProvider, TEEType};
use reqwest::header::HeaderMap;
pub struct AttestationAgent {
    provider: Box<dyn TEEProvider>,
}

impl AttestationAgent {
    pub async fn new() -> Result<AttestationAgent, Error> {
        let tee_type = get_current_tee_type().await?;
        let provider: Box<dyn TEEProvider> = match tee_type {
            TEEType::GCP => Box::new(GCP {}),
            TEEType::AliCloud => Box::new(AliCloud{}),
            TEEType::Unkown => return Err(Error::UnkownTEETypeERROR {
                message: format!("Unkown TEE provider"),
            }),
        };
        Ok(AttestationAgent { provider })
    }

    pub async fn get_attestation_report(&self, nonce: String) -> Result<String, Error> {
        self.provider.get_report(nonce).await
    }

    pub fn get_tee_type(&self) -> Result<i32, Error> {
        Ok(self.provider.get_type()?.into())
    }
}

pub async fn get_current_tee_type() -> Result<TEEType, Error> {
    // Try GCP
    {
        let mut headers = HeaderMap::new();
        headers.insert("Metadata-Flavor", "Google".parse().unwrap());
        let client = reqwest::Client::new();
        let response = client
            .get("http://metadata.google.internal/computeMetadata/v1/instance/")
            .headers(headers)
            .send()
            .await
            .map_err(|e| Error::UnkownTEETypeERROR {
                message: e.to_string(),
            })?;
        if response.status().is_success() {
            return Ok(TEEType::GCP);
        }
    }

    return Err(Error::UnkownTEETypeERROR {
        message: "unkown tee type".to_string(),
    });
}
