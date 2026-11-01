//! Tolerated defects: everything the reader repaired or skipped, with the
//! byte offset it happened at.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningCode {
    EncodingRepaired,
    UnknownElement,
    UnknownAttribute,
    UnclosedElement,
    MismatchedEndTag,
    BadEntity,
    BadDecimal,
    BadInteger,
    BadEnum,
    MissingRequired,
    Cardinality,
    TotalsMismatch,
    OzOverflow,
    VersionGuessed,
    PhaseGuessed,
    PhaseDisagrees,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub code: WarningCode,
    pub offset: u32,
    pub message: String,
}

impl Warning {
    pub fn new(code: WarningCode, offset: usize, message: impl Into<String>) -> Warning {
        Warning {
            code,
            offset: offset.min(u32::MAX as usize) as u32,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} at byte {}: {}",
            self.code, self.offset, self.message
        )
    }
}
