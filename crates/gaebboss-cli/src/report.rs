//! Plain-text reports: the info summary, the item listing, and long texts
//! as plain text or Markdown.

use std::collections::BTreeMap;

use gaebboss_core::Document;
use gaebboss_core::boq::{Entry, ItemRef};
use gaebboss_core::decimal::Decimal;
use gaebboss_core::json::kind_name;

pub fn info(doc: &Document) -> String {
    let project = doc.gaeb.prj_info.as_ref();
    let info = doc.boq().and_then(|b| b.boq_info.as_ref());
    let text = |span: Option<gaebboss_core::span::Span>| {
        span.map(|s| doc.str(s).to_string()).unwrap_or_default()
    };
    let breakdown = doc
        .breakdown()
        .levels()
        .iter()
        .map(|l| format!("{:?}({})", l.kind, l.length))
        .collect::<Vec<_>>()
        .join(" ");
    let lines = [
        ("version", doc.version().as_str().to_string()),
        (
            "phase",
            doc.phase()
                .map(|p| format!("X{}", p.code()))
                .unwrap_or_default(),
        ),
        ("project", text(project.and_then(|p| p.name_prj))),
        ("project label", text(project.and_then(|p| p.lbl_prj))),
        ("boq", text(info.and_then(|i| i.name))),
        ("boq label", text(info.and_then(|i| i.lbl_boq))),
        ("breakdown", breakdown),
        ("categories", doc.categories().count().to_string()),
        ("items", doc.items().count().to_string()),
        ("markup items", doc.markup_items().count().to_string()),
        (
            "total",
            doc.declared_total()
                .map(|d| d.to_string())
                .unwrap_or_default(),
        ),
        ("warnings", doc.warnings.len().to_string()),
    ];
    let mut out: String = lines
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| format!("{key}: {value}\n"))
        .collect();
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    for w in &doc.warnings {
        *histogram.entry(format!("{:?}", w.code)).or_default() += 1;
    }
    out.extend(histogram.iter().map(|(code, n)| format!("  {code}: {n}\n")));
    out
}

pub fn items(doc: &Document, counting: bool, tsv: bool) -> String {
    let cell = |d: Option<Decimal>| d.map(|d| d.to_string()).unwrap_or_default();
    let header = match tsv {
        true => "oz\tid\tkind\tquantity\tunit\tunit_price\ttotal\tshort_text\n",
        false => "",
    };
    let rows = doc
        .items()
        .filter(|item| !counting || item.counts_towards_total())
        .map(|item| {
            let short = item.short_text().replace(['\t', '\n'], " ");
            match tsv {
                true => format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    item.oz,
                    item.id(),
                    kind_name(item.kind()),
                    cell(item.quantity()),
                    item.unit().unwrap_or(""),
                    cell(item.unit_price()),
                    cell(item.total()),
                    short
                ),
                false => format!(
                    "{:<16} {:<24} {:>12} {:<5} {:>12} {:>14}  {}\n",
                    item.oz.dotted(),
                    kind_name(item.kind()),
                    cell(item.quantity()),
                    item.unit().unwrap_or(""),
                    cell(item.unit_price()),
                    cell(item.total()),
                    short
                ),
            }
        });
    std::iter::once(header.to_string()).chain(rows).collect()
}

/// Whether `key` names this item: its dotted or compact Ordnungszahl, or its ID.
pub fn matches(item: &ItemRef<'_>, key: &str) -> bool {
    item.oz.dotted() == key || item.oz.compact() == key || item.id() == key
}

fn find<'a>(doc: &'a Document, oz: &str) -> Result<ItemRef<'a>, String> {
    doc.items()
        .find(|item| matches(item, oz))
        .ok_or_else(|| format!("no item {oz}"))
}

/// The long text of one item, or every item headed by its Ordnungszahl and
/// short text.
pub fn text(doc: &Document, oz: Option<&str>) -> Result<String, String> {
    if let Some(oz) = oz {
        return Ok(find(doc, oz)?.long_text());
    }
    Ok(doc
        .items()
        .map(|item| {
            format!(
                "{}  {}\n{}",
                item.oz.dotted(),
                item.short_text(),
                item.long_text()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// The Markdown long text of one item, or the whole bill: categories as
/// headings by depth, each item a heading with its long text below, remarks
/// in place, and the additional texts before and after the bill.
pub fn markdown(doc: &Document, oz: Option<&str>) -> Result<String, String> {
    if let Some(oz) = oz {
        return Ok(find(doc, oz)?.long_text_markdown());
    }
    let heading =
        |depth: usize, title: String| format!("{} {title}", "#".repeat((depth + 1).min(6)));
    let title = doc
        .boq()
        .and_then(|b| b.boq_info.as_ref())
        .and_then(|i| i.name)
        .map(|s| doc.str(s).trim().to_string())
        .filter(|name| !name.is_empty())
        .map(|name| heading(0, name));
    let before = doc
        .award_add_texts()
        .map(|t| note(t.short_text(), t.long_text_markdown()));
    let blocks = doc.entries().map(|entry| match entry {
        Entry::Category(c) => heading(c.depth + 1, format!("{} {}", c.oz.dotted(), c.label)),
        Entry::Item(item) => {
            let head = heading(
                item.path.len() + 1,
                format!("{} {}", item.oz.dotted(), item.short_text()),
            );
            let body = item.long_text_markdown();
            match body.trim().is_empty() {
                true => head,
                false => format!("{head}\n\n{}", body.trim_end()),
            }
        }
        Entry::Markup(m) => {
            let percent = m.percent().map(|p| format!(" {p} %")).unwrap_or_default();
            heading(
                m.path.len() + 1,
                format!("{} Zuschlag{percent}", m.oz.dotted()),
            )
        }
        Entry::Remark(r) => note(r.short_text(), r.long_text_markdown()),
    });
    let after = doc
        .gaeb_add_texts()
        .map(|t| note(t.short_text(), t.long_text_markdown()));
    Ok(title
        .into_iter()
        .chain(before)
        .chain(blocks)
        .chain(after)
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// A text without an Ordnungszahl: its long text, or its short text when it
/// has none.
fn note(short: String, long: String) -> String {
    match long.trim().is_empty() {
        true => short.trim().to_string(),
        false => long.trim_end().to_string(),
    }
}
