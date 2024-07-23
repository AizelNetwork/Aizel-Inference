use common::error::{Error, VerificationError};
use common::tee::{verifier::TEEVerifier, TEEType, TEEType::AliCloud};
use intel_tee_quote_verification_rs::*;
use intel_tee_quote_verification_sys as qvl_sys;
use log::{error, info, warn};
use std::mem;
use std::ptr;
use std::time::{Duration, SystemTime};

#[cfg(debug_assertions)]
const SGX_DEBUG_FLAG: i32 = 1;
#[cfg(not(debug_assertions))]
const SGX_DEBUG_FLAG: i32 = 0;

#[derive(Debug)]
pub struct AliCloudVerifier {}

impl AliCloudVerifier {}

impl TEEVerifier for AliCloudVerifier {
    async fn verify(&self, quote: String, skip_verify_image_digest: bool) -> Result<bool, Error> {
        let quote = hex::decode(quote).unwrap();
        ecdsa_quote_verification(&quote, false)?;
        return Ok(true);
    }

    fn get_type(&self) -> Result<TEEType, Error> {
        Ok(TEEType::AliCloud)
    }
}

/// Quote verification with QvE/QVL
///
/// # Param
/// - **quote**\
/// ECDSA quote buffer.
/// - **use_qve**\
/// Set quote verification mode.\
///     - If true, quote verification will be performed by Intel QvE.
///     - If false, quote verification will be performed by untrusted QVL.
///
fn ecdsa_quote_verification(quote: &[u8], use_qve: bool) -> Result<(), Error> {
    let mut collateral_expiration_status = 1u32;
    let mut quote_verification_result = sgx_ql_qv_result_t::SGX_QL_QV_RESULT_UNSPECIFIED;

    let mut supp_data: sgx_ql_qv_supplemental_t = Default::default();
    let mut supp_data_desc = tee_supp_data_descriptor_t {
        major_version: 0,
        data_size: 0,
        p_data: &mut supp_data as *mut sgx_ql_qv_supplemental_t as *mut u8,
    };

    if use_qve {
        return Err(Error::VerificationError {
            teetype: AliCloud,
            error: VerificationError::TDXVerificationError {
                message: "Don't support Quote verification enclave!".to_string(),
            },
        });
    } else {
        // Untrusted quote verification

        // call DCAP quote verify library to get supplemental latest version and data size
        // version is a combination of major_version and minor version
        // you can set the major version in 'supp_data.major_version' to get old version supplemental data
        // only support major_version 3 right now
        //
        match tee_get_supplemental_data_version_and_size(quote) {
            Ok((supp_ver, supp_size)) => {
                if supp_size == mem::size_of::<sgx_ql_qv_supplemental_t>() as u32 {
                    info!(
                        "tee_get_quote_supplemental_data_version_and_size successfully returned."
                    );
                    info!(
                        "latest supplemental data major version: {}, minor version: {}, size: {}",
                        u16::from_be_bytes(supp_ver.to_be_bytes()[..2].try_into().unwrap()),
                        u16::from_be_bytes(supp_ver.to_be_bytes()[2..].try_into().unwrap()),
                        supp_size,
                    );
                    supp_data_desc.data_size = supp_size;
                } else {
                    warn!("\tWarning: Quote supplemental data size is different between DCAP QVL and QvE, please make sure you installed DCAP QVL and QvE from same release.")
                }
            }
            Err(e) => {
                return Err(Error::VerificationError {
                    teetype: AliCloud,
                    error: VerificationError::TDXVerificationError {
                        message: format!(
                            "tee_get_quote_supplemental_data_size failed: {:#04x}",
                            e as u32
                        ),
                    },
                });
            }
        }

        // get collateral
        let collateral: Result<QuoteCollateral, quote3_error_t> = tee_qv_get_collateral(quote);
        match collateral {
            Ok(ref c) => info!("tee_qv_get_collateral successfully returned."),
            Err(e) => {
                return Err(Error::VerificationError {
                    teetype: AliCloud,
                    error: VerificationError::TDXVerificationError {
                        message: format!("tee_qv_get_collateral failed: {:#04x}", e as u32),
                    },
                });
            }
        };

        // set current time.
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs() as i64;

        let p_supplemental_data = match supp_data_desc.data_size {
            0 => None,
            _ => Some(&mut supp_data_desc),
        };

        // call DCAP quote verify library for quote verification
        // here you can choose 'trusted' or 'untrusted' quote verification by specifying parameter '&qve_report_info'
        // if '&qve_report_info' is NOT NULL, this API will call Intel QvE to verify quote
        // if '&qve_report_info' is NULL, this API will call 'untrusted quote verify lib' to verify quote, this mode doesn't rely on SGX capable system, but the results can not be cryptographically authenticated
        match tee_verify_quote(
            quote,
            collateral.ok().as_ref(),
            current_time,
            None,
            p_supplemental_data,
        ) {
            Ok((colla_exp_stat, qv_result)) => {
                collateral_expiration_status = colla_exp_stat;
                quote_verification_result = qv_result;
                info!("tee_verify_quote successfully returned.");
            }
            Err(e) => {
                return Err(Error::VerificationError {
                    teetype: AliCloud,
                    error: VerificationError::TDXVerificationError {
                        message: format!("tee_verify_quote failed: {:#04x}", e as u32),
                    },
                });
            }
        }

        // check verification result
        //
        match quote_verification_result {
            sgx_ql_qv_result_t::SGX_QL_QV_RESULT_OK => {
                // check verification collateral expiration status
                // this value should be considered in your own attestation/verification policy
                //
                if collateral_expiration_status == 0 {
                    info!("Verification completed successfully.");
                } else {
                    warn!("Verification completed, but collateral is out of date based on 'expiration_check_date' you provided.");
                }
            }
            sgx_ql_qv_result_t::SGX_QL_QV_RESULT_CONFIG_NEEDED
            | sgx_ql_qv_result_t::SGX_QL_QV_RESULT_OUT_OF_DATE
            | sgx_ql_qv_result_t::SGX_QL_QV_RESULT_OUT_OF_DATE_CONFIG_NEEDED
            | sgx_ql_qv_result_t::SGX_QL_QV_RESULT_SW_HARDENING_NEEDED
            | sgx_ql_qv_result_t::SGX_QL_QV_RESULT_CONFIG_AND_SW_HARDENING_NEEDED => {
                warn!(
                    "Verification completed with Non-terminal result: {:x}",
                    quote_verification_result as u32
                );
            }
            sgx_ql_qv_result_t::SGX_QL_QV_RESULT_INVALID_SIGNATURE
            | sgx_ql_qv_result_t::SGX_QL_QV_RESULT_REVOKED
            | sgx_ql_qv_result_t::SGX_QL_QV_RESULT_UNSPECIFIED
            | _ => {
                return Err(Error::VerificationError {
                    teetype: AliCloud,
                    error: VerificationError::TDXVerificationError {
                        message: format!(
                            "Verification completed with Terminal result: {:x}",
                            quote_verification_result as u32
                        ),
                    },
                });
            }
        }

        // check supplemental data if necessary
        //
        if supp_data_desc.data_size > 0 {
            // you can check supplemental data based on your own attestation/verification policy
            // here we only print supplemental data version for demo usage
            //
            let version_s = unsafe { supp_data.__bindgen_anon_1.__bindgen_anon_1 };
            info!(
                "Supplemental data Major Version: {}",
                version_s.major_version
            );
            info!(
                "\tInfo: Supplemental data Minor Version: {}",
                version_s.minor_version
            );

            // print SA list if exist, SA list is supported from version 3.1
            //
            if unsafe { supp_data.__bindgen_anon_1.version } > 3 {
                let sa_list = unsafe { std::ffi::CStr::from_ptr(supp_data.sa_list.as_ptr()) };
                if sa_list.to_bytes().len() > 0 {
                    info!("\tInfo: Advisory ID: {}", sa_list.to_str().unwrap());
                }
            }
        }
        Ok(())
    }
}
