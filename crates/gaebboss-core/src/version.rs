//! Which GAEB DA XML version and exchange phase a file speaks, judged from
//! its namespace, `GAEBInfo/Version`, `Award/DP` and the file name.

use crate::error::Result;
pub use crate::generated::phase::Phase;
use crate::name::Name;
use crate::span::Span;
use crate::tape::Tape;
use crate::warning::{Warning, WarningCode};

const LEGACY_NAMESPACE: &str = "http://www.gaeb.de/GAEB_DA_XML/200407";
const NAMESPACE_PREFIX: &str = "http://www.gaeb.de/GAEB_DA_XML/DA";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Version {
    V30,
    V31,
    V32,
    V33,
}

impl Version {
    pub fn as_str(self) -> &'static str {
        match self {
            Version::V30 => "3.0",
            Version::V31 => "3.1",
            Version::V32 => "3.2",
            Version::V33 => "3.3",
        }
    }

    pub fn parse(text: &str) -> Option<Version> {
        match text.trim() {
            "3.0" => Some(Version::V30),
            "3.1" => Some(Version::V31),
            "3.2" => Some(Version::V32),
            "3.3" => Some(Version::V33),
            _ => None,
        }
    }
}

/// What detection decided and where it had to guess.
#[derive(Clone, Debug)]
pub struct Route {
    pub version: Version,
    pub declared_version: Option<Span>,
    pub phase: Option<Phase>,
    pub namespace: Span,
    pub warnings: Vec<Warning>,
}

/// Decides version and phase: the namespace family first, then
/// `GAEBInfo/Version`, then `Award/DP`, then the file extension.
pub fn detect(tape: &Tape, input: &[u8], file_name: Option<&str>) -> Result<Route> {
    let mut warnings = Vec::new();
    let namespace = tape.str(input, tape.default_ns);
    let root = tape.root;
    let declared_version = tape
        .child(root, Name::GAEBInfo)
        .and_then(|info| tape.child(info, Name::Version))
        .map(|v| tape.nodes[v as usize].text);
    let declared = declared_version.and_then(|span| Version::parse(tape.str(input, span)));
    let offset = tape.nodes[root as usize].open.start();
    let (version, namespace_phase) = match namespace_family(namespace) {
        NamespaceFamily::Legacy => (
            declared
                .filter(|v| matches!(v, Version::V30 | Version::V31))
                .unwrap_or_else(|| {
                    warnings.push(Warning::new(
                        WarningCode::VersionGuessed,
                        offset,
                        "namespace 200407 without a 3.0 or 3.1 version declaration; reading as 3.1",
                    ));
                    Version::V31
                }),
            None,
        ),
        NamespaceFamily::Phased { version, phase } => (
            version.or(declared).unwrap_or_else(|| {
                warnings.push(Warning::new(
                    WarningCode::VersionGuessed,
                    offset,
                    format!("namespace {namespace} names no known version; reading as 3.3"),
                ));
                Version::V33
            }),
            phase,
        ),
        NamespaceFamily::Other => (
            declared.unwrap_or_else(|| {
                warnings.push(Warning::new(
                    WarningCode::VersionGuessed,
                    offset,
                    "no GAEB namespace and no version declaration; reading as 3.3",
                ));
                Version::V33
            }),
            None,
        ),
    };
    let dp = tape
        .child(root, Name::Award)
        .and_then(|award| tape.child(award, Name::DP))
        .and_then(|dp| Phase::from_code(tape.str(input, tape.nodes[dp as usize].text)));
    let phase = match (namespace_phase, dp) {
        (Some(ns), Some(dp)) if ns != dp => {
            warnings.push(Warning::new(
                WarningCode::PhaseDisagrees,
                offset,
                format!(
                    "namespace says phase {} but Award/DP says {}; the namespace wins",
                    ns.code(),
                    dp.code()
                ),
            ));
            Some(ns)
        }
        (Some(ns), _) => Some(ns),
        (None, Some(dp)) => Some(dp),
        (None, None) => {
            let guessed = file_name
                .and_then(|n| n.rsplit('.').next())
                .and_then(Phase::from_extension);
            if let Some(phase) = guessed {
                warnings.push(Warning::new(
                    WarningCode::PhaseGuessed,
                    offset,
                    format!("phase {} taken from the file extension", phase.code()),
                ));
            }
            guessed
        }
    };
    Ok(Route {
        version,
        declared_version,
        phase,
        namespace: tape.default_ns,
        warnings,
    })
}

enum NamespaceFamily {
    Legacy,
    Phased {
        version: Option<Version>,
        phase: Option<Phase>,
    },
    Other,
}

fn namespace_family(namespace: &str) -> NamespaceFamily {
    let trimmed = namespace.trim_end_matches('/');
    if trimmed == LEGACY_NAMESPACE {
        return NamespaceFamily::Legacy;
    }
    let Some(rest) = trimmed.strip_prefix(NAMESPACE_PREFIX) else {
        return NamespaceFamily::Other;
    };
    let (code, version) = rest.split_once('/').unwrap_or((rest, ""));
    NamespaceFamily::Phased {
        version: Version::parse(version),
        phase: Phase::from_code(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::warning::WarningCode;

    fn route(input: &[u8], name: Option<&str>) -> crate::error::Result<Route> {
        let tape = lex(input)?;
        detect(&tape, input, name)
    }

    #[test]
    fn namespace_33_x84() {
        let input = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84"
        ))
        .unwrap();
        let tape = lex(&input).unwrap();
        let r = detect(&tape, &input, None).unwrap();
        assert_eq!(r.version, Version::V33);
        assert_eq!(r.phase, Some(Phase::X84));
        assert_eq!(tape.str(&input, r.declared_version.unwrap()), "3.3");
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn namespace_200407_version_31() {
        let r = route(b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/200407\"><GAEBInfo><Version>3.1</Version></GAEBInfo><Award><DP>83</DP></Award></GAEB>", None).unwrap();
        assert_eq!(r.version, Version::V31);
        assert_eq!(r.phase, Some(Phase::X83));
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn namespace_200407_version_30() {
        let r = route(b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/200407\"><GAEBInfo><Version>3.0</Version></GAEBInfo><Award><DP>81</DP></Award></GAEB>", None).unwrap();
        assert_eq!(r.version, Version::V30);
        assert_eq!(r.phase, Some(Phase::X81));
    }

    #[test]
    fn namespace_200407_without_version_guesses_31() {
        let r = route(
            b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/200407\"><GAEBInfo/></GAEB>",
            None,
        )
        .unwrap();
        assert_eq!(r.version, Version::V31);
        assert!(
            r.warnings
                .iter()
                .any(|w| w.code == WarningCode::VersionGuessed)
        );
    }

    #[test]
    fn namespace_32_phase_from_namespace_when_dp_missing() {
        let r = route(b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA86/3.2\"><GAEBInfo><Version>3.2</Version></GAEBInfo></GAEB>", None).unwrap();
        assert_eq!(r.version, Version::V32);
        assert_eq!(r.phase, Some(Phase::X86));
    }

    #[test]
    fn dp_disagreeing_with_namespace_warns_and_namespace_wins() {
        let r = route(b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>84</DP></Award></GAEB>", None).unwrap();
        assert_eq!(r.phase, Some(Phase::X83));
        assert!(
            r.warnings
                .iter()
                .any(|w| w.code == WarningCode::PhaseDisagrees)
        );
    }

    #[test]
    fn extension_is_the_last_resort() {
        let r = route(
            b"<GAEB><GAEBInfo><Version>3.2</Version></GAEBInfo></GAEB>",
            Some("Angebot.X84"),
        )
        .unwrap();
        assert_eq!(r.version, Version::V32);
        assert_eq!(r.phase, Some(Phase::X84));
        assert!(
            r.warnings
                .iter()
                .any(|w| w.code == WarningCode::PhaseGuessed)
        );
        let none = route(
            b"<GAEB><GAEBInfo><Version>3.2</Version></GAEBInfo></GAEB>",
            None,
        )
        .unwrap();
        assert_eq!(none.phase, None);
    }

    #[test]
    fn version_from_declaration_when_namespace_is_missing_or_odd() {
        let r = route(
            b"<GAEB xmlns=\"urn:vendor\"><GAEBInfo><Version>3.3</Version></GAEBInfo></GAEB>",
            None,
        )
        .unwrap();
        assert_eq!(r.version, Version::V33);
        let guessed = route(b"<GAEB><GAEBInfo/></GAEB>", None).unwrap();
        assert_eq!(guessed.version, Version::V33);
        assert!(
            guessed
                .warnings
                .iter()
                .any(|w| w.code == WarningCode::VersionGuessed)
        );
    }

    #[test]
    fn phase_codes_round_trip() {
        for phase in Phase::ALL {
            assert_eq!(
                Phase::from_code(phase.code()),
                Some(phase),
                "{}",
                phase.code()
            );
        }
        assert_eq!(Phase::from_code("50.1"), Some(Phase::X50_1));
        assert_eq!(Phase::from_code("89B"), Some(Phase::X89B));
        assert_eq!(Phase::from_extension("x83"), Some(Phase::X83));
        assert_eq!(Phase::from_extension("X86ZR"), Some(Phase::X86ZR));
        assert_eq!(Phase::from_extension("txt"), None);
        assert_eq!(
            Phase::X84.namespace(Version::V33),
            Some("http://www.gaeb.de/GAEB_DA_XML/DA84/3.3".to_string())
        );
        assert_eq!(
            Phase::X84.namespace(Version::V31),
            Some("http://www.gaeb.de/GAEB_DA_XML/200407".to_string())
        );
        assert_eq!(Version::V32.as_str(), "3.2");
    }
}
