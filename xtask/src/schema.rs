//! The GAEB XSDs as a schema model: complex and simple types, particles
//! with their cardinalities, facets, and the per-phase restrictions.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaVersion {
    V31,
    V32,
    V33,
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<roxmltree::Error> for Error {
    fn from(e: roxmltree::Error) -> Self {
        Error(e.to_string())
    }
}

#[derive(Debug)]
pub struct Schema {
    pub version: SchemaVersion,
    pub root: String,
    pub types: BTreeMap<String, ComplexType>,
    pub simple: BTreeMap<String, SimpleType>,
    pub extra_types: BTreeMap<String, ComplexType>,
    pub extra_simple: BTreeMap<String, SimpleType>,
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone)]
pub struct ComplexType {
    pub name: String,
    pub content: Content,
    pub attributes: Vec<Attribute>,
    pub text: Option<String>,
    pub mixed: bool,
}

#[derive(Debug, Clone)]
pub enum Content {
    Sequence(Vec<Particle>),
    Choice(Vec<Particle>),
    Empty,
}

#[derive(Debug, Clone)]
pub enum Particle {
    Element {
        name: String,
        ty: TypeRef,
        min: u32,
        max: Max,
    },
    Group {
        content: Box<Content>,
        min: u32,
        max: Max,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Max {
    Bounded(u32),
    Unbounded,
}

#[derive(Debug, Clone)]
pub enum TypeRef {
    Complex(String),
    Simple(String),
    Inline(Box<ComplexType>),
    InlineSimple(Box<SimpleType>),
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub ty: String,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SimpleType {
    pub name: String,
    pub base: Base,
    pub enumeration: Vec<String>,
    pub max_length: Option<u32>,
    pub pattern: Option<String>,
    pub total_digits: Option<u8>,
    pub fraction_digits: Option<u8>,
    pub min_inclusive: Option<i64>,
    pub max_inclusive: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    String,
    NormalizedString,
    Token,
    Decimal,
    Integer,
    Date,
    Time,
    GYearMonth,
    Boolean,
    Id,
    IdRef,
    Base64,
    Derived(std::string::String),
    Other(std::string::String),
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub phase: String,
    pub namespace: String,
    pub restricted: BTreeMap<String, Restriction>,
}

#[derive(Debug, Clone, Default)]
pub struct Restriction {
    pub allowed: BTreeSet<String>,
    pub required: BTreeSet<String>,
}

impl ComplexType {
    /// Every element particle in document order, groups flattened.
    pub fn elements(&self) -> Vec<&Particle> {
        let mut out = Vec::new();
        collect_elements(&self.content, &mut out);
        out
    }
}

fn collect_elements<'a>(content: &'a Content, out: &mut Vec<&'a Particle>) {
    let parts = match content {
        Content::Sequence(p) | Content::Choice(p) => p,
        Content::Empty => return,
    };
    for p in parts {
        match p {
            Particle::Element { .. } => out.push(p),
            Particle::Group { content, .. } => collect_elements(content, out),
        }
    }
}

impl Particle {
    pub fn name(&self) -> Option<&str> {
        match self {
            Particle::Element { name, .. } => Some(name),
            Particle::Group { .. } => None,
        }
    }

    pub fn type_ref(&self) -> Option<&TypeRef> {
        match self {
            Particle::Element { ty, .. } => Some(ty),
            Particle::Group { .. } => None,
        }
    }

    pub fn type_name(&self) -> Option<&str> {
        match self.type_ref()? {
            TypeRef::Complex(n) | TypeRef::Simple(n) => Some(n),
            TypeRef::Inline(_) | TypeRef::InlineSimple(_) => None,
        }
    }

    pub fn bounds(&self) -> (u32, Max) {
        match self {
            Particle::Element { min, max, .. } | Particle::Group { min, max, .. } => (*min, *max),
        }
    }
}

impl Schema {
    /// The simple type behind a name, following `tg*` derivations.
    pub fn simple_root(&self, name: &str) -> Option<&SimpleType> {
        let mut current = self
            .simple
            .get(name)
            .or_else(|| self.extra_simple.get(name))?;
        loop {
            let Base::Derived(parent) = &current.base else {
                return Some(current);
            };
            current = self
                .simple
                .get(parent)
                .or_else(|| self.extra_simple.get(parent))?;
        }
    }
}

const XS: &str = "http://www.w3.org/2001/XMLSchema";

struct Parsed {
    namespace: Option<String>,
    root: Option<String>,
    types: Vec<ComplexType>,
    simple: Vec<SimpleType>,
    redefined: Vec<ComplexType>,
}

pub fn load(dir: &Path, version: SchemaVersion) -> Result<Schema, Error> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "xsd"))
        .filter(|p| file_name(p).starts_with("GAEB_DA_XML"))
        .collect();
    files.sort();
    let mut schema = Schema {
        version,
        root: String::new(),
        types: BTreeMap::new(),
        simple: BTreeMap::new(),
        extra_types: BTreeMap::new(),
        extra_simple: BTreeMap::new(),
        profiles: BTreeMap::new(),
    };
    let main_file = files
        .iter()
        .find(|p| match version {
            SchemaVersion::V31 => phase_code(p).is_none(),
            _ => file_name(p).starts_with("GAEB_DA_XML_Lib_"),
        })
        .ok_or_else(|| Error(format!("no library schema in {}", dir.display())))?;
    let main = parse_file(main_file)?;
    schema.root = main.root.clone().unwrap_or_else(|| "GAEB".to_string());
    for t in main.types {
        schema.types.insert(t.name.clone(), t);
    }
    for s in main.simple {
        schema.simple.insert(s.name.clone(), s);
    }
    let main_namespace = main.namespace.clone().unwrap_or_default();
    if version == SchemaVersion::V31 {
        for code in ["81", "82", "83", "85", "86", "87"] {
            schema.profiles.insert(
                code.to_string(),
                Profile {
                    phase: code.to_string(),
                    namespace: main_namespace.clone(),
                    restricted: BTreeMap::new(),
                },
            );
        }
    }
    for file in files.iter().filter(|p| *p != main_file) {
        let name = file_name(file);
        let parsed = parse_file(file)?;
        if name.starts_with("GAEB_DA_XML_Lib") {
            absorb_extras(&mut schema, parsed);
            continue;
        }
        let Some(code) = phase_code(file) else {
            continue;
        };
        let mut restricted = BTreeMap::new();
        for t in &parsed.redefined {
            restricted.insert(t.name.clone(), restriction_of(t, false));
        }
        if version == SchemaVersion::V31 {
            for t in &parsed.types {
                if !schema.types.contains_key(&t.name) {
                    continue;
                }
                restricted.insert(t.name.clone(), restriction_of(t, true));
            }
        }
        if schema.root.is_empty() {
            schema.root = parsed.root.clone().unwrap_or_default();
        }
        let namespace = parsed.namespace.clone().unwrap_or_default();
        if version != SchemaVersion::V31 {
            absorb_extras(&mut schema, parsed);
        }
        schema.profiles.insert(
            code.clone(),
            Profile {
                phase: code,
                namespace,
                restricted,
            },
        );
    }
    resolve_type_refs(&mut schema);
    Ok(schema)
}

fn absorb_extras(schema: &mut Schema, parsed: Parsed) {
    for t in parsed.types {
        if schema.types.contains_key(&t.name) {
            continue;
        }
        schema.extra_types.entry(t.name.clone()).or_insert(t);
    }
    for s in parsed.simple {
        if schema.simple.contains_key(&s.name) {
            continue;
        }
        schema.extra_simple.entry(s.name.clone()).or_insert(s);
    }
}

fn file_name(p: &Path) -> &str {
    p.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

/// `GAEB_DA_XML_84_3.3_2021-05.xsd` -> `84`; the 3.1 library file has none.
fn phase_code(p: &Path) -> Option<String> {
    let name = file_name(p);
    let rest = name.strip_prefix("GAEB_DA_XML_")?;
    let code: String = rest
        .chars()
        .take_while(|c| *c != '_' && *c != ' ')
        .collect();
    if code.is_empty() || code.starts_with("Lib") {
        return None;
    }
    Some(code)
}

fn resolve_type_refs(schema: &mut Schema) {
    let complex: BTreeSet<String> = schema
        .types
        .keys()
        .chain(schema.extra_types.keys())
        .cloned()
        .collect();
    let mut all: Vec<&mut ComplexType> = schema
        .types
        .values_mut()
        .chain(schema.extra_types.values_mut())
        .collect();
    for t in all.iter_mut() {
        resolve_content(&mut t.content, &complex);
    }
}

fn resolve_content(content: &mut Content, complex: &BTreeSet<String>) {
    let parts = match content {
        Content::Sequence(p) | Content::Choice(p) => p,
        Content::Empty => return,
    };
    for p in parts {
        match p {
            Particle::Element { ty, .. } => match ty {
                TypeRef::Simple(n) if complex.contains(n) => *ty = TypeRef::Complex(n.clone()),
                TypeRef::Inline(inner) => resolve_content(&mut inner.content, complex),
                _ => {}
            },
            Particle::Group { content, .. } => resolve_content(content, complex),
        }
    }
}

fn restriction_of(t: &ComplexType, zero_max_excludes: bool) -> Restriction {
    let mut r = Restriction::default();
    collect_restriction(&t.content, true, zero_max_excludes, &mut r);
    r
}

fn collect_restriction(
    content: &Content,
    parent_required: bool,
    zero_max_excludes: bool,
    r: &mut Restriction,
) {
    let (parts, is_sequence) = match content {
        Content::Sequence(p) => (p, true),
        Content::Choice(p) => (p, false),
        Content::Empty => return,
    };
    for p in parts {
        let (min, max) = p.bounds();
        if zero_max_excludes && max == Max::Bounded(0) {
            continue;
        }
        let required = parent_required && is_sequence && min >= 1;
        match p {
            Particle::Element { name, .. } => {
                r.allowed.insert(name.clone());
                if required {
                    r.required.insert(name.clone());
                }
            }
            Particle::Group { content, .. } => {
                collect_restriction(content, required, zero_max_excludes, r)
            }
        }
    }
}

fn parse_file(path: &Path) -> Result<Parsed, Error> {
    let text = std::fs::read_to_string(path)?;
    let doc =
        roxmltree::Document::parse(&text).map_err(|e| Error(format!("{}: {e}", path.display())))?;
    let root = doc.root_element();
    let mut parsed = Parsed {
        namespace: root.attribute("targetNamespace").map(str::to_string),
        root: None,
        types: Vec::new(),
        simple: Vec::new(),
        redefined: Vec::new(),
    };
    for child in root.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "complexType" => parsed.types.push(complex_type(child)),
            "simpleType" => parsed.simple.push(simple_type(child)),
            "element" => parsed.root = child.attribute("name").map(str::to_string),
            "redefine" => {
                for inner in child.children().filter(|c| c.is_element()) {
                    match inner.tag_name().name() {
                        "complexType" => parsed.redefined.push(complex_type(inner)),
                        "simpleType" => parsed.simple.push(simple_type(inner)),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(parsed)
}

fn is(node: roxmltree::Node, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace() == Some(XS) && node.tag_name().name() == name
}

fn complex_type(node: roxmltree::Node) -> ComplexType {
    let mut ty = ComplexType {
        name: node.attribute("name").unwrap_or_default().to_string(),
        content: Content::Empty,
        attributes: Vec::new(),
        text: None,
        mixed: node.attribute("mixed") == Some("true"),
    };
    fill_complex(node, &mut ty);
    ty
}

fn fill_complex(node: roxmltree::Node, ty: &mut ComplexType) {
    for child in node.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "sequence" => ty.content = Content::Sequence(particles(child)),
            "choice" => ty.content = Content::Choice(particles(child)),
            "attribute" => ty.attributes.push(attribute(child)),
            "simpleContent" => {
                for ext in child
                    .children()
                    .filter(|c| is(*c, "extension") || is(*c, "restriction"))
                {
                    ty.text = ext.attribute("base").map(str::to_string);
                    for a in ext.children().filter(|c| is(*c, "attribute")) {
                        ty.attributes.push(attribute(a));
                    }
                }
            }
            "complexContent" => {
                for inner in child
                    .children()
                    .filter(|c| is(*c, "extension") || is(*c, "restriction"))
                {
                    fill_complex(inner, ty);
                }
            }
            _ => {}
        }
    }
}

fn attribute(node: roxmltree::Node) -> Attribute {
    Attribute {
        name: element_name(node),
        ty: node.attribute("type").unwrap_or("xs:string").to_string(),
        required: node.attribute("use") == Some("required"),
        default: node.attribute("default").map(str::to_string),
    }
}

fn occurs(node: roxmltree::Node) -> (u32, Max) {
    let min = node
        .attribute("minOccurs")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let max = match node.attribute("maxOccurs") {
        Some("unbounded") => Max::Unbounded,
        Some(v) => Max::Bounded(v.parse().unwrap_or(1)),
        None => Max::Bounded(1),
    };
    (min, max)
}

fn particles(node: roxmltree::Node) -> Vec<Particle> {
    let mut out = Vec::new();
    for child in node.children().filter(|c| c.is_element()) {
        let (min, max) = occurs(child);
        match child.tag_name().name() {
            "element" => out.push(Particle::Element {
                name: element_name(child),
                ty: element_type(child),
                min,
                max,
            }),
            "sequence" => out.push(Particle::Group {
                content: Box::new(Content::Sequence(particles(child))),
                min,
                max,
            }),
            "choice" => out.push(Particle::Group {
                content: Box::new(Content::Choice(particles(child))),
                min,
                max,
            }),
            _ => {}
        }
    }
    out
}

/// A declared name, or the local part of a `ref` such as `ds:Signature`.
fn element_name(node: roxmltree::Node) -> String {
    if let Some(name) = node.attribute("name") {
        return name.to_string();
    }
    let reference = node.attribute("ref").unwrap_or_default();
    reference.rsplit(':').next().unwrap_or_default().to_string()
}

fn element_type(node: roxmltree::Node) -> TypeRef {
    if let Some(t) = node.attribute("type") {
        return TypeRef::Simple(t.to_string());
    }
    if let Some(inline) = node.children().find(|c| is(*c, "complexType")) {
        let mut ty = complex_type(inline);
        ty.name = node.attribute("name").unwrap_or_default().to_string();
        return TypeRef::Inline(Box::new(ty));
    }
    if let Some(inline) = node.children().find(|c| is(*c, "simpleType")) {
        let mut ty = simple_type(inline);
        ty.name = node.attribute("name").unwrap_or_default().to_string();
        return TypeRef::InlineSimple(Box::new(ty));
    }
    TypeRef::Simple("xs:string".to_string())
}

fn simple_type(node: roxmltree::Node) -> SimpleType {
    let mut ty = SimpleType {
        name: node.attribute("name").unwrap_or_default().to_string(),
        base: Base::Other("anySimpleType".to_string()),
        enumeration: Vec::new(),
        max_length: None,
        pattern: None,
        total_digits: None,
        fraction_digits: None,
        min_inclusive: None,
        max_inclusive: None,
    };
    for child in node.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "restriction" => {
                ty.base = base_of(child.attribute("base").unwrap_or(""));
                for facet in child.children().filter(|c| c.is_element()) {
                    let value = facet.attribute("value").unwrap_or("");
                    match facet.tag_name().name() {
                        "enumeration" => ty.enumeration.push(value.to_string()),
                        "maxLength" => ty.max_length = value.parse().ok(),
                        "pattern" => ty.pattern = Some(value.to_string()),
                        "totalDigits" => ty.total_digits = value.parse().ok(),
                        "fractionDigits" => ty.fraction_digits = value.parse().ok(),
                        "minInclusive" => ty.min_inclusive = value.parse().ok(),
                        "maxInclusive" => ty.max_inclusive = value.parse().ok(),
                        _ => {}
                    }
                }
            }
            "union" => {
                ty.base = Base::Other(format!(
                    "union:{}",
                    child.attribute("memberTypes").unwrap_or("")
                ))
            }
            _ => {}
        }
    }
    ty
}

fn base_of(name: &str) -> Base {
    match name {
        "xs:string" => Base::String,
        "xs:normalizedString" => Base::NormalizedString,
        "xs:token" => Base::Token,
        "xs:decimal" => Base::Decimal,
        "xs:integer" | "xs:int" | "xs:long" | "xs:positiveInteger" | "xs:nonNegativeInteger" => {
            Base::Integer
        }
        "xs:date" => Base::Date,
        "xs:time" => Base::Time,
        "xs:gYearMonth" => Base::GYearMonth,
        "xs:boolean" => Base::Boolean,
        "xs:ID" => Base::Id,
        "xs:IDREF" => Base::IdRef,
        "xs:base64Binary" => Base::Base64,
        other if other.starts_with("tg") => Base::Derived(other.to_string()),
        other => Base::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../standard")
    }

    fn element<'a>(ty: &'a ComplexType, name: &str) -> Option<&'a Particle> {
        ty.elements().into_iter().find(|p| p.name() == Some(name))
    }

    #[test]
    fn v33_lib_has_122_top_level_complex_types() {
        let schema = load(&standard().join("3.3"), SchemaVersion::V33).unwrap();
        assert_eq!(schema.types.len(), 122);
        assert!(schema.types.contains_key("tgItem"));
        assert_eq!(schema.root, "GAEB");
    }

    #[test]
    fn v33_item_qty_is_decimal_11_3() {
        let schema = load(&standard().join("3.3"), SchemaVersion::V33).unwrap();
        let item = &schema.types["tgItem"];
        let qty = element(item, "Qty").unwrap();
        assert_eq!(qty.type_name(), Some("tgDecimal_11_3"));
        let decimal = &schema.simple["tgDecimal_11_3"];
        assert_eq!(decimal.base, Base::Decimal);
        assert_eq!(decimal.total_digits, Some(11));
        assert_eq!(decimal.fraction_digits, Some(3));
    }

    #[test]
    fn v33_provis_is_an_enumeration() {
        let schema = load(&standard().join("3.3"), SchemaVersion::V33).unwrap();
        let provis = &schema.simple["tgProvis"];
        assert_eq!(provis.enumeration, vec!["WithoutTotal", "WithTotal"]);
        assert_eq!(
            schema.simple["tgRNoPart"].pattern.as_deref(),
            Some("(_|[0-9]|[A-Z]|[a-z]){1,14}")
        );
    }

    #[test]
    fn v33_x84_profile_allows_prices_and_requires_category_totals() {
        let schema = load(&standard().join("3.3"), SchemaVersion::V33).unwrap();
        let x84 = &schema.profiles["84"];
        assert_eq!(x84.namespace, "http://www.gaeb.de/GAEB_DA_XML/DA84/3.3");
        let item = &x84.restricted["tgItem"];
        assert!(item.allowed.contains("UP"));
        assert!(item.allowed.contains("IT"));
        assert!(!item.allowed.contains("Provis"));
        assert!(!item.required.contains("UP"));
        let ctgy = &x84.restricted["tgBoQCtgy"];
        assert!(ctgy.required.contains("Totals"));
    }

    #[test]
    fn v33_has_all_procurement_profiles() {
        let schema = load(&standard().join("3.3"), SchemaVersion::V33).unwrap();
        for code in [
            "80", "81", "82", "83", "84", "85", "86", "87", "89", "89B", "83Z", "31", "93",
        ] {
            assert!(schema.profiles.contains_key(code), "missing profile {code}");
        }
    }

    #[test]
    fn v32_loads_eight_profiles() {
        let schema = load(&standard().join("3.2"), SchemaVersion::V32).unwrap();
        assert_eq!(schema.profiles.len(), 8);
        assert_eq!(
            schema.profiles["83"].namespace,
            "http://www.gaeb.de/GAEB_DA_XML/DA83/3.2"
        );
        assert_eq!(schema.types.len(), 120);
    }

    #[test]
    fn v31_single_schema_loads() {
        let schema = load(&standard().join("3.1"), SchemaVersion::V31).unwrap();
        assert_eq!(schema.types.len(), 72);
        let x84 = &schema.profiles["84"];
        assert_eq!(x84.namespace, "http://www.gaeb.de/GAEB_DA_XML/200407");
        let item = &x84.restricted["tgItem"];
        assert!(item.allowed.contains("UP"));
        assert!(!item.allowed.contains("Provis"));
        assert!(schema.profiles.contains_key("83"));
        assert!(schema.profiles["83"].restricted.is_empty());
    }

    #[test]
    fn simple_content_carries_text_and_attributes() {
        let schema = load(&standard().join("3.3"), SchemaVersion::V33).unwrap();
        let span = &schema.types["tgspan"];
        assert_eq!(span.text.as_deref(), Some("tgNormalizedString"));
        assert!(span.attributes.iter().any(|a| a.name == "style"));
        let item = &schema.types["tgItem"];
        let id = item.attributes.iter().find(|a| a.name == "ID").unwrap();
        assert!(id.required);
        assert_eq!(id.ty, "xs:ID");
    }

    #[test]
    fn inline_types_and_groups_are_modelled() {
        let schema = load(&standard().join("3.3"), SchemaVersion::V33).unwrap();
        let ctlg = &schema.types["tgCtlg"];
        let assign = element(ctlg, "CtlgAssignType").unwrap();
        assert!(matches!(assign.type_ref(), Some(TypeRef::InlineSimple(_))));
        let item = &schema.types["tgItem"];
        let parts = match &item.content {
            Content::Sequence(parts) => Some(parts),
            _ => None,
        }
        .unwrap();
        assert!(parts.iter().any(|p| matches!(p, Particle::Group { .. })));
        let bkdn = element(&schema.types["tgBoQInfo"], "BoQBkdn").unwrap();
        assert_eq!(bkdn.bounds(), (1, Max::Bounded(7)));
    }
}
