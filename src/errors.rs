#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("ReqwestError: {0}")]
    ClientError(#[from] reqwest::Error),

    #[error("ReqwestError: {0}")]
    CantFindElement(&'static str),

    #[error("Invalid login credentials/session expired!")]
    InvalidCredentialsSessionExpired,

    #[error("Failed to parse PEM!")]
    FailedToParsePEM,

    #[error("Failed to generate key from PEM!")]
    FailedToGenerateKeyFromPEM,

    #[error("Failed to parse selector!")]
    FailedToParseSelector,

    #[error("Failed to parse url!")]
    FailedToParseUrl,

    #[error("Invalid gym!")]
    InvalidGym(String),

    #[error("Tokio file io error: {0}")]
    Io(#[from] std::io::Error)
}
