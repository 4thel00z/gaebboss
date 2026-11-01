//! A byte range into one of the document buffers: the input itself or the
//! side buffer that holds decoded text.

const DECODED: u32 = 1 << 31;

/// Largest input the span encoding addresses: 2 GiB.
pub const MAX_INPUT: usize = (DECODED - 1) as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Span {
    start: u32,
    len: u32,
}

impl Span {
    pub const EMPTY: Span = Span { start: 0, len: 0 };

    pub fn input(start: usize, end: usize) -> Span {
        Span {
            start: start as u32,
            len: end.saturating_sub(start) as u32,
        }
    }

    pub fn decoded(start: usize, end: usize) -> Span {
        Span {
            start: start as u32 | DECODED,
            len: end.saturating_sub(start) as u32,
        }
    }

    pub fn is_decoded(self) -> bool {
        self.start & DECODED != 0
    }

    pub fn start(self) -> usize {
        (self.start & !DECODED) as usize
    }

    pub fn end(self) -> usize {
        self.start() + self.len as usize
    }

    pub fn len(self) -> usize {
        self.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }

    /// The bytes behind the span, from whichever buffer it points into.
    pub fn resolve<'a>(self, input: &'a [u8], decoded: &'a [u8]) -> &'a [u8] {
        let buffer = match self.is_decoded() {
            true => decoded,
            false => input,
        };
        buffer.get(self.start()..self.end()).unwrap_or(&[])
    }
}
