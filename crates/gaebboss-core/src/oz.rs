//! The Ordnungszahl: an item's position in the bill, formed from the
//! breakdown levels declared in `BoQInfo/BoQBkdn`.

use crate::Document;
use crate::model::{BoQBkdn, BoQBkdnType, BoQInfo, LeftRight};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BkdnLevel {
    Lot,
    BoQLevel,
    Item,
    Index,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    pub kind: BkdnLevel,
    pub length: usize,
    pub numeric: bool,
    pub alignment: Option<LeftRight>,
    pub label: Option<String>,
}

/// The breakdown structure of a bill: which levels an Ordnungszahl has
/// and how each one is written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Breakdown {
    levels: Vec<Level>,
}

impl Breakdown {
    pub fn new(levels: &[BoQBkdn]) -> Breakdown {
        Breakdown {
            levels: levels.iter().map(|l| level_of(l, None)).collect(),
        }
    }

    pub fn from_info(doc: &Document, info: &BoQInfo) -> Breakdown {
        Breakdown {
            levels: info
                .boq_bkdn
                .iter()
                .map(|l| level_of(l, l.lbl_boq_bkdn.map(|s| doc.str(s).to_string())))
                .collect(),
        }
    }

    pub fn levels(&self) -> &[Level] {
        &self.levels
    }

    /// How many category levels (Lot and BoQLevel) precede the item part.
    pub fn category_levels(&self) -> usize {
        self.levels
            .iter()
            .filter(|l| matches!(l.kind, BkdnLevel::Lot | BkdnLevel::BoQLevel))
            .count()
    }

    fn level_at(&self, depth: usize) -> Option<&Level> {
        self.levels
            .iter()
            .filter(|l| l.kind != BkdnLevel::Index)
            .nth(depth)
    }

    /// One part of the Ordnungszahl written to its level's width: numeric
    /// levels zero-pad on the left, others pad with spaces per alignment.
    pub fn part(&self, depth: usize, raw: &str) -> String {
        let Some(level) = self.level_at(depth) else {
            return raw.to_string();
        };
        let width = level.length;
        let len = raw.chars().count();
        if len >= width || width == 0 {
            return raw.to_string();
        }
        let pad = width - len;
        match (level.numeric, level.alignment) {
            (true, _) => format!("{}{raw}", "0".repeat(pad)),
            (false, Some(LeftRight::Right)) => format!("{}{raw}", " ".repeat(pad)),
            (false, _) => format!("{raw}{}", " ".repeat(pad)),
        }
    }

    /// The Ordnungszahl of an item below the given category parts.
    pub fn oz(&self, category_parts: &[&str], item_part: &str, index: Option<&str>) -> Oz {
        let mut parts: Vec<String> = category_parts
            .iter()
            .enumerate()
            .map(|(i, p)| self.part(i, p))
            .collect();
        let wanted = self.category_levels();
        while !self.levels.is_empty() && parts.len() < wanted {
            parts.push("_".to_string());
        }
        let item = self.part(wanted.max(category_parts.len()), item_part);
        Oz {
            parts,
            item: Some(item),
            index: index.map(str::to_string),
            lot: self
                .levels
                .first()
                .is_some_and(|l| l.kind == BkdnLevel::Lot),
        }
    }

    /// The Ordnungszahl of a category at its own depth.
    pub fn category_oz(&self, parts: &[&str]) -> Oz {
        Oz {
            parts: parts
                .iter()
                .enumerate()
                .map(|(i, p)| self.part(i, p))
                .collect(),
            item: None,
            index: None,
            lot: self
                .levels
                .first()
                .is_some_and(|l| l.kind == BkdnLevel::Lot),
        }
    }
}

fn level_of(l: &BoQBkdn, label: Option<String>) -> Level {
    let kind = match l.ty {
        Some(BoQBkdnType::Lot) => BkdnLevel::Lot,
        Some(BoQBkdnType::Item) => BkdnLevel::Item,
        Some(BoQBkdnType::Index) => BkdnLevel::Index,
        Some(BoQBkdnType::BoQLevel) | None => BkdnLevel::BoQLevel,
    };
    Level {
        kind,
        length: l.length.and_then(|n| usize::try_from(n).ok()).unwrap_or(0),
        numeric: l.num.unwrap_or(true),
        alignment: l.alignment,
        label,
    }
}

/// An item's or category's Ordnungszahl, part by part.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Oz {
    parts: Vec<String>,
    item: Option<String>,
    index: Option<String>,
    lot: bool,
}

impl Oz {
    /// Levels joined by dots, the index appended directly: `001.001.0010A`.
    pub fn dotted(&self) -> String {
        let mut out = self.parts.join(".");
        if let Some(item) = &self.item {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(item);
        }
        if let Some(index) = &self.index {
            out.push_str(index);
        }
        out
    }

    /// All parts at their declared width with no separators.
    pub fn compact(&self) -> String {
        let mut out = self.parts.concat();
        if let Some(item) = &self.item {
            out.push_str(item);
        }
        if let Some(index) = &self.index {
            out.push_str(index);
        }
        out
    }

    /// The first level, which is the trade in most bills.
    pub fn trade(&self) -> &str {
        self.parts.first().map(String::as_str).unwrap_or("")
    }

    pub fn lot(&self) -> Option<&str> {
        match self.lot {
            true => self.parts.first().map(String::as_str),
            false => None,
        }
    }

    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    pub fn item_part(&self) -> &str {
        self.item.as_deref().unwrap_or("")
    }

    pub fn index(&self) -> Option<&str> {
        self.index.as_deref()
    }

    pub fn depth(&self) -> usize {
        self.parts.len()
    }
}

impl std::fmt::Display for Oz {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.dotted())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BoQBkdn, BoQBkdnType, LeftRight};

    fn level(ty: BoQBkdnType, length: i64, num: bool, alignment: Option<LeftRight>) -> BoQBkdn {
        BoQBkdn {
            ty: Some(ty),
            length: Some(length),
            num: Some(num),
            alignment,
            ..Default::default()
        }
    }

    fn bvbs() -> Breakdown {
        Breakdown::new(&[
            level(BoQBkdnType::BoQLevel, 3, true, None),
            level(BoQBkdnType::BoQLevel, 3, true, None),
            level(BoQBkdnType::Item, 4, true, None),
            level(BoQBkdnType::Index, 1, false, Some(LeftRight::Left)),
        ])
    }

    #[test]
    fn numeric_levels_pad_with_zeros_on_the_left() {
        let b = bvbs();
        assert_eq!(b.part(0, "1"), "001");
        assert_eq!(b.part(2, "10"), "0010");
        assert_eq!(b.part(2, "0010"), "0010");
    }

    #[test]
    fn num_no_left_alignment_pads_right_with_spaces() {
        let b = Breakdown::new(&[
            level(BoQBkdnType::BoQLevel, 3, false, Some(LeftRight::Left)),
            level(BoQBkdnType::Item, 4, true, None),
        ]);
        assert_eq!(b.part(0, "A"), "A  ");
        let right = Breakdown::new(&[level(
            BoQBkdnType::BoQLevel,
            3,
            false,
            Some(LeftRight::Right),
        )]);
        assert_eq!(right.part(0, "A"), "  A");
    }

    #[test]
    fn oz_forms_from_category_parts_item_and_index() {
        let b = bvbs();
        let oz = b.oz(&["1", "1"], "10", None);
        assert_eq!(oz.dotted(), "001.001.0010");
        assert_eq!(oz.compact(), "0010010010");
        let indexed = b.oz(&["1", "1"], "10", Some("A"));
        assert_eq!(indexed.dotted(), "001.001.0010A");
        assert_eq!(indexed.compact(), "0010010010A");
        assert_eq!(indexed.trade(), "001");
        assert_eq!(indexed.item_part(), "0010");
        assert_eq!(indexed.index(), Some("A"));
    }

    #[test]
    fn missing_levels_show_as_underscores() {
        let b = Breakdown::new(&[
            level(BoQBkdnType::BoQLevel, 1, true, None),
            level(BoQBkdnType::BoQLevel, 1, true, None),
            level(BoQBkdnType::Item, 2, true, None),
        ]);
        let oz = b.oz(&["2"], "10", None);
        assert_eq!(oz.dotted(), "2._.10");
    }

    #[test]
    fn lot_is_the_first_level() {
        let b = Breakdown::new(&[
            level(BoQBkdnType::Lot, 1, true, None),
            level(BoQBkdnType::BoQLevel, 2, true, None),
            level(BoQBkdnType::Item, 3, true, None),
        ]);
        let oz = b.oz(&["1", "1"], "1", None);
        assert_eq!(oz.dotted(), "1.01.001");
        assert_eq!(oz.lot(), Some("1"));
    }

    #[test]
    fn without_breakdown_parts_pass_through() {
        let b = Breakdown::new(&[]);
        assert_eq!(
            b.oz(&["01", "02"], "0010", Some("A")).dotted(),
            "01.02.0010A"
        );
    }

    #[test]
    fn category_oz_stops_at_its_depth() {
        let b = bvbs();
        assert_eq!(b.category_oz(&["1"]).dotted(), "001");
        assert_eq!(b.category_oz(&["1", "2"]).dotted(), "001.002");
    }
}
