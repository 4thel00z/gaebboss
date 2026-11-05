//! Python bindings for gaebboss, compiled as `gaebboss._gaebboss` and
//! re-exported by the `gaebboss` package. Documents are immutable and
//! shareable across threads; reading, item walks, validation, writing and
//! bidding run with the GIL released.
//! Every GAEB decimal crosses as `decimal.Decimal`, never as a float.

use std::path::PathBuf;
use std::sync::Arc;

use pyo3::buffer::PyBuffer;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock as OnceLock;
use pyo3::types::{PyDict, PyFloat};

use gaebboss_core::Document;
use gaebboss_core::bid::{Bid, Bidder};
use gaebboss_core::boq::{AddTextRef, CategoryRef, Entry, ItemKind, ItemRef, MarkupRef, RemarkRef};
use gaebboss_core::decimal::Decimal;
use gaebboss_core::json::{kind_name, to_json};
use gaebboss_core::model::BoQCtgy;
use gaebboss_core::oz::Breakdown;
use gaebboss_core::validate::{Finding, validate, validate_against_tender, validate_with_tender};
use gaebboss_core::version::{Phase, Version};
use gaebboss_core::write::WriteOptions;

create_exception!(
    gaebboss,
    GaebError,
    PyException,
    "Raised for any GAEB processing error: unreadable input, unknown items, invalid bids, write failures."
);

fn gaeb_err(e: impl std::fmt::Display) -> PyErr {
    GaebError::new_err(e.to_string())
}

static DECIMAL: OnceLock<Py<PyAny>> = OnceLock::new();

/// A GAEB decimal as `decimal.Decimal`, built from its exact text.
fn decimal(py: Python<'_>, value: Option<Decimal>) -> PyResult<Option<Py<PyAny>>> {
    let Some(value) = value else { return Ok(None) };
    let class = DECIMAL.get_or_try_init(py, || {
        py.import("decimal")?.getattr("Decimal").map(Bound::unbind)
    })?;
    Ok(Some(class.bind(py).call1((value.to_string(),))?.unbind()))
}

/// A Python number as a GAEB decimal: `decimal.Decimal`, `str` or `int`;
/// floats are refused because they are not exact.
fn decimal_arg(value: &Bound<'_, PyAny>) -> PyResult<Decimal> {
    if value.is_instance_of::<PyFloat>() {
        return Err(gaeb_err("pass a decimal.Decimal, str or int, not a float"));
    }
    let text = value.str()?.to_str()?.to_string();
    Decimal::parse(text.trim()).map_err(|e| gaeb_err(format!("{text:?}: {e}")))
}

/// The bytes of a buffer-like argument in one copy: `bytes`, `bytearray`,
/// `memoryview` or any other buffer object. A plain sequence of ints is
/// extracted element by element.
fn byte_arg(data: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let Ok(buffer) = PyBuffer::<u8>::get(data) else {
        return data.extract();
    };
    buffer.to_vec(data.py())
}

#[pyclass(frozen, name = "Warning", module = "gaebboss")]
struct PyWarning {
    #[pyo3(get)]
    code: String,
    #[pyo3(get)]
    offset: u32,
    #[pyo3(get)]
    message: String,
}

#[pymethods]
impl PyWarning {
    fn __repr__(&self) -> String {
        format!(
            "Warning({} at {}: {})",
            self.code, self.offset, self.message
        )
    }
}

#[pyclass(frozen, name = "Project", module = "gaebboss")]
struct PyProject {
    #[pyo3(get)]
    name: Option<String>,
    #[pyo3(get)]
    label: Option<String>,
    #[pyo3(get)]
    currency: Option<String>,
}

#[pyclass(frozen, name = "Level", module = "gaebboss")]
struct PyLevel {
    #[pyo3(get, name = "type")]
    kind: String,
    #[pyo3(get)]
    label: Option<String>,
    #[pyo3(get)]
    length: usize,
    #[pyo3(get)]
    numeric: bool,
    #[pyo3(get)]
    alignment: Option<String>,
}

#[pyclass(frozen, name = "Boq", module = "gaebboss")]
struct PyBoq {
    #[pyo3(get)]
    id: Option<String>,
    #[pyo3(get)]
    name: Option<String>,
    #[pyo3(get)]
    label: Option<String>,
    #[pyo3(get)]
    breakdown: Vec<Py<PyLevel>>,
    #[pyo3(get)]
    total: Option<Py<PyAny>>,
}

#[derive(Clone)]
#[pyclass(frozen, name = "Complement", module = "gaebboss")]
struct PyComplement {
    #[pyo3(get)]
    mark: Option<i64>,
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    label: String,
    #[pyo3(get)]
    value: String,
    #[pyo3(get)]
    empty: bool,
    #[pyo3(get)]
    tail: String,
    number: Option<Decimal>,
}

#[pymethods]
impl PyComplement {
    #[getter]
    fn number(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.number)
    }
}

/// The dotted Ordnungszahl of every category on `path`, outermost first.
fn category_ozs(doc: &Document, breakdown: &Breakdown, path: &[&BoQCtgy]) -> Vec<String> {
    let parts: Vec<&str> = path
        .iter()
        .map(|c| c.rno_part.map(|s| doc.str(s)).unwrap_or(""))
        .collect();
    (1..=parts.len())
        .map(|depth| breakdown.category_oz(&parts[..depth]).dotted())
        .collect()
}

#[pyclass(frozen, name = "Item", module = "gaebboss")]
struct PyItem {
    #[pyo3(get)]
    id: String,
    #[pyo3(get)]
    oz: String,
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    alternative_group: Option<i64>,
    #[pyo3(get)]
    alternative_serial: Option<i64>,
    #[pyo3(get)]
    short_text: String,
    #[pyo3(get)]
    long_text: String,
    #[pyo3(get)]
    long_text_markdown: String,
    quantity: Option<Decimal>,
    #[pyo3(get)]
    unit: Option<String>,
    unit_price: Option<Decimal>,
    total: Option<Decimal>,
    vat: Option<Decimal>,
    #[pyo3(get)]
    counts_towards_total: bool,
    #[pyo3(get)]
    not_offered: bool,
    #[pyo3(get)]
    category_ozs: Vec<String>,
    complements: Vec<PyComplement>,
}

impl PyItem {
    fn new(doc: &Document, breakdown: &Breakdown, i: &ItemRef<'_>) -> PyItem {
        let item_kind = i.kind();
        let (group, serial) = match item_kind {
            ItemKind::Alternative { group, serial } => (Some(group), Some(serial)),
            ItemKind::Base { group } => (Some(group), Some(0)),
            _ => (None, None),
        };
        let complements = i
            .complements()
            .iter()
            .map(|c| PyComplement {
                mark: c.mark,
                kind: format!("{:?}", c.kind),
                label: c.label().to_string(),
                value: c.value(),
                empty: c.empty,
                tail: c.tail.clone(),
                number: c.number,
            })
            .collect();
        PyItem {
            id: i.id().to_string(),
            oz: i.oz.dotted(),
            kind: kind_name(item_kind).to_string(),
            alternative_group: group,
            alternative_serial: serial,
            short_text: i.short_text(),
            long_text: i.long_text(),
            long_text_markdown: i.long_text_markdown(),
            quantity: i.quantity(),
            unit: i.unit().map(str::to_string),
            unit_price: i.unit_price(),
            total: i.total(),
            vat: i.vat_percent(),
            counts_towards_total: i.counts_towards_total(),
            not_offered: i.not_offered(),
            category_ozs: category_ozs(doc, breakdown, &i.path),
            complements,
        }
    }
}

#[pymethods]
impl PyItem {
    #[getter]
    fn quantity(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.quantity)
    }

    #[getter]
    fn unit_price(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.unit_price)
    }

    #[getter]
    fn total(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.total)
    }

    #[getter]
    fn vat(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.vat)
    }

    #[getter]
    fn complements(&self) -> Vec<PyComplement> {
        self.complements.clone()
    }

    fn __repr__(&self) -> String {
        format!("Item({} {} {:?})", self.oz, self.kind, self.short_text)
    }
}

#[pyclass(frozen, name = "Category", module = "gaebboss")]
struct PyCategory {
    #[pyo3(get)]
    oz: String,
    #[pyo3(get)]
    label: String,
    #[pyo3(get)]
    depth: usize,
    total: Option<Decimal>,
}

impl PyCategory {
    fn new(c: CategoryRef<'_>) -> PyCategory {
        PyCategory {
            oz: c.oz.dotted(),
            total: c.ctgy.totals.as_ref().and_then(|t| t.total),
            label: c.label,
            depth: c.depth,
        }
    }
}

#[pymethods]
impl PyCategory {
    #[getter]
    fn total(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.total)
    }
}

#[pyclass(frozen, name = "MarkupItem", module = "gaebboss")]
struct PyMarkupItem {
    #[pyo3(get)]
    id: String,
    #[pyo3(get)]
    oz: String,
    #[pyo3(get)]
    short_text: String,
    #[pyo3(get)]
    long_text: String,
    #[pyo3(get)]
    long_text_markdown: String,
    base: Option<Decimal>,
    percent: Option<Decimal>,
    total: Option<Decimal>,
    #[pyo3(get)]
    category_ozs: Vec<String>,
}

impl PyMarkupItem {
    fn new(doc: &Document, breakdown: &Breakdown, m: &MarkupRef<'_>) -> PyMarkupItem {
        PyMarkupItem {
            id: m.id().to_string(),
            oz: m.oz.dotted(),
            short_text: m.short_text(),
            long_text: m.long_text(),
            long_text_markdown: m.long_text_markdown(),
            base: m.base(),
            percent: m.percent(),
            total: m.total(),
            category_ozs: category_ozs(doc, breakdown, &m.path),
        }
    }
}

#[pymethods]
impl PyMarkupItem {
    #[getter]
    fn base(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.base)
    }

    #[getter]
    fn percent(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.percent)
    }

    #[getter]
    fn total(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        decimal(py, self.total)
    }
}

#[pyclass(frozen, name = "Remark", module = "gaebboss")]
struct PyRemark {
    #[pyo3(get)]
    id: String,
    #[pyo3(get)]
    short_text: String,
    #[pyo3(get)]
    long_text: String,
    #[pyo3(get)]
    long_text_markdown: String,
    #[pyo3(get)]
    category_ozs: Vec<String>,
}

impl PyRemark {
    fn new(doc: &Document, breakdown: &Breakdown, r: &RemarkRef<'_>) -> PyRemark {
        PyRemark {
            id: r.id().to_string(),
            short_text: r.short_text(),
            long_text: r.long_text(),
            long_text_markdown: r.long_text_markdown(),
            category_ozs: category_ozs(doc, breakdown, &r.path),
        }
    }
}

#[pyclass(frozen, name = "AddText", module = "gaebboss")]
struct PyAddText {
    #[pyo3(get)]
    short_text: String,
    #[pyo3(get)]
    long_text: String,
    #[pyo3(get)]
    long_text_markdown: String,
}

impl PyAddText {
    fn new(t: &AddTextRef<'_>) -> PyAddText {
        PyAddText {
            short_text: t.short_text(),
            long_text: t.long_text(),
            long_text_markdown: t.long_text_markdown(),
        }
    }
}

/// One node of the bill as its Python class.
#[derive(IntoPyObject)]
enum PyEntry {
    Category(PyCategory),
    Item(PyItem),
    Markup(PyMarkupItem),
    Remark(PyRemark),
}

#[pyclass(frozen, name = "Finding", module = "gaebboss")]
struct PyFinding {
    #[pyo3(get)]
    severity: String,
    #[pyo3(get)]
    code: String,
    #[pyo3(get)]
    oz: Option<String>,
    #[pyo3(get)]
    message: String,
}

#[pymethods]
impl PyFinding {
    fn __repr__(&self) -> String {
        format!(
            "Finding({} {}{}: {})",
            self.severity,
            self.code,
            self.oz
                .as_deref()
                .map(|o| format!(" [{o}]"))
                .unwrap_or_default(),
            self.message
        )
    }
}

#[pyclass(frozen, name = "Dropped", module = "gaebboss")]
struct PyDropped {
    #[pyo3(get)]
    type_name: String,
    #[pyo3(get)]
    element: String,
    #[pyo3(get)]
    count: usize,
}

#[pyclass(frozen, name = "Missing", module = "gaebboss")]
struct PyMissing {
    #[pyo3(get)]
    type_name: String,
    #[pyo3(get)]
    element: String,
}

#[pyclass(frozen, name = "WriteReport", module = "gaebboss")]
struct PyWriteReport {
    #[pyo3(get)]
    dropped: Vec<Py<PyDropped>>,
    #[pyo3(get)]
    missing: Vec<Py<PyMissing>>,
}

fn finding(f: Finding) -> PyFinding {
    PyFinding {
        severity: format!("{:?}", f.severity),
        code: format!("{:?}", f.code),
        oz: f.oz,
        message: f.message,
    }
}

fn phase_label(phase: Option<Phase>) -> Option<String> {
    phase.map(|p| format!("X{}", p.code()))
}

fn parse_phase(text: &str) -> PyResult<Phase> {
    Phase::from_extension(text)
        .or_else(|| Phase::from_code(text))
        .ok_or_else(|| gaeb_err(format!("unknown phase {text}")))
}

fn parse_version(text: &str) -> PyResult<Version> {
    Version::parse(text).ok_or_else(|| gaeb_err(format!("unknown version {text}")))
}

#[pyclass(frozen, name = "Document", module = "gaebboss")]
struct PyDocument {
    inner: Arc<Document>,
}

impl PyDocument {
    fn options(
        &self,
        phase: Option<&str>,
        version: Option<&str>,
        pretty: bool,
        strict: bool,
    ) -> PyResult<WriteOptions> {
        let phase = match phase {
            Some(p) => parse_phase(p)?,
            None => self
                .inner
                .phase()
                .ok_or_else(|| gaeb_err("the document names no phase; pass phase="))?,
        };
        let version = match version {
            Some(v) => parse_version(v)?,
            None => self.inner.version(),
        };
        Ok(WriteOptions {
            version,
            phase,
            indent: pretty.then_some(2),
            prog_system: None,
            strict,
        })
    }
}

#[pymethods]
impl PyDocument {
    /// `Document(path)` or `Document(data=bytes, name="x.X83")`.
    #[new]
    #[pyo3(signature = (path=None, *, data=None, name=None))]
    fn new(
        py: Python<'_>,
        path: Option<PathBuf>,
        data: Option<&Bound<'_, PyAny>>,
        name: Option<String>,
    ) -> PyResult<Self> {
        let doc = match (path, data) {
            (Some(path), None) => {
                let file_name = name.or_else(|| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_string)
                });
                py.detach(move || {
                    let bytes = std::fs::read(&path)
                        .map_err(|e| gaeb_err(format!("{}: {e}", path.display())))?;
                    Document::parse_named(&bytes, file_name.as_deref()).map_err(gaeb_err)
                })?
            }
            (None, Some(data)) => {
                let bytes = byte_arg(data)?;
                py.detach(move || Document::parse_named(&bytes, name.as_deref()))
                    .map_err(gaeb_err)?
            }
            _ => return Err(gaeb_err("pass either a path or data=")),
        };
        Ok(PyDocument {
            inner: Arc::new(doc),
        })
    }

    #[getter]
    fn version(&self) -> String {
        self.inner.version().as_str().to_string()
    }

    #[getter]
    fn phase(&self) -> Option<String> {
        phase_label(self.inner.phase())
    }

    #[getter]
    fn warnings(&self) -> Vec<PyWarning> {
        self.inner
            .warnings
            .iter()
            .map(|w| PyWarning {
                code: format!("{:?}", w.code),
                offset: w.offset,
                message: w.message.clone(),
            })
            .collect()
    }

    #[getter]
    fn project(&self) -> PyProject {
        let doc = &self.inner;
        let p = doc.gaeb.prj_info.as_ref();
        PyProject {
            name: p.and_then(|p| p.name_prj).map(|s| doc.str(s).to_string()),
            label: p.and_then(|p| p.lbl_prj).map(|s| doc.str(s).to_string()),
            currency: p.and_then(|p| p.cur).map(|s| doc.str(s).to_string()),
        }
    }

    #[getter]
    fn boq(&self, py: Python<'_>) -> PyResult<Option<PyBoq>> {
        let doc = &self.inner;
        let Some(boq) = doc.boq() else {
            return Ok(None);
        };
        let info = boq.boq_info.as_ref();
        let breakdown = doc
            .breakdown()
            .levels()
            .iter()
            .map(|l| {
                Py::new(
                    py,
                    PyLevel {
                        kind: format!("{:?}", l.kind),
                        label: l.label.clone(),
                        length: l.length,
                        numeric: l.numeric,
                        alignment: l.alignment.map(|a| format!("{a:?}").to_ascii_lowercase()),
                    },
                )
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Some(PyBoq {
            id: boq.id.map(|s| doc.str(s).to_string()),
            name: info.and_then(|i| i.name).map(|s| doc.str(s).to_string()),
            label: info.and_then(|i| i.lbl_boq).map(|s| doc.str(s).to_string()),
            breakdown,
            total: decimal(py, doc.declared_total())?,
        }))
    }

    /// Every item in document order; `kind="counting"` keeps only those
    /// whose total belongs in the sums.
    #[pyo3(signature = (kind="all"))]
    fn items(&self, py: Python<'_>, kind: &str) -> PyResult<Vec<PyItem>> {
        let counting = match kind {
            "all" => false,
            "counting" => true,
            other => {
                return Err(gaeb_err(format!(
                    "kind must be \"all\" or \"counting\", not {other:?}"
                )));
            }
        };
        let doc = Arc::clone(&self.inner);
        Ok(py.detach(move || {
            let breakdown = doc.breakdown();
            doc.items()
                .filter(|i| !counting || i.counts_towards_total())
                .map(|i| PyItem::new(&doc, &breakdown, &i))
                .collect()
        }))
    }

    /// Categories, items, markup items and remarks in document order, each
    /// category before what it contains.
    fn entries(&self, py: Python<'_>) -> Vec<PyEntry> {
        let doc = Arc::clone(&self.inner);
        py.detach(move || {
            let breakdown = doc.breakdown();
            doc.entries()
                .map(|entry| match entry {
                    Entry::Category(c) => PyEntry::Category(PyCategory::new(c)),
                    Entry::Item(i) => PyEntry::Item(PyItem::new(&doc, &breakdown, &i)),
                    Entry::Markup(m) => PyEntry::Markup(PyMarkupItem::new(&doc, &breakdown, &m)),
                    Entry::Remark(r) => PyEntry::Remark(PyRemark::new(&doc, &breakdown, &r)),
                })
                .collect()
        })
    }

    /// The award's additional texts, which come before the bill: in a tender,
    /// its preliminary notes.
    fn award_add_texts(&self, py: Python<'_>) -> Vec<PyAddText> {
        let doc = Arc::clone(&self.inner);
        py.detach(move || doc.award_add_texts().map(|t| PyAddText::new(&t)).collect())
    }

    /// The file's additional texts, which come after the award.
    fn gaeb_add_texts(&self, py: Python<'_>) -> Vec<PyAddText> {
        let doc = Arc::clone(&self.inner);
        py.detach(move || doc.gaeb_add_texts().map(|t| PyAddText::new(&t)).collect())
    }

    fn categories(&self, py: Python<'_>) -> Vec<PyCategory> {
        let doc = Arc::clone(&self.inner);
        py.detach(move || doc.categories().map(PyCategory::new).collect())
    }

    fn markup_items(&self, py: Python<'_>) -> Vec<PyMarkupItem> {
        let doc = Arc::clone(&self.inner);
        py.detach(move || {
            let breakdown = doc.breakdown();
            doc.markup_items()
                .map(|m| PyMarkupItem::new(&doc, &breakdown, &m))
                .collect()
        })
    }

    /// The document as a JSON string; decimals are strings.
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let doc = Arc::clone(&self.inner);
        py.detach(move || serde_json::to_string(&to_json(&doc)))
            .map_err(gaeb_err)
    }

    /// Every rule the document can be checked against; with the tender a
    /// bid answers, its totals follow the tender's alternative and
    /// not-applicable items and categories, and it is checked against the
    /// tender as well.
    #[pyo3(signature = (tender=None))]
    fn validate(&self, py: Python<'_>, tender: Option<&PyDocument>) -> Vec<PyFinding> {
        let doc = Arc::clone(&self.inner);
        let Some(tender) = tender else {
            return py.detach(move || validate(&doc).into_iter().map(finding).collect());
        };
        let tender = Arc::clone(&tender.inner);
        py.detach(move || {
            validate_with_tender(&doc, &tender)
                .into_iter()
                .map(finding)
                .collect()
        })
    }

    fn validate_against(&self, py: Python<'_>, tender: &PyDocument) -> Vec<PyFinding> {
        let (doc, tender) = (Arc::clone(&self.inner), Arc::clone(&tender.inner));
        py.detach(move || {
            validate_against_tender(&doc, &tender)
                .into_iter()
                .map(finding)
                .collect()
        })
    }

    /// The document as GAEB DA XML bytes, in its own phase and version
    /// unless told otherwise.
    #[pyo3(signature = (*, phase=None, version=None, pretty=false, strict=false))]
    fn write(
        &self,
        py: Python<'_>,
        phase: Option<&str>,
        version: Option<&str>,
        pretty: bool,
        strict: bool,
    ) -> PyResult<Vec<u8>> {
        let opts = self.options(phase, version, pretty, strict)?;
        let doc = Arc::clone(&self.inner);
        py.detach(move || doc.write(&opts)).map_err(gaeb_err)
    }

    /// What writing would leave out or find missing.
    #[pyo3(signature = (*, phase=None, version=None, strict=false))]
    fn write_report(
        &self,
        py: Python<'_>,
        phase: Option<&str>,
        version: Option<&str>,
        strict: bool,
    ) -> PyResult<PyWriteReport> {
        let opts = self.options(phase, version, false, strict)?;
        let doc = Arc::clone(&self.inner);
        let (_, report) = py
            .detach(move || doc.write_with_report(&opts))
            .map_err(gaeb_err)?;
        Ok(PyWriteReport {
            dropped: report
                .dropped
                .into_iter()
                .map(|d| {
                    Py::new(
                        py,
                        PyDropped {
                            type_name: d.type_name.to_string(),
                            element: d.element,
                            count: d.count,
                        },
                    )
                })
                .collect::<PyResult<_>>()?,
            missing: report
                .missing
                .into_iter()
                .map(|m| {
                    Py::new(
                        py,
                        PyMissing {
                            type_name: m.type_name.to_string(),
                            element: m.element.as_str().to_string(),
                        },
                    )
                })
                .collect::<PyResult<_>>()?,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Document(version={}, phase={})",
            self.inner.version().as_str(),
            phase_label(self.inner.phase()).unwrap_or_else(|| "?".to_string())
        )
    }
}

#[pyclass(name = "Bid", module = "gaebboss")]
struct PyBid {
    inner: Option<Bid>,
}

impl PyBid {
    fn bid(&mut self) -> PyResult<&mut Bid> {
        self.inner
            .as_mut()
            .ok_or_else(|| gaeb_err("this bid is finished"))
    }
}

#[pymethods]
impl PyBid {
    /// Starts a bid from a tender; `bidder` holds name1, street, pcode,
    /// city and optionally name2, contact, email, bidder_no.
    #[staticmethod]
    fn from_tender(
        py: Python<'_>,
        tender: &PyDocument,
        bidder: &Bound<'_, PyDict>,
    ) -> PyResult<PyBid> {
        let field = |key: &str| -> PyResult<Option<String>> {
            bidder
                .get_item(key)?
                .map(|v| v.extract::<String>())
                .transpose()
        };
        let required = |key: &str| -> PyResult<String> {
            field(key)?.ok_or_else(|| gaeb_err(format!("bidder needs {key}")))
        };
        let bidder = Bidder {
            name1: required("name1")?,
            name2: field("name2")?,
            street: required("street")?,
            pcode: required("pcode")?,
            city: required("city")?,
            contact: field("contact")?,
            email: field("email")?,
            bidder_no: field("bidder_no")?,
        };
        let tender = Arc::clone(&tender.inner);
        let bid = py
            .detach(move || Bid::from_tender(&tender, bidder))
            .map_err(gaeb_err)?;
        Ok(PyBid { inner: Some(bid) })
    }

    fn price(&mut self, id: &str, up: &Bound<'_, PyAny>) -> PyResult<()> {
        let up = decimal_arg(up)?;
        self.bid()?.price(id, up).map_err(gaeb_err)
    }

    fn price_components(&mut self, id: &str, components: Vec<Bound<'_, PyAny>>) -> PyResult<()> {
        let values = components
            .iter()
            .map(decimal_arg)
            .collect::<PyResult<Vec<_>>>()?;
        self.bid()?.price_components(id, &values).map_err(gaeb_err)
    }

    fn not_offered(&mut self, id: &str) -> PyResult<()> {
        self.bid()?.not_offered(id).map_err(gaeb_err)
    }

    fn free_quantity(&mut self, id: &str, qty: &Bound<'_, PyAny>) -> PyResult<()> {
        let qty = decimal_arg(qty)?;
        self.bid()?.free_quantity(id, qty).map_err(gaeb_err)
    }

    fn fill_complement(&mut self, id: &str, mark: i64, text: &str) -> PyResult<()> {
        self.bid()?
            .fill_complement(id, mark, text)
            .map_err(gaeb_err)
    }

    fn markup(
        &mut self,
        id: &str,
        base: &Bound<'_, PyAny>,
        percent: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let (base, percent) = (decimal_arg(base)?, decimal_arg(percent)?);
        self.bid()?.markup(id, base, percent).map_err(gaeb_err)
    }

    fn discount_percent(&mut self, percent: &Bound<'_, PyAny>) -> PyResult<()> {
        let percent = decimal_arg(percent)?;
        self.bid()?.discount_percent(percent);
        Ok(())
    }

    fn vat_percent(&mut self, percent: &Bound<'_, PyAny>) -> PyResult<()> {
        let percent = decimal_arg(percent)?;
        self.bid()?.vat_percent(percent);
        Ok(())
    }

    /// Checks every item is priced, computes all totals and returns the
    /// finished X84 document. The bid cannot be used afterwards.
    fn finish(&mut self, py: Python<'_>) -> PyResult<PyDocument> {
        let bid = self
            .inner
            .take()
            .ok_or_else(|| gaeb_err("this bid is finished"))?;
        let doc = py.detach(move || bid.finish()).map_err(gaeb_err)?;
        Ok(PyDocument {
            inner: Arc::new(doc),
        })
    }
}

#[pymodule(name = "_gaebboss")]
fn gaebboss(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("GaebError", m.py().get_type::<GaebError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyBid>()?;
    m.add_class::<PyItem>()?;
    m.add_class::<PyCategory>()?;
    m.add_class::<PyMarkupItem>()?;
    m.add_class::<PyRemark>()?;
    m.add_class::<PyAddText>()?;
    m.add_class::<PyComplement>()?;
    m.add_class::<PyBoq>()?;
    m.add_class::<PyLevel>()?;
    m.add_class::<PyProject>()?;
    m.add_class::<PyWarning>()?;
    m.add_class::<PyFinding>()?;
    m.add_class::<PyWriteReport>()?;
    m.add_class::<PyDropped>()?;
    m.add_class::<PyMissing>()?;
    Ok(())
}
