//! Python bindings for gaebboss, compiled as `gaebboss._gaebboss` and
//! re-exported by the `gaebboss` package. Documents are immutable and
//! shareable across threads; parsing and writing run with the GIL released.
//! Every GAEB decimal crosses as `decimal.Decimal`, never as a float.

use std::path::PathBuf;
use std::sync::Arc;

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock as OnceLock;
use pyo3::types::{PyDict, PyFloat};

use gaebboss_core::Document;
use gaebboss_core::bid::{Bid, Bidder};
use gaebboss_core::boq::ItemKind;
use gaebboss_core::decimal::Decimal;
use gaebboss_core::json::{kind_name, to_json};
use gaebboss_core::validate::{Finding, validate, validate_against_tender};
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
    #[pyo3(get)]
    number: Option<Py<PyAny>>,
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
    #[pyo3(get)]
    quantity: Option<Py<PyAny>>,
    #[pyo3(get)]
    unit: Option<String>,
    #[pyo3(get)]
    unit_price: Option<Py<PyAny>>,
    #[pyo3(get)]
    total: Option<Py<PyAny>>,
    #[pyo3(get)]
    vat: Option<Py<PyAny>>,
    #[pyo3(get)]
    counts_towards_total: bool,
    #[pyo3(get)]
    not_offered: bool,
    #[pyo3(get)]
    category_ozs: Vec<String>,
    #[pyo3(get)]
    complements: Vec<Py<PyComplement>>,
}

#[pymethods]
impl PyItem {
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
    #[pyo3(get)]
    total: Option<Py<PyAny>>,
}

#[pyclass(frozen, name = "MarkupItem", module = "gaebboss")]
struct PyMarkupItem {
    #[pyo3(get)]
    id: String,
    #[pyo3(get)]
    oz: String,
    #[pyo3(get)]
    base: Option<Py<PyAny>>,
    #[pyo3(get)]
    percent: Option<Py<PyAny>>,
    #[pyo3(get)]
    total: Option<Py<PyAny>>,
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
        data: Option<Vec<u8>>,
        name: Option<String>,
    ) -> PyResult<Self> {
        let (bytes, file_name) = match (path, data) {
            (Some(path), None) => {
                let bytes = std::fs::read(&path)
                    .map_err(|e| gaeb_err(format!("{}: {e}", path.display())))?;
                let file_name = name.or_else(|| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_string)
                });
                (bytes, file_name)
            }
            (None, Some(data)) => (data, name),
            _ => return Err(gaeb_err("pass either a path or data=")),
        };
        let doc = py
            .detach(move || Document::parse_named(&bytes, file_name.as_deref()))
            .map_err(gaeb_err)?;
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
        let doc = &self.inner;
        let mut out = Vec::new();
        for i in doc.items() {
            if counting && !i.counts_towards_total() {
                continue;
            }
            let item_kind = i.kind();
            let (group, serial) = match item_kind {
                ItemKind::Alternative { group, serial } => (Some(group), Some(serial)),
                ItemKind::Base { group } => (Some(group), Some(0)),
                _ => (None, None),
            };
            let complements = i
                .complements()
                .iter()
                .map(|c| {
                    Py::new(
                        py,
                        PyComplement {
                            mark: c.mark,
                            kind: format!("{:?}", c.kind),
                            label: c.label().to_string(),
                            value: c.value(),
                            empty: c.empty,
                            tail: c.tail.clone(),
                            number: decimal(py, c.number)?,
                        },
                    )
                })
                .collect::<PyResult<Vec<_>>>()?;
            let category_ozs = {
                let breakdown = doc.breakdown();
                let mut parts: Vec<&str> = Vec::new();
                let mut ozs = Vec::new();
                for c in &i.path {
                    parts.push(c.rno_part.map(|s| doc.str(s)).unwrap_or(""));
                    ozs.push(breakdown.category_oz(&parts).dotted());
                }
                ozs
            };
            out.push(PyItem {
                id: i.id().to_string(),
                oz: i.oz.dotted(),
                kind: kind_name(item_kind).to_string(),
                alternative_group: group,
                alternative_serial: serial,
                short_text: i.short_text(),
                long_text: i.long_text(),
                long_text_markdown: i.long_text_markdown(),
                quantity: decimal(py, i.quantity())?,
                unit: i.unit().map(str::to_string),
                unit_price: decimal(py, i.unit_price())?,
                total: decimal(py, i.total())?,
                vat: decimal(py, i.vat_percent())?,
                counts_towards_total: i.counts_towards_total(),
                not_offered: i.not_offered(),
                category_ozs,
                complements,
            });
        }
        Ok(out)
    }

    fn categories(&self, py: Python<'_>) -> PyResult<Vec<PyCategory>> {
        self.inner
            .categories()
            .map(|c| {
                Ok(PyCategory {
                    oz: c.oz.dotted(),
                    label: c.label,
                    depth: c.depth,
                    total: decimal(py, c.ctgy.totals.as_ref().and_then(|t| t.total))?,
                })
            })
            .collect()
    }

    fn markup_items(&self, py: Python<'_>) -> PyResult<Vec<PyMarkupItem>> {
        self.inner
            .markup_items()
            .map(|m| {
                Ok(PyMarkupItem {
                    id: m.id().to_string(),
                    oz: m.oz.dotted(),
                    base: decimal(py, m.base())?,
                    percent: decimal(py, m.percent())?,
                    total: decimal(py, m.total())?,
                })
            })
            .collect()
    }

    /// The document as a JSON string; decimals are strings.
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let doc = Arc::clone(&self.inner);
        py.detach(move || serde_json::to_string(&to_json(&doc)))
            .map_err(gaeb_err)
    }

    fn validate(&self) -> Vec<PyFinding> {
        validate(&self.inner).into_iter().map(finding).collect()
    }

    fn validate_against(&self, tender: &PyDocument) -> Vec<PyFinding> {
        validate_against_tender(&self.inner, &tender.inner)
            .into_iter()
            .map(finding)
            .collect()
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
        let (_, report) = self.inner.write_with_report(&opts).map_err(gaeb_err)?;
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
    fn from_tender(tender: &PyDocument, bidder: &Bound<'_, PyDict>) -> PyResult<PyBid> {
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
        let bid = Bid::from_tender(&tender.inner, bidder).map_err(gaeb_err)?;
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
    fn finish(&mut self) -> PyResult<PyDocument> {
        let bid = self
            .inner
            .take()
            .ok_or_else(|| gaeb_err("this bid is finished"))?;
        let doc = bid.finish().map_err(gaeb_err)?;
        Ok(PyDocument {
            inner: Arc::new(doc),
        })
    }
}

#[pymodule]
fn _gaebboss(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("GaebError", m.py().get_type::<GaebError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyBid>()?;
    m.add_class::<PyItem>()?;
    m.add_class::<PyCategory>()?;
    m.add_class::<PyMarkupItem>()?;
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
