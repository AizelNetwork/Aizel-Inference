use crate::chains::contract::Contract;
use crate::node::config::AIZEL_CONFIG;
use common::error::Error;
use lazy_static::lazy_static;
use log::{error, info};
use minio::s3::{
    args::{
        BucketExistsArgs, DownloadObjectArgs, GetObjectArgs, ObjectConditionalReadArgs,
        PutObjectApiArgs,
    },
    client::Client,
    creds::StaticProvider,
    error,
    http::BaseUrl,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

lazy_static! {
    static ref MINIO_CLIENT: MinioClient = MinioClient::new();
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct UserInput {
    pub user: String,
    pub input: String,
}

impl UserInput {
    pub fn to_string(&self) -> String {
        format!("{}-{}", self.user, self.input)
    }
}
#[derive(Debug)]
pub struct MinioClient {
    pub client: Client,
}

impl MinioClient {
    fn new() -> Self {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let data_node_url = rt
            .block_on(Contract::query_data_node_url(AIZEL_CONFIG.data_node_id))
            .unwrap()
            .parse::<BaseUrl>()
            .unwrap();
        let static_provider = StaticProvider::new(
            &AIZEL_CONFIG.minio_account,
            &AIZEL_CONFIG.minio_password,
            None,
        );
        let client =
            Client::new(data_node_url, Some(Box::new(static_provider)), None, None).unwrap();
        MinioClient { client }
    }

    pub fn get<'a>() -> &'a Self {
        &MINIO_CLIENT
    }

    pub async fn get_inputs(
        &self,
        bucket_name: &str,
        object_name: &str,
    ) -> Result<UserInput, Error> {
        // Check 'bucket_name' bucket exist or not.
        self.bucket_exists(bucket_name).await?;

        let resp = self
            .client
            .get_object_old(&GetObjectArgs::new(bucket_name, object_name).unwrap())
            .await
            .map_err(|e| Error::MinIOError {
                message: format!("failed to get object {}", e.to_string()),
            })?;
        // info!("{:?}", resp.text().await);
        let user_input = resp
            .json::<UserInput>()
            .await
            .map_err(|e| Error::MinIOError {
                message: format!("failed to parse response {}", e.to_string()),
            })?;
        Ok(user_input)
        // Ok(InputInfo::default())
    }

    pub async fn upload(
        &self,
        bucket_name: &str,
        object_name: &str,
        data: &[u8],
    ) -> Result<(), Error> {
        // Check 'bucket_name' bucket exist or not.
        self.bucket_exists(bucket_name).await?;

        match self
            .client
            .get_object_old(&ObjectConditionalReadArgs::new(bucket_name, object_name).unwrap())
            .await
        {
            Ok(_) => {
                info!("{bucket_name:?} {object_name:?} exists");
                Ok(())
            }
            Err(err) => match err {
                error::Error::S3Error(e) => {
                    if e.code != "NoSuchKey" {
                        error!("{bucket_name:?} {object_name:?} down failed {e:?}");
                        return Err(Error::MinIOError {
                            message: format!("failed to get object {}", e.message),
                        });
                    }

                    let object_args =
                        &PutObjectApiArgs::new(&bucket_name, &object_name, data).unwrap();

                    self.client.put_object_api(object_args).await.map_err(|e| {
                        Error::MinIOError {
                            message: format!("failed to put object {}", e.to_string()),
                        }
                    })?;
                    Ok(())
                }
                _ => {
                    error!("{bucket_name:?} {object_name:?} down failed {err:?}");
                    Err(Error::MinIOError {
                        message: format!("failed to put object"),
                    })
                }
            },
        }
    }

    pub async fn download_model(
        &self,
        bucket: &str,
        model: &str,
        path: &PathBuf,
    ) -> Result<(), Error> {
        self.bucket_exists(bucket).await?;
        let time_start = Instant::now();
        let args: DownloadObjectArgs =
            DownloadObjectArgs::new(bucket, model, path.to_str().unwrap()).unwrap();
        let _ =
            self.client
                .download_object(&args)
                .await
                .map_err(|e| Error::DownloadingModelError {
                    model: model.to_string(),
                    message: format!("failed to download model {}", e.to_string()),
                })?;
        let duration = time_start.elapsed();
        info!("downloading model time cost: {:?}", duration);
        Ok(())
    }

    async fn bucket_exists(&self, bucket_name: &str) -> Result<bool, Error> {
        // Check 'bucket_name' bucket exist or not.
        match self
            .client
            .bucket_exists(&BucketExistsArgs::new(bucket_name).unwrap())
            .await
        {
            Ok(exists) => {
                if !exists {
                    return Ok(false);
                }
                Ok(true)
            }
            Err(e) => Err(Error::MinIOError {
                message: format!("failed to get bucket {}", e.to_string()),
            }),
        }
    }
}

#[tokio::test]
async fn test_get_input() {
    env::set_var("DATA_ADDRESS", "http://35.197.133.226:9112");
    let client = MinioClient::get();
    let input = client
        .get_inputs(
            "users-output",
            "0x34ee5d5e212beedbd5f45ff352bffc82dd356ff03e830272a44ab976f4a421bc",
        )
        .await;
    println!("{:?}", input.unwrap());
}
