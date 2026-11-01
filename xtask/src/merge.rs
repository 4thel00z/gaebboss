//! One typed model from all schema versions: the 3.3 library is the base,
//! elements and types that exist only in 3.1 or 3.2 are appended, and every
//! field remembers which versions declare it.

use std::collections::{BTreeMap, BTreeSet};

use crate::schema::{
    Base, ComplexType, Content, Max, Particle, Schema, SchemaVersion, SimpleType, TypeRef,
};

#[derive(Debug)]
pub struct Model {
    pub types: BTreeMap<String, MType>,
    pub enums: BTreeMap<String, MEnum>,
    /// `(kept, other)`: one element typed differently across versions; the
    /// kept type absorbs the other's members.
    pub aliases: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct MType {
    pub name: String,
    pub xsd: String,
    pub fields: Vec<MField>,
    pub attrs: Vec<MAttr>,
    pub text: Option<Scalar>,
    pub versions: BTreeSet<SchemaVersion>,
    /// Element order per schema version, for writing.
    pub order: BTreeMap<SchemaVersion, Vec<String>>,
    /// Children of different names may interleave (a repeated choice), so
    /// instances record their child order.
    pub ordered: bool,
}

#[derive(Debug, Clone)]
pub struct MField {
    pub element: String,
    pub name: String,
    pub kind: FieldKind,
    pub many: bool,
    pub boxed: bool,
    pub versions: BTreeSet<SchemaVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    Complex(String),
    Scalar(Scalar),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    Span,
    Decimal,
    Integer,
    Bool,
    Enum(String),
}

#[derive(Debug, Clone)]
pub struct MAttr {
    pub attribute: String,
    pub name: String,
    pub scalar: Scalar,
}

#[derive(Debug)]
pub struct MEnum {
    pub name: String,
    pub variants: Vec<(String, String)>,
}

const MAX_ENUM_VALUES: usize = 40;
const SPAN_ENUMS: &[&str] = &["tgDP", "tgVersion", "tgGAEBVersDate", "tgCur"];

pub fn merge(schemas: &[Schema]) -> Model {
    let mut model = Model {
        types: BTreeMap::new(),
        enums: BTreeMap::new(),
        aliases: Vec::new(),
    };
    let mut ordered: Vec<&Schema> = schemas.iter().collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.version));
    for schema in ordered {
        let mut pending: Vec<(String, ComplexType)> = schema
            .types
            .values()
            .chain(schema.extra_types.values())
            .map(|t| (rust_type_name(&t.name), t.clone()))
            .collect();
        while let Some((rust_name, ty)) = pending.pop() {
            let inline = absorb_type(&mut model, schema, &rust_name, &ty);
            pending.extend(inline);
        }
    }
    unify_aliases(&mut model);
    box_cycles(&mut model);
    model
}

/// Adds one complex type to the model; returns inline types found inside it.
fn absorb_type(
    model: &mut Model,
    schema: &Schema,
    rust_name: &str,
    ty: &ComplexType,
) -> Vec<(String, ComplexType)> {
    let mut inline = Vec::new();
    let text = ty.text.as_ref().map(|base| scalar_of(model, schema, base));
    let existing = model.types.get(rust_name).cloned();
    let mut fields = existing
        .as_ref()
        .map(|e| e.fields.clone())
        .unwrap_or_default();
    let mut attrs = existing
        .as_ref()
        .map(|e| e.attrs.clone())
        .unwrap_or_default();
    let existing_text = existing.as_ref().and_then(|e| e.text.clone());
    let mut versions = existing
        .as_ref()
        .map(|e| e.versions.clone())
        .unwrap_or_default();
    let mut order = existing
        .as_ref()
        .map(|e| e.order.clone())
        .unwrap_or_default();
    let mut ordered = existing.as_ref().is_some_and(|e| e.ordered);
    versions.insert(schema.version);
    ordered |= interleaves(&ty.content);
    let mut particles = Vec::new();
    collect(&ty.content, &mut particles);
    let mut sequence: Vec<String> = Vec::new();
    for (element, _, _) in &particles {
        if !sequence.contains(element) {
            sequence.push(element.clone());
        }
    }
    order.entry(schema.version).or_insert(sequence);
    for (element, type_ref, max_many) in particles {
        let kind = match &type_ref {
            TypeRef::Complex(name) => FieldKind::Complex(rust_type_name(name)),
            TypeRef::Simple(name) => FieldKind::Scalar(scalar_of(model, schema, name)),
            TypeRef::Inline(inner) => {
                let name = format!("{rust_name}{}", camel(&element));
                inline.push((name.clone(), (**inner).clone()));
                FieldKind::Complex(name)
            }
            TypeRef::InlineSimple(inner) => FieldKind::Scalar(scalar_of_simple(
                model,
                schema,
                inner,
                &format!("{rust_name}{}", camel(&element)),
            )),
        };
        match fields.iter_mut().find(|f| f.element == element) {
            Some(found) => {
                found.many |= max_many;
                found.versions.insert(schema.version);
                if let (FieldKind::Complex(kept), FieldKind::Complex(other)) = (&found.kind, &kind)
                    && kept != other
                {
                    model.aliases.push((kept.clone(), other.clone()));
                }
            }
            None => fields.push(MField {
                name: field_name(&element, &[]),
                element,
                kind,
                many: max_many,
                boxed: false,
                versions: BTreeSet::from([schema.version]),
            }),
        }
    }
    for a in &ty.attributes {
        if attrs.iter().any(|x| x.attribute == a.name) {
            continue;
        }
        let scalar = scalar_of(model, schema, &a.ty);
        let taken: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
        attrs.push(MAttr {
            attribute: a.name.clone(),
            name: field_name(&a.name, &taken),
            scalar,
        });
    }
    model.types.insert(
        rust_name.to_string(),
        MType {
            name: rust_name.to_string(),
            xsd: ty.name.clone(),
            fields,
            attrs,
            text: existing_text.or(text),
            versions,
            order,
            ordered,
        },
    );
    inline
}

/// Copies the members of an aliased type into the type the model kept, so a
/// 3.1 `PerfDescr/Description` (tgDescription) still reads into the 3.3
/// field (tgPerfDescription).
fn unify_aliases(model: &mut Model) {
    let aliases = std::mem::take(&mut model.aliases);
    for _ in 0..4 {
        for (kept, other) in &aliases {
            let Some(source) = model.types.get(other).cloned() else {
                continue;
            };
            let Some(target) = model.types.get_mut(kept) else {
                continue;
            };
            target.ordered |= source.ordered;
            for (version, sequence) in source.order {
                let entry = target.order.entry(version).or_default();
                for element in sequence {
                    if !entry.contains(&element) {
                        entry.push(element);
                    }
                }
            }
            for f in source.fields {
                if target.fields.iter().any(|t| t.element == f.element) {
                    continue;
                }
                target.fields.push(f);
            }
            for a in source.attrs {
                if target.attrs.iter().any(|t| t.attribute == a.attribute) {
                    continue;
                }
                target.attrs.push(a);
            }
        }
    }
    model.aliases = aliases;
}

/// Whether a repeated group with several alternatives lets children of
/// different names interleave.
fn interleaves(content: &Content) -> bool {
    let parts = match content {
        Content::Sequence(p) | Content::Choice(p) => p,
        Content::Empty => return false,
    };
    parts.iter().any(|p| match p {
        Particle::Group { content, max, .. } => {
            let alternatives = match &**content {
                Content::Sequence(inner) | Content::Choice(inner) => inner.len(),
                Content::Empty => 0,
            };
            (*max != Max::Bounded(1) && alternatives > 1) || interleaves(content)
        }
        Particle::Element { .. } => false,
    })
}

fn collect(content: &Content, out: &mut Vec<(String, TypeRef, bool)>) {
    let parts = match content {
        Content::Sequence(p) | Content::Choice(p) => p,
        Content::Empty => return,
    };
    for p in parts {
        match p {
            Particle::Element { name, ty, max, .. } => {
                out.push((name.clone(), ty.clone(), *max != Max::Bounded(1)))
            }
            Particle::Group { content, max, .. } => {
                let mut inner = Vec::new();
                collect(content, &mut inner);
                let repeated = *max != Max::Bounded(1);
                for (name, ty, many) in inner {
                    out.push((name, ty, many || repeated));
                }
            }
        }
    }
}

fn scalar_of(model: &mut Model, schema: &Schema, type_name: &str) -> Scalar {
    if let Some(xs) = type_name.strip_prefix("xs:") {
        return match xs {
            "decimal" => Scalar::Decimal,
            "integer" | "int" | "long" | "positiveInteger" | "nonNegativeInteger" => {
                Scalar::Integer
            }
            "boolean" => Scalar::Bool,
            _ => Scalar::Span,
        };
    }
    let Some(simple) = schema
        .simple
        .get(type_name)
        .or_else(|| schema.extra_simple.get(type_name))
    else {
        return Scalar::Span;
    };
    scalar_of_simple(model, schema, simple, &rust_type_name(type_name))
}

/// The type itself when it enumerates values, else the nearest ancestor
/// that does (`tgHourIt` restricts `tgYesNo` without repeating Yes/No).
fn nearest_enumerated<'s>(schema: &'s Schema, simple: &'s SimpleType) -> &'s SimpleType {
    let mut current = simple;
    loop {
        if !current.enumeration.is_empty() {
            return current;
        }
        let Base::Derived(parent) = &current.base else {
            return simple;
        };
        let Some(next) = schema
            .simple
            .get(parent)
            .or_else(|| schema.extra_simple.get(parent))
        else {
            return simple;
        };
        current = next;
    }
}

fn scalar_of_simple(
    model: &mut Model,
    schema: &Schema,
    simple: &SimpleType,
    rust_name: &str,
) -> Scalar {
    let enumerated = nearest_enumerated(schema, simple);
    let values = &enumerated.enumeration;
    let enum_name = match std::ptr::eq(enumerated, simple) {
        true => rust_name.to_string(),
        false => rust_type_name(&enumerated.name),
    };
    let yes_no = !values.is_empty() && values.iter().all(|v| v == "Yes" || v == "No");
    if yes_no {
        return Scalar::Bool;
    }
    let enumerable = values.len() >= 2
        && values.len() <= MAX_ENUM_VALUES
        && !SPAN_ENUMS.contains(&enumerated.name.as_str())
        && values
            .iter()
            .all(|v| !v.starts_with(|c: char| c.is_ascii_digit()));
    if enumerable {
        let entry = model
            .enums
            .entry(enum_name.clone())
            .or_insert_with(|| MEnum {
                name: enum_name.clone(),
                variants: Vec::new(),
            });
        for v in values {
            if entry.variants.iter().any(|(_, xml)| xml == v) {
                continue;
            }
            entry.variants.push((variant_name(v), v.clone()));
        }
        return Scalar::Enum(enum_name);
    }
    let root = schema
        .simple_root(&simple.name)
        .map(|s| s.base.clone())
        .unwrap_or_else(|| simple.base.clone());
    let base = match &simple.base {
        Base::Derived(_) => root,
        other => other.clone(),
    };
    match base {
        Base::Decimal => Scalar::Decimal,
        Base::Integer => Scalar::Integer,
        Base::Boolean => Scalar::Bool,
        _ => Scalar::Span,
    }
}

/// Boxes single-valued complex fields that would otherwise make a type
/// contain itself.
fn box_cycles(model: &mut Model) {
    let names: Vec<String> = model.types.keys().cloned().collect();
    let edges: BTreeMap<String, Vec<String>> = names
        .iter()
        .map(|n| {
            let targets = model.types[n]
                .fields
                .iter()
                .filter(|f| !f.many)
                .filter_map(|f| match &f.kind {
                    FieldKind::Complex(t) => Some(t.clone()),
                    FieldKind::Scalar(_) => None,
                })
                .collect();
            (n.clone(), targets)
        })
        .collect();
    for name in &names {
        let Some(entry) = model.types.get_mut(name) else {
            continue;
        };
        for field in entry.fields.iter_mut().filter(|f| !f.many) {
            let FieldKind::Complex(target) = &field.kind else {
                continue;
            };
            if reaches(&edges, target, name) {
                field.boxed = true;
            }
        }
    }
}

fn reaches(edges: &BTreeMap<String, Vec<String>>, from: &str, to: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from.to_string()];
    while let Some(current) = stack.pop() {
        if current == to {
            return true;
        }
        if !seen.insert(current.clone()) {
            continue;
        }
        if let Some(next) = edges.get(&current) {
            stack.extend(next.iter().cloned());
        }
    }
    false
}

/// `tgBoQCtgy` -> `BoQCtgy`; lower-case HTML-ish types get an `Html` prefix;
/// underscores camel-case away.
pub fn rust_type_name(xsd: &str) -> String {
    let bare = xsd.strip_prefix("tg").unwrap_or(xsd);
    if bare.chars().next().is_some_and(char::is_lowercase) {
        return format!("Html{}", camel(bare));
    }
    bare.split('_').map(camel).collect()
}

pub fn camel(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first
        .to_uppercase()
        .chain(chars)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// `N/A` -> `NA`, `left` -> `Left`, `BoQLevel` stays.
pub fn variant_name(value: &str) -> String {
    let cleaned: String = value
        .split(|c: char| !c.is_alphanumeric())
        .map(camel)
        .collect();
    match cleaned.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        true => format!("V{cleaned}"),
        false => cleaned,
    }
}

const WORD_FIXES: &[(&str, &str)] = &[("BoQ", "Boq"), ("RNo", "Rno")];
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "box", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true",
    "try", "type", "unsafe", "use", "where", "while", "yield",
];

/// `LblBoQ` -> `lbl_boq`, `RNoPart` -> `rno_part`, `Type` -> `ty`.
pub fn field_name(element: &str, taken: &[String]) -> String {
    let mut fixed = element.to_string();
    for (from, to) in WORD_FIXES {
        fixed = fixed.replace(from, to);
    }
    let mut out = String::new();
    let chars: Vec<char> = fixed.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i + 1);
        let boundary = c.is_uppercase()
            && prev.is_some_and(|p| {
                p.is_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))
            });
        if boundary {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    if out == "type" {
        out = "ty".to_string();
    } else if KEYWORDS.contains(&out.as_str()) {
        out.push_str("_value");
    }
    if taken.contains(&out) {
        out = format!("attr_{out}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_names_follow_the_house_style() {
        assert_eq!(field_name("LblBoQ", &[]), "lbl_boq");
        assert_eq!(field_name("BoQBkdn", &[]), "boq_bkdn");
        assert_eq!(field_name("RNoPart", &[]), "rno_part");
        assert_eq!(field_name("GAEBInfo", &[]), "gaeb_info");
        assert_eq!(field_name("LblUPComp1", &[]), "lbl_up_comp1");
        assert_eq!(field_name("ITMarkup", &[]), "it_markup");
        assert_eq!(field_name("ALNGroupNo", &[]), "aln_group_no");
        assert_eq!(field_name("CtlgID", &[]), "ctlg_id");
        assert_eq!(field_name("IDRef", &[]), "id_ref");
        assert_eq!(field_name("Type", &[]), "ty");
        assert_eq!(field_name("UP", &[]), "up");
        assert_eq!(field_name("Name", &["name".to_string()]), "attr_name");
    }

    #[test]
    fn type_and_variant_names() {
        assert_eq!(rust_type_name("tgBoQCtgy"), "BoQCtgy");
        assert_eq!(rust_type_name("tgspan"), "HtmlSpan");
        assert_eq!(rust_type_name("tgpTC"), "HtmlPTC");
        assert_eq!(rust_type_name("tgSC_GAEB"), "SCGAEB");
        assert_eq!(variant_name("N/A"), "NA");
        assert_eq!(variant_name("left"), "Left");
        assert_eq!(variant_name("BoQLevel"), "BoQLevel");
    }

    #[test]
    fn interleaving_types_are_marked_ordered() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../standard/3.3");
        let schema = crate::schema::load(&dir, SchemaVersion::V33).unwrap();
        let model = merge(&[schema]);
        assert!(model.types["BoQText"].ordered);
        assert!(model.types["FTextTC"].ordered);
        assert!(model.types["Itemlist"].ordered);
        assert!(model.types["BoQBody"].ordered);
        assert!(!model.types["Item"].ordered);
        assert!(!model.types["GAEBInfo"].ordered);
    }
}
