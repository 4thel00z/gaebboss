//! GAEB DA XML in pure Rust: tokenizer, generated typed model, writers,
//! bill-of-quantities navigation and bid construction.

pub mod bytes;
pub mod decimal;
pub mod document;
pub mod error;
pub mod generated;
pub mod lexer;
pub mod name;
pub mod read;
pub mod span;
pub mod tape;
pub mod text;
pub mod version;
pub mod warning;

/// The typed GAEB model generated from the schemas.
pub mod model {
    pub use crate::generated::model::*;
}

pub use document::Document;
pub use error::{Error, Result};
