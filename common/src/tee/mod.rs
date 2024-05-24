pub mod provider;
pub mod verifier;
use std::fmt;
#[derive(Debug)]
pub enum TEEType {
    GCP,
    Unkown,
}

impl fmt::Display for TEEType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TEEType::GCP => write!(f, "GCP"),
            TEEType::Unkown => write!(f, "Unkown"),
        }
    }
}
