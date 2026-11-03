//! The bid inputs: a price list as CSV and the bidder as flat TOML.

use std::collections::BTreeMap;
use std::path::Path;

use gaebboss_core::decimal::Decimal;

pub struct PriceRow {
    pub key: String,
    pub up: Option<Decimal>,
    pub qty: Option<Decimal>,
}

/// Rows of `oz,up[,qty]` or `id,up[,qty]`, separated by commas, semicolons
/// or tabs; decimal commas are accepted and `-` marks an item not offered.
pub fn read_prices(path: &Path) -> Result<Vec<PriceRow>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(header) = text.lines().next() else {
        return Ok(Vec::new());
    };
    let delimiter = match (header.contains(';'), header.contains('\t')) {
        (true, _) => ';',
        (false, true) => '\t',
        (false, false) => ',',
    };
    let decimal = |value: &str, line: usize| -> Result<Decimal, String> {
        Decimal::parse(&value.replace(',', "."))
            .map_err(|e| format!("{}:{line}: {e}", path.display()))
    };
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        let lower = line.to_ascii_lowercase();
        if line.is_empty() || (i == 0 && (lower.starts_with("oz") || lower.starts_with("id"))) {
            continue;
        }
        let fields: Vec<&str> = line.split(delimiter).map(str::trim).collect();
        let key = fields.first().copied().unwrap_or_default().to_string();
        let up = match fields.get(1).copied().unwrap_or("-") {
            "-" | "" => None,
            value => Some(decimal(value, i + 1)?),
        };
        let qty = match fields.get(2).copied().unwrap_or("") {
            "" => None,
            value => Some(decimal(value, i + 1)?),
        };
        rows.push(PriceRow { key, up, qty });
    }
    Ok(rows)
}

/// `key = "value"` lines; enough TOML for a bidder's address.
pub fn read_toml(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('['))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            (key.trim().to_string(), value.to_string())
        })
        .collect())
}
