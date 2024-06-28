use common::error::{AttestationError, Error};
use common::tee::{provider::TEEProvider, TEEType};
use hyper::{Body, Client};
use hyperlocal::{UnixConnector, Uri};
use serde::Serialize;
use sha256::digest;
use std::future::Future;
use std::pin::Pin;
const _CONTAINER_RUNTIME_MOUNT_PATH: &'static str = "/run/container_launcher/";
const _ATTESTATION_VERIFIER_TOKEN_FILENAME: &'static str = "attestation_verifier_claims_token";
const AIZEL_DEFAULT_AUDIENCE: &'static str = "http://aizel.com";
const CONTAINER_LAUNCHER_SOCKET: &'static str = "/run/container_launcher/teeserver.sock";
#[derive(Debug)]
pub struct GCP {}

#[derive(Serialize)]
struct CustomToken {
    audience: String,
    nonces: Vec<String>,
    token_type: String,
}

async fn internal_get_report(nonce: String) -> Result<String, Error> {
    let request = CustomToken {
        audience: AIZEL_DEFAULT_AUDIENCE.to_string(),
        nonces: vec![digest(&nonce)],
        token_type: "OIDC".to_string(),
    };
    let custom_json = serde_json::to_string(&request).unwrap();
    let connector = UnixConnector;
    let client: Client<UnixConnector, Body> = Client::builder().build(connector);
    let url = "http://localhost/v1/token";
    let req = hyper::Request::builder()
        .method("POST")
        .uri(Uri::new(CONTAINER_LAUNCHER_SOCKET, url))
        .body(Body::from(custom_json))
        .unwrap();
    let response = client
        .request(req)
        .await
        .map_err(|e| Error::AttestationError {
            teetype: TEEType::GCP,
            error: AttestationError::ReportError {
                message: format!("failed to send request {}", e.to_string()),
            },
        })?;

    Ok(String::from_utf8(
        hyper::body::to_bytes(response.into_body())
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap())
}

impl TEEProvider for GCP {
    fn get_report(
        &self,
        nonce: String,
    ) -> Pin<
        Box<
            (dyn Future<Output = std::result::Result<std::string::String, common::error::Error>>
                 + Send
                 + 'static),
        >,
    > {
        Box::pin(internal_get_report(nonce))

        // TODO: TO BE DELETED
        // Ok(String::from_utf8(hyper::body::to_bytes(response.into_body()).await.unwrap().to_vec()).unwrap())

        // let gcp_report = fs::read_to_string(format!(
        //     "{}{}",
        //     CONTAINER_RUNTIME_MOUNT_PATH, ATTESTATION_VERIFIER_TOKEN_FILENAME
        // ))
        // .map_err(|e| Error::AttestationError {
        //     teetype: TEEType::GCP,
        //     error: AttestationError::ReportError {
        //         message: e.to_string(),
        //     },
        // })?;

        // Ok(gcp_report)
    }

    fn get_type(&self) -> Result<TEEType, Error> {
        Ok(TEEType::GCP)
    }
}
