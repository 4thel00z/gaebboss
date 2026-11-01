//! Input normalization: byte order marks, UTF-16 and single-byte encodings
//! become UTF-8 once, and the format family is sniffed from the first bytes.

use std::borrow::Cow;

use crate::error::{Error, Result};
use crate::warning::{Warning, WarningCode};

/// The format a file belongs to, judged from its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// GAEB DA XML 3.x.
    DaXml,
    /// GAEB DA XML before 3.0.
    DaXmlLegacy,
    /// GAEB 90 fixed-width records (`.d8x`).
    Gaeb90,
    /// GAEB 2000 bracket records (`.p8x`).
    Gaeb2000,
    /// ÖNORM A 2063 / B 2063 (Austria).
    Onorm,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Latin1,
    Windows1252,
}

pub struct Normalized<'a> {
    pub bytes: Cow<'a, [u8]>,
    pub encoding: Encoding,
    pub warnings: Vec<Warning>,
}

/// UTF-8 bytes for any supported input encoding, transcoding only when needed.
pub fn normalize(input: &[u8]) -> Result<Normalized<'_>> {
    if let Some(rest) = input.strip_prefix(b"\xEF\xBB\xBF") {
        return Ok(repair_utf8(rest, Encoding::Utf8Bom));
    }
    if let Some(rest) = input.strip_prefix(b"\xFF\xFE") {
        return Ok(Normalized {
            bytes: Cow::Owned(utf16_to_utf8(rest, u16::from_le_bytes)),
            encoding: Encoding::Utf16Le,
            warnings: Vec::new(),
        });
    }
    if let Some(rest) = input.strip_prefix(b"\xFE\xFF") {
        return Ok(Normalized {
            bytes: Cow::Owned(utf16_to_utf8(rest, u16::from_be_bytes)),
            encoding: Encoding::Utf16Be,
            warnings: Vec::new(),
        });
    }
    if input.starts_with(b"<\0") {
        return Ok(Normalized {
            bytes: Cow::Owned(utf16_to_utf8(input, u16::from_le_bytes)),
            encoding: Encoding::Utf16Le,
            warnings: Vec::new(),
        });
    }
    if input.starts_with(b"\0<") {
        return Ok(Normalized {
            bytes: Cow::Owned(utf16_to_utf8(input, u16::from_be_bytes)),
            encoding: Encoding::Utf16Be,
            warnings: Vec::new(),
        });
    }
    match declared_encoding(input) {
        Some(Declared::Latin1) => Ok(Normalized {
            bytes: Cow::Owned(single_byte_to_utf8(input, latin1)),
            encoding: Encoding::Latin1,
            warnings: Vec::new(),
        }),
        Some(Declared::Windows1252) => Ok(Normalized {
            bytes: Cow::Owned(single_byte_to_utf8(input, windows1252)),
            encoding: Encoding::Windows1252,
            warnings: Vec::new(),
        }),
        Some(Declared::Unsupported(name)) => {
            Err(Error::Encoding(format!("unsupported encoding {name}")))
        }
        Some(Declared::Utf8) | Some(Declared::Utf16) | None => {
            Ok(repair_utf8(input, Encoding::Utf8))
        }
    }
}

enum Declared {
    Utf8,
    Utf16,
    Latin1,
    Windows1252,
    Unsupported(String),
}

fn declared_encoding(input: &[u8]) -> Option<Declared> {
    let head = &input[..input.len().min(256)];
    let close = memchr::memchr(b'>', head)?;
    let decl = &head[..close];
    if !decl.starts_with(b"<?xml") {
        return None;
    }
    let at = memchr::memmem::find(decl, b"encoding")?;
    let rest = &decl[at + b"encoding".len()..];
    let quote = rest.iter().position(|b| *b == b'"' || *b == b'\'')?;
    let value = &rest[quote + 1..];
    let end = value.iter().position(|b| *b == rest[quote])?;
    let name = String::from_utf8_lossy(&value[..end]).to_ascii_lowercase();
    Some(match name.as_str() {
        "utf-8" | "utf8" => Declared::Utf8,
        "utf-16" | "utf-16le" | "utf-16be" => Declared::Utf16,
        "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" | "iso-8859-15" => Declared::Latin1,
        "windows-1252" | "cp1252" | "windows1252" => Declared::Windows1252,
        other => Declared::Unsupported(other.to_string()),
    })
}

/// Valid UTF-8 is borrowed; stray single bytes are read as Latin-1 and reported.
fn repair_utf8(input: &[u8], encoding: Encoding) -> Normalized<'_> {
    let Err(first) = std::str::from_utf8(input) else {
        return Normalized {
            bytes: Cow::Borrowed(input),
            encoding,
            warnings: Vec::new(),
        };
    };
    let mut out = Vec::with_capacity(input.len() + 16);
    let mut rest = input;
    let mut offset = first.valid_up_to();
    let mut repaired = 0usize;
    loop {
        match std::str::from_utf8(rest) {
            Ok(_) => {
                out.extend_from_slice(rest);
                break;
            }
            Err(e) => {
                let good = e.valid_up_to();
                out.extend_from_slice(&rest[..good]);
                let bad_len = e.error_len().unwrap_or(rest.len() - good);
                for b in &rest[good..good + bad_len] {
                    push_char(&mut out, latin1(*b));
                }
                repaired += bad_len;
                rest = &rest[good + bad_len..];
            }
        }
    }
    let message = format!("{repaired} byte(s) were not UTF-8 and were read as ISO-8859-1");
    offset = offset.min(u32::MAX as usize);
    Normalized {
        bytes: Cow::Owned(out),
        encoding,
        warnings: vec![Warning::new(WarningCode::EncodingRepaired, offset, message)],
    }
}

fn utf16_to_utf8(input: &[u8], unit: fn([u8; 2]) -> u16) -> Vec<u8> {
    let units = input.as_chunks::<2>().0.iter().map(|c| unit(*c));
    let mut out = Vec::with_capacity(input.len());
    for c in char::decode_utf16(units) {
        push_char(&mut out, c.unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    out
}

fn single_byte_to_utf8(input: &[u8], table: fn(u8) -> char) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() + input.len() / 8);
    for b in input {
        match *b < 0x80 {
            true => out.push(*b),
            false => push_char(&mut out, table(*b)),
        }
    }
    out
}

fn push_char(out: &mut Vec<u8>, c: char) {
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

fn latin1(b: u8) -> char {
    char::from(b)
}

fn windows1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

/// The format family of raw input, from its first bytes only.
pub fn sniff_family(input: &[u8]) -> Family {
    let head = &input[..input.len().min(4096)];
    let text = match normalize(head) {
        Ok(n) => n.bytes.into_owned(),
        Err(_) => head.to_vec(),
    };
    let lower: Vec<u8> = text.iter().map(u8::to_ascii_lowercase).collect();
    if lower.starts_with(b"#begin[gaeb]") {
        return Family::Gaeb2000;
    }
    if contains(&lower, b"onorm")
        || contains(&lower, b"oenorm")
        || contains(&lower, b"a2063")
        || contains(&lower, b"b2063")
    {
        return Family::Onorm;
    }
    if contains(&lower, b"<?xml") || contains(&lower, b"<gaeb") {
        if legacy_version(&text) {
            return Family::DaXmlLegacy;
        }
        return Family::DaXml;
    }
    if looks_like_gaeb90(&text) {
        return Family::Gaeb90;
    }
    Family::Unknown
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    memchr::memmem::find(haystack, needle).is_some()
}

fn legacy_version(text: &[u8]) -> bool {
    let Some(at) = memchr::memmem::find(text, b"<Version>") else {
        return false;
    };
    let value = &text[at + b"<Version>".len()..];
    value.starts_with(b"1.") || value.starts_with(b"2.")
}

/// GAEB 90 records are 80 columns wide and start with a two-digit record type.
fn looks_like_gaeb90(text: &[u8]) -> bool {
    let mut lines = text
        .split(|b| *b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l));
    let Some(first) = lines.next() else {
        return false;
    };
    let typed = first.len() >= 2 && first[..2].iter().all(u8::is_ascii_digit);
    let wide = (76..=82).contains(&first.len());
    typed && wide
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::warning::WarningCode;
    use std::borrow::Cow;

    fn utf16le(text: &str) -> Vec<u8> {
        let mut out = vec![0xFF, 0xFE];
        out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        out
    }

    #[test]
    fn utf8_passes_through_borrowed() {
        let input = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><GAEB/>";
        let n = normalize(input).unwrap();
        assert!(matches!(n.bytes, Cow::Borrowed(_)));
        assert_eq!(n.encoding, Encoding::Utf8);
        assert_eq!(&*n.bytes, input);
        assert!(n.warnings.is_empty());
    }

    #[test]
    fn utf8_bom_is_stripped() {
        let input = b"\xEF\xBB\xBF<GAEB/>";
        let n = normalize(input).unwrap();
        assert_eq!(&*n.bytes, b"<GAEB/>");
        assert_eq!(n.encoding, Encoding::Utf8Bom);
    }

    #[test]
    fn utf16le_bom_is_transcoded() {
        let text = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><GAEB>ä</GAEB>";
        let input = utf16le(text);
        let n = normalize(&input).unwrap();
        assert_eq!(&*n.bytes, text.as_bytes());
        assert_eq!(n.encoding, Encoding::Utf16Le);
    }

    #[test]
    fn latin1_declared_is_transcoded() {
        let input = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><GAEB>\xE4</GAEB>";
        let n = normalize(input).unwrap();
        assert_eq!(n.encoding, Encoding::Latin1);
        assert_eq!(
            &*n.bytes,
            "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><GAEB>ä</GAEB>".as_bytes()
        );
    }

    #[test]
    fn windows1252_declared_maps_the_euro_sign() {
        let input = b"<?xml version='1.0' encoding='windows-1252'?><GAEB>\x80</GAEB>";
        let n = normalize(input).unwrap();
        assert_eq!(n.encoding, Encoding::Windows1252);
        assert!(std::str::from_utf8(&n.bytes).unwrap().contains('€'));
    }

    #[test]
    fn undeclared_invalid_utf8_is_repaired_with_warning() {
        let input = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><GAEB>\xE4 ok</GAEB>";
        let n = normalize(input).unwrap();
        assert_eq!(n.encoding, Encoding::Utf8);
        assert!(std::str::from_utf8(&n.bytes).unwrap().contains("ä ok"));
        assert_eq!(n.warnings.len(), 1);
        assert_eq!(n.warnings[0].code, WarningCode::EncodingRepaired);
    }

    #[test]
    fn gaeb90_sniffs_as_gaeb90() {
        let mut line = vec![b' '; 80];
        line[0] = b'0';
        line[1] = b'0';
        line[31] = b'8';
        line[32] = b'3';
        line.push(b'\n');
        line.extend_from_slice(b"01Leistungsverzeichnis");
        assert_eq!(sniff_family(&line), Family::Gaeb90);
    }

    #[test]
    fn gaeb2000_sniffs_as_gaeb2000() {
        assert_eq!(
            sniff_family(b"#begin[GAEB]\n  #begin[GAEBInfo]\n"),
            Family::Gaeb2000
        );
    }

    #[test]
    fn da_xml_sniffs_from_declaration_or_root() {
        assert_eq!(
            sniff_family(
                b"<?xml version=\"1.0\"?><GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\">"
            ),
            Family::DaXml
        );
        assert_eq!(sniff_family(b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/200407\"><GAEBInfo><Version>3.0</Version>"), Family::DaXml);
    }

    #[test]
    fn legacy_da_xml_version_sniffs_as_legacy() {
        let input = b"<?xml version=\"1.0\"?><GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/200201\"><GAEBInfo><Version>2.0</Version>";
        assert_eq!(sniff_family(input), Family::DaXmlLegacy);
    }

    #[test]
    fn onorm_sniffs_as_onorm() {
        assert_eq!(
            sniff_family(
                b"<?xml version=\"1.0\"?><onlv xmlns=\"http://www.oenorm.at/schema/A2063/2009\">"
            ),
            Family::Onorm
        );
    }

    #[test]
    fn garbage_is_unknown() {
        assert_eq!(sniff_family(b"hello world"), Family::Unknown);
        assert_eq!(sniff_family(b""), Family::Unknown);
    }
}
