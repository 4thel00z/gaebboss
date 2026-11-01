//! Hard errors: input that cannot be read as GAEB DA XML at all.

use crate::bytes::Family;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not XML")]
    NotXml,
    #[error("no GAEB root element")]
    NoGaebRoot,
    #[error("unsupported format family {0:?}")]
    UnsupportedFamily(Family),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("encoding: {0}")]
    Encoding(String),
}

pub type Result<T> = std::result::Result<T, Error>;
