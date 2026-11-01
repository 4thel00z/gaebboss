//! The document as JSON for other programs: header, breakdown, categories,
//! items with texts and complements, markup items, totals and warnings.
//! Every decimal is a string, never a JSON number.

use serde_json::{Map, Value, json};

use crate::Document;
use crate::boq::ItemKind;
use crate::decimal::Decimal;
use crate::model::Totals;

fn decimal(value: Option<Decimal>) -> Value {
    value.map_or(Value::Null, |d| Value::String(d.to_string()))
}

fn text(doc: &Document, span: Option<crate::span::Span>) -> Value {
    span.map_or(Value::Null, |s| Value::String(doc.str(s).to_string()))
}

fn totals(totals: Option<&Totals>) -> Value {
    let Some(t) = totals else { return Value::Null };
    json!({
        "total": decimal(t.total),
        "discount_pcnt": decimal(t.discount_pcnt),
        "discount_amt": decimal(t.discount_amt),
        "tot_after_disc": decimal(t.tot_after_disc),
        "vat": decimal(t.vat),
        "vat_amount": decimal(t.vat_amount),
        "total_gross": decimal(t.total_gross),
    })
}

pub fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Standard => "Standard",
        ItemKind::LumpSum => "LumpSum",
        ItemKind::ProvisionalWithTotal => "ProvisionalWithTotal",
        ItemKind::ProvisionalWithoutTotal => "ProvisionalWithoutTotal",
        ItemKind::Alternative { .. } => "Alternative",
        ItemKind::Base { .. } => "Base",
        ItemKind::Hourly => "Hourly",
        ItemKind::Key => "Key",
        ItemKind::FreeQuantity => "FreeQuantity",
        ItemKind::NotApplicable => "NotApplicable",
    }
}

/// The whole document as one JSON value.
pub fn to_json(doc: &Document) -> Value {
    let project = doc.gaeb.prj_info.as_ref();
    let boq = doc.boq();
    let info = boq.and_then(|b| b.boq_info.as_ref());
    let breakdown: Vec<Value> = doc
        .breakdown()
        .levels()
        .iter()
        .map(|l| {
            json!({
                "type": format!("{:?}", l.kind),
                "label": l.label,
                "length": l.length,
                "numeric": l.numeric,
                "alignment": l.alignment.map(|a| format!("{a:?}").to_ascii_lowercase()),
            })
        })
        .collect();
    let items: Vec<Value> = doc
        .items()
        .map(|i| {
            let kind = i.kind();
            let mut row = Map::new();
            row.insert("id".into(), Value::String(i.id().to_string()));
            row.insert("oz".into(), Value::String(i.oz.dotted()));
            row.insert("kind".into(), Value::String(kind_name(kind).to_string()));
            if let ItemKind::Alternative { group, serial } = kind {
                row.insert("alternative_group".into(), json!(group));
                row.insert("alternative_serial".into(), json!(serial));
            }
            if let ItemKind::Base { group } = kind {
                row.insert("alternative_group".into(), json!(group));
            }
            row.insert("short_text".into(), Value::String(i.short_text()));
            row.insert("long_text".into(), Value::String(i.long_text()));
            row.insert("quantity".into(), decimal(i.quantity()));
            row.insert(
                "unit".into(),
                i.unit()
                    .map_or(Value::Null, |u| Value::String(u.to_string())),
            );
            row.insert("unit_price".into(), decimal(i.unit_price()));
            row.insert("total".into(), decimal(i.total()));
            row.insert("vat".into(), decimal(i.vat_percent()));
            row.insert(
                "counts_towards_total".into(),
                Value::Bool(i.counts_towards_total()),
            );
            row.insert("not_offered".into(), Value::Bool(i.not_offered()));
            let complements: Vec<Value> = i
                .complements()
                .iter()
                .map(|c| {
                    json!({
                        "mark": c.mark,
                        "kind": format!("{:?}", c.kind),
                        "label": c.label(),
                        "value": c.value(),
                        "empty": c.empty,
                        "tail": c.tail,
                        "number": decimal(c.number),
                    })
                })
                .collect();
            row.insert("complements".into(), Value::Array(complements));
            Value::Object(row)
        })
        .collect();
    let categories: Vec<Value> = doc
        .categories()
        .map(|c| json!({ "oz": c.oz.dotted(), "label": c.label, "depth": c.depth, "totals": totals(c.ctgy.totals.as_ref()) }))
        .collect();
    let markup_items: Vec<Value> = doc
        .markup_items()
        .map(|m| json!({ "id": m.id(), "oz": m.oz.dotted(), "base": decimal(m.base()), "percent": decimal(m.percent()), "total": decimal(m.total()) }))
        .collect();
    let warnings: Vec<Value> = doc.warnings.iter().map(|w| json!({ "code": format!("{:?}", w.code), "offset": w.offset, "message": w.message })).collect();
    json!({
        "version": doc.version().as_str(),
        "phase": doc.phase().map(|p| format!("X{}", p.code())),
        "project": {
            "name": text(doc, project.and_then(|p| p.name_prj)),
            "label": text(doc, project.and_then(|p| p.lbl_prj)),
            "currency": text(doc, project.and_then(|p| p.cur)),
        },
        "boq": {
            "id": text(doc, boq.and_then(|b| b.id)),
            "name": text(doc, info.and_then(|i| i.name)),
            "label": text(doc, info.and_then(|i| i.lbl_boq)),
            "breakdown": breakdown,
            "totals": totals(info.and_then(|i| i.totals.as_ref())),
        },
        "categories": categories,
        "items": items,
        "markup_items": markup_items,
        "warnings": warnings,
    })
}
