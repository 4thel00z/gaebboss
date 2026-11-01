//! GAEB DA XML in pure Rust: tokenizer, generated typed model, writers,
//! bill-of-quantities navigation and bid construction.

pub mod bid;
pub mod boq;
pub mod bytes;
pub mod decimal;
pub mod document;
pub mod error;
pub mod generated;
pub mod lexer;
pub mod name;
pub mod oz;
pub mod read;
pub mod span;
pub mod tape;
pub mod text;
pub mod validate;
pub mod version;
pub mod warning;
pub mod write;

/// The typed GAEB model generated from the schemas.
pub mod model {
    pub use crate::generated::model::*;
}

pub use document::Document;
pub use error::{Error, Result};
