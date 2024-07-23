use common::error::{AttestationError, Error};
use common::tee::{provider::TEEProvider, TEEType};
use std::future::Future;
use std::pin::Pin;
use sha256::digest;
use log::{error, info};
#[derive(Debug)]
pub struct AliCloud {}

async fn internal_get_report(nonce: String) -> Result<String, Error> {
    let mut d: Vec<u8> = hex::decode(digest(&nonce)).unwrap();
    d.resize(64, 0);
    let report_data = tdx_attest_rs::tdx_report_data_t {
        d: d.try_into().unwrap()
    };
    let mut tdx_report = tdx_attest_rs::tdx_report_t { d: [0; 1024usize] };
    let result = tdx_attest_rs::tdx_att_get_report(Some(&report_data), &mut tdx_report);
    if result != tdx_attest_rs::tdx_attest_error_t::TDX_ATTEST_SUCCESS {
        error!("failed to get the report.");
        return Err(Error::AttestationError { teetype: TEEType::AliCloud, error: AttestationError::ReportError {
            message: format!("failed to get tdx report "),
        } });
    }
    let mut selected_att_key_id = tdx_attest_rs::tdx_uuid_t { d: [0; 16usize] };
    let (result, quote) = tdx_attest_rs::tdx_att_get_quote(
        Some(&report_data),
        None,
        Some(&mut selected_att_key_id),
        0,
    );
    if result != tdx_attest_rs::tdx_attest_error_t::TDX_ATTEST_SUCCESS {
        error!("failed to get the quote.");
        return Err(Error::AttestationError { teetype: TEEType::AliCloud, error: AttestationError::ReportError {
            message: format!("failed to get tdx quote "),
        } });
    }
    match quote {
        Some(q) => {
            info!("Successfully get the TD Quote.");
            Ok(hex::encode(&q))
        }
        None => {
            error!("failed to get the quote.");
            Err(Error::AttestationError { teetype: TEEType::AliCloud, error: AttestationError::ReportError {
                message: format!("failed to get tdx quote "),
            } })
        }
    }
}

impl TEEProvider for AliCloud {

    fn get_report(
        &self,
        nonce: String
    ) -> Pin<
    Box<
        (dyn Future<Output = std::result::Result<std::string::String, common::error::Error>>
             + Send
             + 'static),
    >,
    > {
        Box::pin(internal_get_report(nonce))
    }

    fn get_type(&self) -> Result<TEEType, Error> {
        Ok(TEEType::AliCloud)
    }
}
