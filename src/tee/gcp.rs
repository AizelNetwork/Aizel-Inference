use common::error::{AttestationError, Error};
use common::tee::{provider::TEEProvider, TEEType};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1::handshake;
use hyper_util::rt::TokioIo;
use serde::Serialize;
use sha256::digest;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use tokio::net::UnixStream;
use tokio::spawn;
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
    let stream = match UnixStream::connect(Path::new(CONTAINER_LAUNCHER_SOCKET)).await {
        Err(e) => {
            return Err(Error::AttestationError {
                teetype: TEEType::GCP,
                error: AttestationError::ReportError {
                    message: format!("failed to connect to socket request {}", e.to_string()),
                },
            })
        }
        Ok(stream) => TokioIo::new(stream),
    };
    let (mut sender, _connection) = match handshake(stream).await {
        Err(e) => {
            return Err(Error::AttestationError {
                teetype: TEEType::GCP,
                error: AttestationError::ReportError {
                    message: format!(
                        "failed to connect hand shake with socket request {}",
                        e.to_string()
                    ),
                },
            })
        }
        Ok((sender, connection)) => (sender, spawn(async move { connection.await })),
    };

    let req = hyper::Request::builder()
        .method("POST")
        .uri("/v1/token")
        .header("Host", "localhost")
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(custom_json)))
        .unwrap();
    let resp: hyper::Response<hyper::body::Incoming> =
        sender
            .send_request(req)
            .await
            .map_err(|e| Error::AttestationError {
                teetype: TEEType::GCP,
                error: AttestationError::ReportError {
                    message: format!("failed to send request {}", e.to_string()),
                },
            })?;
    let token: Vec<u8> = resp
        .collect()
        .await
        .map_err(|e| Error::AttestationError {
            teetype: TEEType::GCP,
            error: AttestationError::ReportError {
                message: format!("failed to read resp {}", e.to_string()),
            },
        })?
        .to_bytes()
        .to_vec();
    // let _ = connection.await;
    Ok(String::from_utf8(token).unwrap())
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
