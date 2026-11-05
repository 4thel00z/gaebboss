//! The gaebboss command line: inspect, explore, export, validate, round-trip
//! and price GAEB DA XML files.

mod hexdump;
mod pricing;
mod q;
mod report;
mod skill;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use gaebboss_core::Document;
use gaebboss_core::bid::{Bid, Bidder};
use gaebboss_core::decimal::Decimal;
use gaebboss_core::json::to_json;
use gaebboss_core::validate::{Severity, validate, validate_with_tender};
use gaebboss_core::version::{Phase, Version};
use gaebboss_core::write::WriteOptions;

/// Why a command failed and the exit code it maps to.
#[derive(Debug)]
pub struct Failure {
    pub message: String,
    pub code: u8,
}

impl Failure {
    /// A file, parse or I/O failure (exit code 1).
    pub fn new(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            code: 1,
        }
    }

    /// An invalid jq program (exit code 2).
    pub fn program(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            code: 2,
        }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Failure {
        Failure::new(message)
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Failure {
        Failure::new(message)
    }
}

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
    /// Show version, phase, project, counts, totals and warnings.
    Info {
        /// Path to the GAEB file.
        file: PathBuf,
    },
    /// List the items, one row each.
    Items {
        /// Path to the GAEB file.
        file: PathBuf,
        /// Only items that count towards the total.
        #[arg(long)]
        counting: bool,
        /// Tab-separated with a header line.
        #[arg(long)]
        tsv: bool,
    },
    /// Extract long texts (every item unless --oz is given).
    Text {
        /// Path to the GAEB file.
        file: PathBuf,
        /// One item by Ordnungszahl (dotted or compact) or ID.
        #[arg(long)]
        oz: Option<String>,
        /// Markdown instead of plain text, the same as `gaebboss md`.
        #[arg(long)]
        markdown: bool,
    },
    /// Extract Markdown: category headings, item long texts, remarks and additional texts.
    Md {
        /// Path to the GAEB file.
        file: PathBuf,
        /// One item by Ordnungszahl (dotted or compact) or ID.
        #[arg(long)]
        oz: Option<String>,
    },
    /// Dump the document as JSON (for piping to external tools).
    Json {
        /// Path to the GAEB file.
        file: PathBuf,
        /// One line instead of indented.
        #[arg(long)]
        compact: bool,
    },
    /// Run a jq program over the document's JSON.
    Q {
        /// Path to the GAEB file.
        file: PathBuf,
        /// jq program, e.g. '.items[] | select(.kind == "LumpSum") | .oz'.
        program: String,
        /// Print string results raw, without quotes (like jq -r).
        #[arg(short = 'r')]
        raw_strings: bool,
    },
    /// Hexdump the document or a selected element (hexyl-style).
    ///
    /// Offsets are into the document as parsed: UTF-8, without a byte order
    /// mark, Latin-1 and UTF-16 inputs transcoded.
    Hex {
        /// Path to the GAEB file.
        file: PathBuf,
        /// oz:OZ | id:ID | range:START-END (offsets decimal or 0x-hex).
        /// Default: the whole document.
        selector: Option<String>,
        /// Print labeled category and item boundaries as the dump crosses them.
        #[arg(long)]
        annotate: bool,
        /// Bytes per row.
        #[arg(long, default_value_t = 16)]
        width: usize,
    },
    /// Explore a GAEB file interactively in the terminal.
    Tui {
        /// Path to the GAEB file.
        file: PathBuf,
    },
    /// Check phase, schema and arithmetic rules; exit code 1 on errors.
    Validate {
        /// Path to the GAEB file.
        file: PathBuf,
        /// The tender this bid answers; also tells the totals which items
        /// and categories are alternatives or not applicable.
        #[arg(long)]
        tender: Option<PathBuf>,
    },
    /// Read and write back, optionally as another phase or version.
    Roundtrip {
        /// Path to the GAEB file.
        file: PathBuf,
        /// Output GAEB file.
        #[arg(short, long)]
        out: PathBuf,
        /// Target phase, e.g. 84 or X84 (default: the file's own).
        #[arg(long)]
        phase: Option<String>,
        /// Target version, e.g. 3.3 (default: the file's own).
        #[arg(long)]
        version: Option<String>,
        /// Indent the written XML.
        #[arg(long)]
        pretty: bool,
        /// Refuse when a required element is missing.
        #[arg(long)]
        strict: bool,
    },
    /// Price a tender into an X84 bid.
    Bid {
        /// Path to the X83 tender.
        tender: PathBuf,
        /// CSV with columns oz,up[,qty] or id,up[,qty]; "-" marks not offered.
        #[arg(long)]
        prices: PathBuf,
        /// TOML with name1, street, pcode, city and optional name2, contact,
        /// email, bidder_no, vat_percent, discount_percent.
        #[arg(long)]
        bidder: PathBuf,
        /// Output X84 file.
        #[arg(short, long)]
        out: PathBuf,
        /// Indent the written XML.
        #[arg(long)]
        pretty: bool,
    },
    /// Install or print the bundled Claude Code skill for coding agents.
    Skill {
        #[command(subcommand)]
        command: skill::SkillCommand,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(code) => code,
        Err(failure) => {
            eprintln!("gaebboss: {}", failure.message);
            ExitCode::from(failure.code)
        }
    }
}

fn open(path: &Path) -> Result<Document, Failure> {
    Document::open(path).map_err(|e| Failure::new(format!("{}: {e}", path.display())))
}

fn run(command: Command) -> Result<ExitCode, Failure> {
    match command {
        Command::Info { file } => {
            print!("{}", report::info(&open(&file)?));
            Ok(ExitCode::SUCCESS)
        }
        Command::Items {
            file,
            counting,
            tsv,
        } => {
            print!("{}", report::items(&open(&file)?, counting, tsv));
            Ok(ExitCode::SUCCESS)
        }
        Command::Text { file, oz, markdown } => {
            let doc = open(&file)?;
            let text = match markdown {
                true => report::markdown(&doc, oz.as_deref())?,
                false => report::text(&doc, oz.as_deref())?,
            };
            println!("{text}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Md { file, oz } => {
            println!("{}", report::markdown(&open(&file)?, oz.as_deref())?);
            Ok(ExitCode::SUCCESS)
        }
        Command::Json { file, compact } => {
            let value = to_json(&open(&file)?);
            let text = match compact {
                true => serde_json::to_string(&value),
                false => serde_json::to_string_pretty(&value),
            }
            .map_err(|e| Failure::new(e.to_string()))?;
            println!("{text}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Q {
            file,
            program,
            raw_strings,
        } => {
            q::cmd_q(&file, &program, raw_strings)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Hex {
            file,
            selector,
            annotate,
            width,
        } => {
            hexdump::cmd_hex(&open(&file)?, selector.as_deref(), annotate, width)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Tui { file } => {
            tui(&file)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate { file, tender } => {
            let doc = open(&file)?;
            let findings = match tender {
                Some(tender) => validate_with_tender(&doc, &open(&tender)?),
                None => validate(&doc),
            };
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
            out,
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
            let (bytes, report) = doc
                .write_with_report(&opts)
                .map_err(|e| Failure::new(e.to_string()))?;
            std::fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
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
            out,
            pretty,
        } => {
            bid(&tender, &prices, &bidder, &out, pretty)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Skill { command } => {
            skill::cmd_skill(command)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `gaebboss tui`: parses first, so a broken file fails before the terminal
/// switches screens, then runs the explorer on a current-thread runtime. Quitting
/// before the outline arrives does not wait for it.
fn tui(file: &Path) -> Result<(), Failure> {
    let doc = open(file)?;
    let title = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.display().to_string());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|e| Failure::new(e.to_string()))?;
    let result = runtime.block_on(gaebboss_tui::run(doc, title, file.display().to_string()));
    runtime.shutdown_background();
    result.map_err(|e| Failure::new(e.to_string()))
}

fn bid(
    tender: &Path,
    prices: &Path,
    bidder: &Path,
    out: &Path,
    pretty: bool,
) -> Result<(), Failure> {
    let doc = open(tender)?;
    let settings = pricing::read_toml(bidder)?;
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
    for (line_no, row) in pricing::read_prices(prices)?.into_iter().enumerate() {
        let id = by_key
            .get(&row.key)
            .ok_or_else(|| format!("{}:{}: no item {}", prices.display(), line_no + 2, row.key))?;
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
        bid.discount_percent(Decimal::parse(&v).map_err(|e| format!("discount_percent: {e}"))?);
    }
    let finished = bid.finish().map_err(|e| e.to_string())?;
    let opts = WriteOptions {
        indent: pretty.then_some(2),
        ..finished.x84_options()
    };
    let bytes = finished.write(&opts).map_err(|e| e.to_string())?;
    std::fs::write(out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_is_the_output_flag_and_output_is_gone() {
        let cli = Cli::try_parse_from(["gaebboss", "roundtrip", "a.x83", "--out", "b.x83"]);
        assert!(cli.is_ok());
        let old = Cli::try_parse_from(["gaebboss", "roundtrip", "a.x83", "--output", "b.x83"]);
        assert!(old.is_err());
    }

    #[test]
    fn text_oz_is_optional() {
        assert!(Cli::try_parse_from(["gaebboss", "text", "a.x83"]).is_ok());
        assert!(Cli::try_parse_from(["gaebboss", "md", "a.x83", "--oz", "01.0010"]).is_ok());
    }

    #[test]
    fn every_command_documents_its_arguments() {
        let command = <Cli as clap::CommandFactory>::command();
        let undocumented: Vec<String> = command
            .get_subcommands()
            .flat_map(|sub| {
                sub.get_arguments()
                    .filter(|arg| arg.get_help().is_none())
                    .map(|arg| format!("{} {}", sub.get_name(), arg.get_id()))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(undocumented.is_empty(), "{undocumented:?}");
    }
}
