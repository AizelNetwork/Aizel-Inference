use aizel_inference::utils::error::AizelError;
use chrono::Local;
use env_logger::Env;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Header, Validation};
use log::{error, info};
use reqwest::{Client, Error};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use url::Url;
const EXPECTED_ISSUER: &str = "https://confidentialcomputing.googleapis.com";
const WELL_KNOWN_URL_PATH: &str = "/.well-known/openid-configuration";

const TOKEN_FILENAME: &str = "JWTtoken";

#[derive(Debug, Serialize, Deserialize)]
struct ContainerClaims {
    image_reference: String,
    image_digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Submods {
    container: ContainerClaims,
}
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    aud: String,
    iss: String,
    sub: String,
    exp: usize,
    submods: Submods,
    hwmodel: String,
    swname: String,
    swversion: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JsonWebKeySet {
    pub alg: String,
    pub kty: String,
    pub n: String,
    #[serde(rename = "use")]
    pub usage: String,
    pub kid: String,
    pub e: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct KeySets {
    keys: Vec<JsonWebKeySet>,
}

#[derive(Debug, Deserialize)]
struct OpenIDConfResponse {
    pub jwks_uri: String,
}

fn read_raw_token() -> Result<String, AizelError> {
    let token = fs::read_to_string(TOKEN_FILENAME).map_err(|e| AizelError::FileError {
        path: TOKEN_FILENAME.into(),
        message: e.to_string(),
    })?;
    return Ok(token);
}

async fn get_openid_configuration() -> Result<String, AizelError> {
    let url = format!("{}{}", EXPECTED_ISSUER, WELL_KNOWN_URL_PATH);
    let url = Url::parse(&url).unwrap();
    let client = Client::builder().build().unwrap();
    match client.get(url.clone()).send().await {
        Ok(res) => {
            let configuration: Result<OpenIDConfResponse, Error> =
                res.json::<OpenIDConfResponse>().await;
            match configuration {
                Ok(conf) => {
                    return Ok(conf.jwks_uri);
                }
                Err(e) => {
                    return Err(AizelError::SerDeError {
                        message: e.to_string(),
                    })
                }
            }
        }
        Err(e) => {
            error!("failed to send request: url {}, reason {}", url, e);
            return Err(AizelError::NetworkError {
                url: url,
                message: e.to_string(),
            });
        }
    }
}

async fn get_json_web_key_sets(url: String) -> Result<KeySets, AizelError> {
    let url = Url::parse(&url).unwrap();
    let client = Client::builder().build().unwrap();
    match client.get(url.clone()).send().await {
        Ok(res) => {
            let key_sets = res.json::<KeySets>().await;
            match key_sets {
                Ok(keys) => {
                    return Ok(keys);
                }
                Err(e) => {
                    return Err(AizelError::SerDeError {
                        message: e.to_string(),
                    })
                }
            }
        }
        Err(e) => {
            error!("failed to send request: url {}, reason {}", url, e);
            return Err(AizelError::NetworkError {
                url: url,
                message: e.to_string(),
            });
        }
    }
}

fn find_jwt_key_set(s: &KeySets, kid: String) -> Result<JsonWebKeySet, AizelError> {
    for k in &s.keys {
        if k.kid == kid {
            return Ok(k.clone());
        }
    }
    return Err(AizelError::KidNotFoundError { kid: kid });
}

fn init_log() {
    let _logger = env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let level = { buf.default_level_style(record.level()) };
            writeln!(
                buf,
                "{} {} [{}:{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                format_args!("{:>5}", level),
                record.module_path().unwrap_or("<unnamed>"),
                record.line().unwrap_or(0),
                &record.args()
            )
        })
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_log();
    let token: String = read_raw_token()?;
    info!("{}", token);
    let jwks_uri = get_openid_configuration().await?;
    info!("{}", jwks_uri);
    let key_sets: KeySets = get_json_web_key_sets(jwks_uri).await?;
    let header: Header = decode_header(&token).unwrap();
    assert_eq!(header.alg, Algorithm::RS256);
    info!("{:?}", header);
    let kid = header.kid.unwrap().clone();
    let key_set = find_jwt_key_set(&key_sets, kid)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&["https://sts.googleapis.com"]);
    validation.set_issuer(&["https://confidentialcomputing.googleapis.com"]);
    let claims = decode::<Claims>(
        &token,
        &DecodingKey::from_rsa_components(&key_set.n, &key_set.e).unwrap(),
        &validation,
    )
    .unwrap();
    info!("{:?}", claims);
    info!(
        "image reference {}",
        claims.claims.submods.container.image_reference
    );
    info!(
        "image digest {}",
        claims.claims.submods.container.image_digest
    );
    Ok(())
}
