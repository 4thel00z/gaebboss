//! The gaebboss command line: inspect, list, export, validate, round-trip
//! and price GAEB DA XML files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use gaebboss_core::Document;
use gaebboss_core::bid::{Bid, Bidder};
use gaebboss_core::decimal::Decimal;
use gaebboss_core::json::{kind_name, to_json};
use gaebboss_core::validate::{Severity, validate, validate_against_tender};
use gaebboss_core::version::{Phase, Version};
use gaebboss_core::write::WriteOptions;

#[derive(Parser)]
#[command(
    name = "gaebboss",
    version,
    about = "GAEB DA XML: read, validate, write and price"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Version, phase, project, counts, totals and warnings
    Info { file: PathBuf },
    /// One row per item
    Items {
        file: PathBuf,
        /// Only items that count towards the total
        #[arg(long)]
        counting: bool,
        /// Tab-separated with a header line
        #[arg(long)]
        tsv: bool,
    },
    /// The long text of one item
    Text {
        file: PathBuf,
        #[arg(long)]
        oz: String,
        #[arg(long)]
        markdown: bool,
    },
    /// The document as JSON
    Json {
        file: PathBuf,
        #[arg(long)]
        compact: bool,
    },
    /// Phase, schema and arithmetic rules; exit code 1 on errors
    Validate {
        file: PathBuf,
        /// The tender this bid answers
        #[arg(long)]
        tender: Option<PathBuf>,
    },
    /// Read and write back, optionally as another phase or version
    Roundtrip {
        file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        phase: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        pretty: bool,
        /// Refuse when a required element is missing
        #[arg(long)]
        strict: bool,
    },
    /// Price a tender into an X84 bid
    Bid {
        tender: PathBuf,
        /// CSV with columns oz,up[,qty] or id,up[,qty]; "-" marks not offered
        #[arg(long)]
        prices: PathBuf,
        /// TOML with name1, street, pcode, city and optional name2, contact,
        /// email, bidder_no, vat_percent, discount_percent
        #[arg(long)]
        bidder: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        pretty: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("gaebboss: {message}");
            ExitCode::from(2)
        }
    }
}

fn open(path: &Path) -> Result<Document, String> {
    Document::open(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn run(command: Command) -> Result<ExitCode, String> {
    match command {
        Command::Info { file } => {
            let doc = open(&file)?;
            print!("{}", info(&doc));
            Ok(ExitCode::SUCCESS)
        }
        Command::Items {
            file,
            counting,
            tsv,
        } => {
            let doc = open(&file)?;
            print!("{}", items(&doc, counting, tsv));
            Ok(ExitCode::SUCCESS)
        }
        Command::Text { file, oz, markdown } => {
            let doc = open(&file)?;
            let Some(item) = doc
                .items()
                .find(|i| i.oz.dotted() == oz || i.oz.compact() == oz || i.id() == oz)
            else {
                return Err(format!("no item {oz}"));
            };
            match markdown {
                true => println!("{}", item.long_text_markdown()),
                false => println!("{}", item.long_text()),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Json { file, compact } => {
            let doc = open(&file)?;
            let value = to_json(&doc);
            let text = match compact {
                true => serde_json::to_string(&value),
                false => serde_json::to_string_pretty(&value),
            }
            .map_err(|e| e.to_string())?;
            println!("{text}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate { file, tender } => {
            let doc = open(&file)?;
            let mut findings = validate(&doc);
            if let Some(tender) = tender {
                findings.extend(validate_against_tender(&doc, &open(&tender)?));
            }
            let errors = findings
                .iter()
                .filter(|f| f.severity == Severity::Error)
                .count();
            let warnings = findings
                .iter()
                .filter(|f| f.severity == Severity::Warning)
                .count();
            for f in &findings {
                let oz =
                    f.oz.as_deref()
                        .map(|o| format!(" [{o}]"))
                        .unwrap_or_default();
                println!("{:?} {:?}{oz}: {}", f.severity, f.code, f.message);
            }
            println!(
                "{errors} errors, {warnings} warnings, {} notes",
                findings.len() - errors - warnings
            );
            Ok(match errors {
                0 => ExitCode::SUCCESS,
                _ => ExitCode::from(1),
            })
        }
        Command::Roundtrip {
            file,
            output,
            phase,
            version,
            pretty,
            strict,
        } => {
            let doc = open(&file)?;
            let phase = match phase {
                Some(code) => Phase::from_extension(&code)
                    .or_else(|| Phase::from_code(&code))
                    .ok_or_else(|| format!("unknown phase {code}"))?,
                None => doc.phase().ok_or("the file names no phase; pass --phase")?,
            };
            let version = match version {
                Some(v) => Version::parse(&v).ok_or_else(|| format!("unknown version {v}"))?,
                None => doc.version(),
            };
            let opts = WriteOptions {
                version,
                phase,
                indent: pretty.then_some(2),
                prog_system: None,
                strict,
            };
            let (bytes, report) = doc.write_with_report(&opts).map_err(|e| e.to_string())?;
            std::fs::write(&output, bytes).map_err(|e| format!("{}: {e}", output.display()))?;
            for d in &report.dropped {
                eprintln!(
                    "dropped <{}> from <{}> ({}x)",
                    d.element, d.type_name, d.count
                );
            }
            for m in &report.missing {
                eprintln!("missing <{}> in <{}>", m.element.as_str(), m.type_name);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Bid {
            tender,
            prices,
            bidder,
            output,
            pretty,
        } => {
            let doc = open(&tender)?;
            let settings = read_toml(&bidder)?;
            let get = |key: &str| settings.get(key).cloned();
            let bidder = Bidder {
                name1: get("name1").ok_or("bidder.toml needs name1")?,
                name2: get("name2"),
                street: get("street").ok_or("bidder.toml needs street")?,
                pcode: get("pcode").ok_or("bidder.toml needs pcode")?,
                city: get("city").ok_or("bidder.toml needs city")?,
                contact: get("contact"),
                email: get("email"),
                bidder_no: get("bidder_no"),
            };
            let mut bid = Bid::from_tender(&doc, bidder).map_err(|e| e.to_string())?;
            let by_key: BTreeMap<String, String> = doc
                .items()
                .flat_map(|i| {
                    [
                        (i.oz.dotted(), i.id().to_string()),
                        (i.oz.compact(), i.id().to_string()),
                        (i.id().to_string(), i.id().to_string()),
                    ]
                })
                .collect();
            for (line_no, row) in read_prices(&prices)?.into_iter().enumerate() {
                let id = by_key.get(&row.key).ok_or_else(|| {
                    format!("{}:{}: no item {}", prices.display(), line_no + 2, row.key)
                })?;
                if let Some(qty) = row.qty {
                    bid.free_quantity(id, qty)
                        .map_err(|e| format!("{}: {e}", row.key))?;
                }
                match row.up {
                    Some(up) => bid.price(id, up).map_err(|e| format!("{}: {e}", row.key))?,
                    None => bid
                        .not_offered(id)
                        .map_err(|e| format!("{}: {e}", row.key))?,
                }
            }
            if let Some(v) = get("vat_percent") {
                bid.vat_percent(Decimal::parse(&v).map_err(|e| format!("vat_percent: {e}"))?);
            }
            if let Some(v) = get("discount_percent") {
                bid.discount_percent(
                    Decimal::parse(&v).map_err(|e| format!("discount_percent: {e}"))?,
                );
            }
            let finished = bid.finish().map_err(|e| e.to_string())?;
            let opts = WriteOptions {
                indent: pretty.then_some(2),
                ..finished.x84_options()
            };
            let bytes = finished.write(&opts).map_err(|e| e.to_string())?;
            std::fs::write(&output, bytes).map_err(|e| format!("{}: {e}", output.display()))?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn info(doc: &Document) -> String {
    let mut out = String::new();
    let project = doc.gaeb.prj_info.as_ref();
    let info = doc.boq().and_then(|b| b.boq_info.as_ref());
    let line = |out: &mut String, key: &str, value: String| {
        if value.is_empty() {
            return;
        }
        out.push_str(&format!("{key}: {value}\n"));
    };
    line(&mut out, "version", doc.version().as_str().to_string());
    line(
        &mut out,
        "phase",
        doc.phase()
            .map(|p| format!("X{}", p.code()))
            .unwrap_or_default(),
    );
    line(
        &mut out,
        "project",
        project
            .and_then(|p| p.name_prj)
            .map(|s| doc.str(s).to_string())
            .unwrap_or_default(),
    );
    line(
        &mut out,
        "project label",
        project
            .and_then(|p| p.lbl_prj)
            .map(|s| doc.str(s).to_string())
            .unwrap_or_default(),
    );
    line(
        &mut out,
        "boq",
        info.and_then(|i| i.name)
            .map(|s| doc.str(s).to_string())
            .unwrap_or_default(),
    );
    line(
        &mut out,
        "boq label",
        info.and_then(|i| i.lbl_boq)
            .map(|s| doc.str(s).to_string())
            .unwrap_or_default(),
    );
    line(
        &mut out,
        "breakdown",
        doc.breakdown()
            .levels()
            .iter()
            .map(|l| format!("{:?}({})", l.kind, l.length))
            .collect::<Vec<_>>()
            .join(" "),
    );
    line(&mut out, "categories", doc.categories().count().to_string());
    line(&mut out, "items", doc.items().count().to_string());
    line(
        &mut out,
        "markup items",
        doc.markup_items().count().to_string(),
    );
    line(
        &mut out,
        "total",
        doc.declared_total()
            .map(|d| d.to_string())
            .unwrap_or_default(),
    );
    line(&mut out, "warnings", doc.warnings.len().to_string());
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    for w in &doc.warnings {
        *histogram.entry(format!("{:?}", w.code)).or_default() += 1;
    }
    for (code, n) in histogram {
        out.push_str(&format!("  {code}: {n}\n"));
    }
    out
}

fn items(doc: &Document, counting: bool, tsv: bool) -> String {
    let mut out = String::new();
    if tsv {
        out.push_str("oz\tid\tkind\tquantity\tunit\tunit_price\ttotal\tshort_text\n");
    }
    for item in doc.items() {
        if counting && !item.counts_towards_total() {
            continue;
        }
        let cell = |d: Option<Decimal>| d.map(|d| d.to_string()).unwrap_or_default();
        let short = item.short_text().replace(['\t', '\n'], " ");
        match tsv {
            true => out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                item.oz,
                item.id(),
                kind_name(item.kind()),
                cell(item.quantity()),
                item.unit().unwrap_or(""),
                cell(item.unit_price()),
                cell(item.total()),
                short
            )),
            false => out.push_str(&format!(
                "{:<16} {:<24} {:>12} {:<5} {:>12} {:>14}  {}\n",
                item.oz.dotted(),
                kind_name(item.kind()),
                cell(item.quantity()),
                item.unit().unwrap_or(""),
                cell(item.unit_price()),
                cell(item.total()),
                short
            )),
        }
    }
    out
}

struct PriceRow {
    key: String,
    up: Option<Decimal>,
    qty: Option<Decimal>,
}

fn read_prices(path: &Path) -> Result<Vec<PriceRow>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || (i == 0 && (line.starts_with("oz") || line.starts_with("id"))) {
            continue;
        }
        let fields: Vec<&str> = line.split([',', ';', '\t']).map(str::trim).collect();
        let key = fields.first().copied().unwrap_or_default().to_string();
        let up = match fields.get(1).copied().unwrap_or("-") {
            "-" | "" => None,
            value => Some(
                Decimal::parse(&value.replace(',', "."))
                    .map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?,
            ),
        };
        let qty = match fields.get(2).copied().unwrap_or("") {
            "" => None,
            value => Some(
                Decimal::parse(&value.replace(',', "."))
                    .map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?,
            ),
        };
        rows.push(PriceRow { key, up, qty });
    }
    Ok(rows)
}

/// `key = "value"` lines; enough TOML for a bidder's address.
fn read_toml(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        out.insert(key.trim().to_string(), value);
    }
    Ok(out)
}
