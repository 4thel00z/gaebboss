//! A parsed GAEB file: the bytes, the tape over them, what version and
//! phase it speaks, the typed tree, and every warning raised on the way.

use std::path::Path;

use crate::bytes::{Family, normalize, sniff_family};
use crate::error::{Error, Result};
use crate::generated::model::GAEB;
use crate::generated::read::read_gaeb;
use crate::lexer::lex;
use crate::read::Reader;
use crate::span::Span;
use crate::tape::Tape;
use crate::version::{Phase, Route, Version, detect};
use crate::warning::Warning;

pub struct Document {
    bytes: Vec<u8>,
    tape: Tape,
    pub route: Route,
    pub gaeb: GAEB,
    pub warnings: Vec<Warning>,
}

impl Document {
    /// Parses a GAEB DA XML document from its bytes.
    pub fn parse(input: &[u8]) -> Result<Document> {
        Document::parse_named(input, None)
    }

    /// Parses with the file name at hand, which decides the phase when
    /// nothing inside the file does.
    pub fn parse_named(input: &[u8], file_name: Option<&str>) -> Result<Document> {
        match sniff_family(input) {
            Family::DaXml => {}
            other => return Err(Error::UnsupportedFamily(other)),
        }
        let normalized = normalize(input)?;
        let mut warnings = normalized.warnings;
        let bytes = normalized.bytes.into_owned();
        let mut tape = lex(&bytes)?;
        warnings.append(&mut tape.warnings);
        let mut route = detect(&tape, &bytes, file_name)?;
        warnings.append(&mut route.warnings);
        let mut reader = Reader::new(&tape, &bytes);
        let gaeb = read_gaeb(&mut reader, tape.root);
        warnings.append(&mut reader.warnings);
        Ok(Document {
            bytes,
            tape,
            route,
            gaeb,
            warnings,
        })
    }

    pub fn open(path: &Path) -> Result<Document> {
        let input = std::fs::read(path)?;
        let name = path.file_name().and_then(|n| n.to_str());
        Document::parse_named(&input, name)
    }

    /// The text behind a span of this document.
    pub fn str(&self, span: Span) -> &str {
        std::str::from_utf8(span.resolve(&self.bytes, &self.tape.decoded)).unwrap_or("")
    }

    /// Stores new text and returns a span for it, for fields set after parsing.
    pub fn intern(&mut self, text: &str) -> Span {
        let start = self.tape.decoded.len();
        self.tape.decoded.extend_from_slice(text.as_bytes());
        Span::decoded(start, self.tape.decoded.len())
    }

    pub fn tape(&self) -> &Tape {
        &self.tape
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn version(&self) -> Version {
        self.route.version
    }

    pub fn phase(&self) -> Option<Phase> {
        self.route.phase
    }
}
