//! The yank menu: what can be copied from the selected row, and the text of
//! each choice.

use crossterm::event::KeyCode;
use gaebboss_core::Document;

use crate::outline::{Outline, RowKind};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum YankTarget {
    Oz,
    Id,
    ShortText,
    LongText,
    Markdown,
    Xml,
    Query,
    Command,
}

pub const TARGETS: [YankTarget; 8] = [
    YankTarget::Oz,
    YankTarget::Id,
    YankTarget::ShortText,
    YankTarget::LongText,
    YankTarget::Markdown,
    YankTarget::Xml,
    YankTarget::Query,
    YankTarget::Command,
];

impl YankTarget {
    pub fn key(self) -> char {
        match self {
            YankTarget::Oz => 'o',
            YankTarget::Id => 'i',
            YankTarget::ShortText => 's',
            YankTarget::LongText => 't',
            YankTarget::Markdown => 'm',
            YankTarget::Xml => 'x',
            YankTarget::Query => 'q',
            YankTarget::Command => 'c',
        }
    }

    pub fn from_key(code: KeyCode) -> Option<YankTarget> {
        let KeyCode::Char(c) = code else {
            return None;
        };
        TARGETS.into_iter().find(|t| t.key() == c)
    }

    pub fn name(self) -> &'static str {
        match self {
            YankTarget::Oz => "OZ",
            YankTarget::Id => "ID",
            YankTarget::ShortText => "short text",
            YankTarget::LongText => "long text",
            YankTarget::Markdown => "Markdown",
            YankTarget::Xml => "XML",
            YankTarget::Query => "jq query",
            YankTarget::Command => "shell command",
        }
    }
}

/// Quotes `text` for a POSIX shell when it holds anything but safe characters.
pub fn shell_quote(text: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c);
    if !text.is_empty() && text.chars().all(safe) {
        return text.to_string();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The jq program selecting this row in `gaebboss q`.
fn query(outline: &Outline, index: usize) -> Option<String> {
    let row = outline.row(index)?;
    let program = match row.kind() {
        RowKind::Bill => ".boq".to_string(),
        RowKind::Category => format!(".categories[] | select(.oz == \"{}\")", row.oz),
        RowKind::Item => format!(".items[] | select(.oz == \"{}\")", row.oz),
        RowKind::Markup => format!(".markup_items[] | select(.oz == \"{}\")", row.oz),
    };
    Some(program)
}

/// The shell command printing what the inspector shows for this row.
fn command(outline: &Outline, index: usize, target: &str) -> Option<String> {
    let row = outline.row(index)?;
    let file = shell_quote(target);
    let line = match row.kind() {
        RowKind::Item => format!("gaebboss md {file} --oz {}", shell_quote(&row.oz)),
        RowKind::Bill => format!("gaebboss info {file}"),
        RowKind::Category | RowKind::Markup => format!(
            "gaebboss hex {file} {}",
            shell_quote(&format!("oz:{}", row.oz))
        ),
    };
    Some(line)
}

/// The text a yank copies, `None` when the row has nothing of that kind.
pub fn yank_text(
    doc: &Document,
    outline: &Outline,
    index: usize,
    target: YankTarget,
    file: &str,
) -> Option<String> {
    let row = outline.row(index)?;
    let text = match target {
        YankTarget::Oz => row.oz.clone(),
        YankTarget::Id => row.id.clone(),
        YankTarget::ShortText => row.label.clone(),
        YankTarget::LongText => row.long_text.clone(),
        YankTarget::Markdown => row.markdown.clone(),
        YankTarget::Xml => {
            let range = outline.source_range(doc, index);
            String::from_utf8_lossy(doc.bytes().get(range).unwrap_or(&[])).into_owned()
        }
        YankTarget::Query => query(outline, index)?,
        YankTarget::Command => command(outline, index, file)?,
    };
    Some(text).filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for target in TARGETS {
            assert_eq!(
                YankTarget::from_key(KeyCode::Char(target.key())),
                Some(target)
            );
        }
        assert_eq!(YankTarget::from_key(KeyCode::Enter), None);
    }

    #[test]
    fn shell_quote_leaves_safe_text_alone() {
        assert_eq!(shell_quote("tender.X83"), "tender.X83");
        assert_eq!(shell_quote("my bill.x83"), "'my bill.x83'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn item_yanks_name_the_item() {
        let path = format!(
            "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
            env!("CARGO_MANIFEST_DIR")
        );
        let doc = Document::open(std::path::Path::new(&path)).unwrap();
        let outline = Outline::build(&doc);
        let index = outline
            .rows
            .iter()
            .position(|r| r.oz == "001.001.0010")
            .unwrap();
        let yank = |target| yank_text(&doc, &outline, index, target, "bid.x84");
        assert_eq!(yank(YankTarget::Oz).as_deref(), Some("001.001.0010"));
        assert_eq!(yank(YankTarget::Id).as_deref(), Some("ID02BED3D8"));
        assert_eq!(
            yank(YankTarget::Query).as_deref(),
            Some(".items[] | select(.oz == \"001.001.0010\")")
        );
        assert_eq!(
            yank(YankTarget::Command).as_deref(),
            Some("gaebboss md bid.x84 --oz 001.001.0010")
        );
        assert!(yank(YankTarget::Xml).is_some_and(|x| x.starts_with("<Item")));
    }
}
