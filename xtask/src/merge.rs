//! One typed model from all schema versions: the 3.3 library is the base,
//! elements and types that exist only in 3.1 or 3.2 are appended, and every
//! field remembers which versions declare it.

use std::collections::{BTreeMap, BTreeSet};

use crate::schema::{
    Base, ComplexType, Content, Particle, Schema, SchemaVersion, SimpleType, TypeRef,
};

#[derive(Debug)]
pub struct Model {
    pub root: String,
    pub types: BTreeMap<String, MType>,
    pub enums: BTreeMap<String, MEnum>,
    /// `(kept, other)`: one element typed differently across versions; the
    /// kept type absorbs the other's members.
    pub aliases: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct MType {
    pub name: String,
    pub xsd: String,
    pub fields: Vec<MField>,
    pub attrs: Vec<MAttr>,
    pub text: Option<Scalar>,
    pub versions: BTreeSet<SchemaVersion>,
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
    pub required: bool,
}

#[derive(Debug)]
pub struct MEnum {
    pub name: String,
    pub xsd: String,
    pub variants: Vec<(String, String)>,
}

const MAX_ENUM_VALUES: usize = 40;
const SPAN_ENUMS: &[&str] = &["tgDP", "tgVersion", "tgGAEBVersDate", "tgCur"];

pub fn merge(schemas: &[Schema]) -> Model {
    let mut model = Model {
        root: "GAEB".to_string(),
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

/// Copies the members of an aliased type into the type the model kept, so a
/// 3.1 `PerfDescr/Description` (tgDescription) still reads into the 3.3
/// field (tgPerfDescription).
fn unify_aliases(model: &mut Model) {
    let aliases = std::mem::take(&mut model.aliases);
    for _ in 0..4 {
        for (kept, other) in &aliases {
            let Some(source) = model.types.get(other) else {
                continue;
            };
            let (fields, attrs) = (source.fields.clone(), source.attrs.clone());
            let Some(target) = model.types.get_mut(kept) else {
                continue;
            };
            for f in fields {
                if target.fields.iter().any(|t| t.element == f.element) {
                    continue;
                }
                target.fields.push(f);
            }
            for a in attrs {
                if target.attrs.iter().any(|t| t.attribute == a.attribute) {
                    continue;
                }
                target.attrs.push(a);
            }
        }
    }
    model.aliases = aliases;
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
    let (mut fields, mut attrs, existing_text, mut versions) = match model.types.get(rust_name) {
        Some(existing) => (
            existing.fields.clone(),
            existing.attrs.clone(),
            existing.text.clone(),
            existing.versions.clone(),
        ),
        None => (Vec::new(), Vec::new(), None, BTreeSet::new()),
    };
    versions.insert(schema.version);
    let mut particles = Vec::new();
    collect(&ty.content, &mut particles);
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
            Some(existing) => {
                existing.many |= max_many;
                existing.versions.insert(schema.version);
                if let (FieldKind::Complex(kept), FieldKind::Complex(other)) =
                    (&existing.kind, &kind)
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
            required: a.required,
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
        },
    );
    inline
}

fn collect(content: &Content, out: &mut Vec<(String, TypeRef, bool)>) {
    let parts = match content {
        Content::Sequence(p) | Content::Choice(p) => p,
        Content::Empty => return,
    };
    for p in parts {
        match p {
            Particle::Element { name, ty, max, .. } => out.push((
                name.clone(),
                ty.clone(),
                *max != crate::schema::Max::Bounded(1),
            )),
            Particle::Group { content, max, .. } => {
                let mut inner = Vec::new();
                collect(content, &mut inner);
                let repeated = *max != crate::schema::Max::Bounded(1);
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

fn scalar_of_simple(
    model: &mut Model,
    schema: &Schema,
    simple: &SimpleType,
    rust_name: &str,
) -> Scalar {
    let values = &simple.enumeration;
    let yes_no = !values.is_empty() && values.iter().all(|v| v == "Yes" || v == "No");
    if yes_no {
        return Scalar::Bool;
    }
    if values.len() >= 2
        && values.len() <= MAX_ENUM_VALUES
        && !SPAN_ENUMS.contains(&simple.name.as_str())
        && values
            .iter()
            .all(|v| !v.starts_with(|c: char| c.is_ascii_digit()))
    {
        let entry = model
            .enums
            .entry(rust_name.to_string())
            .or_insert_with(|| MEnum {
                name: rust_name.to_string(),
                xsd: simple.name.clone(),
                variants: Vec::new(),
            });
        for v in values {
            if entry.variants.iter().any(|(_, xml)| xml == v) {
                continue;
            }
            entry.variants.push((variant_name(v), v.clone()));
        }
        return Scalar::Enum(rust_name.to_string());
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
            (
                n.clone(),
                model.types[n]
                    .fields
                    .iter()
                    .filter(|f| !f.many)
                    .filter_map(|f| match &f.kind {
                        FieldKind::Complex(t) => Some(t.clone()),
                        FieldKind::Scalar(_) => None,
                    })
                    .collect(),
            )
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

/// `tgBoQCtgy` -> `BoQCtgy`; lower-case HTML-ish types get an `Html` prefix.
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
        assert_eq!(variant_name("N/A"), "NA");
        assert_eq!(variant_name("left"), "Left");
        assert_eq!(variant_name("BoQLevel"), "BoQLevel");
    }
}
