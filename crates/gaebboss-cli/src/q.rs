//! `gaebboss q`: jq programs over the document's JSON, run by the jaq engine.
//! Compile errors exit 2, file and jq runtime errors exit 1.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use jaq_core::data::JustLut;
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars};
use jaq_json::{Num, Val};
use serde_json::Value;

use crate::Failure;

pub struct Program {
    filter: jaq_core::Filter<JustLut<Val>>,
}

/// Compiles `code` against the jq standard library, reporting lex, parse and
/// compile errors with byte positions.
pub fn compile_program(code: &str) -> Result<Program, String> {
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let loader = Loader::new(defs);
    let arena = Arena::default();
    let modules = loader
        .load(&arena, File { path: (), code })
        .map_err(|errors| describe_load_errors(code, errors))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errors| describe_compile_errors(code, errors))?;
    Ok(Program { filter })
}

/// Every output of the program over `input`, in order; runtime errors such as
/// `error("boom")` come back as `Err` items.
pub fn run_program(program: &Program, input: Value) -> Vec<Result<Value, String>> {
    let input = match serde_json::from_value::<Val>(input) {
        Ok(val) => val,
        Err(e) => return vec![Err(e.to_string())],
    };
    let ctx = Ctx::<JustLut<Val>>::new(&program.filter.lut, Vars::new([]));
    program
        .filter
        .id
        .run((ctx, input))
        .map(|item| {
            item.map(|val| json_value(&val))
                .map_err(|exn| match exn.get_err() {
                    Ok(error) => error.to_string(),
                    Err(exn) => match exn.get_halt() {
                        Ok(code) => format!("halt({code})"),
                        Err(_) => "jq: filter interrupted".to_string(),
                    },
                })
        })
        .collect()
}

/// The JSON a jaq value denotes; infinities and NaN become `null` as in jq.
fn json_value(val: &Val) -> Value {
    match val {
        Val::Null => Value::Null,
        Val::Bool(b) => Value::Bool(*b),
        Val::Num(num) => json_number(num),
        Val::TStr(bytes) | Val::BStr(bytes) => {
            Value::String(String::from_utf8_lossy(bytes).into_owned())
        }
        Val::Arr(items) => Value::Array(items.iter().map(json_value).collect()),
        Val::Obj(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (json_key(key), json_value(value)))
                .collect(),
        ),
    }
}

fn json_key(key: &Val) -> String {
    match json_value(key) {
        Value::String(text) => text,
        other => other.to_string(),
    }
}

fn json_number(num: &Num) -> Value {
    match num {
        Num::Int(i) => Value::from(*i as i64),
        Num::Float(f) => serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number),
        Num::BigInt(big) => big
            .to_string()
            .parse::<serde_json::Number>()
            .map_or(Value::Null, Value::Number),
        Num::Dec(text) => text
            .parse::<serde_json::Number>()
            .map_or(Value::Null, Value::Number),
    }
}

fn offset_in(code: &str, part: &str) -> usize {
    (part.as_ptr() as usize).saturating_sub(code.as_ptr() as usize)
}

fn describe_load_errors(code: &str, errors: jaq_core::load::Errors<&str, ()>) -> String {
    let messages: Vec<String> = errors
        .into_iter()
        .flat_map(|(_, error)| match error {
            jaq_core::load::Error::Io(items) => items
                .into_iter()
                .map(|(path, message)| format!("io error ({path}): {message}"))
                .collect::<Vec<_>>(),
            jaq_core::load::Error::Lex(items) => items
                .into_iter()
                .map(|(expected, found)| {
                    format!(
                        "lex error at byte {}: expected {}",
                        offset_in(code, found),
                        expected.as_str()
                    )
                })
                .collect(),
            jaq_core::load::Error::Parse(items) => items
                .into_iter()
                .map(|(expected, found)| {
                    format!(
                        "parse error at byte {}: expected {}",
                        offset_in(code, found),
                        expected.as_str()
                    )
                })
                .collect(),
        })
        .collect();
    joined(messages)
}

fn describe_compile_errors(code: &str, errors: jaq_core::compile::Errors<&str, ()>) -> String {
    let messages: Vec<String> = errors
        .into_iter()
        .flat_map(|(_, file_errors)| file_errors)
        .map(|(found, undefined)| {
            format!(
                "compile error at byte {}: undefined {}",
                offset_in(code, found),
                undefined.as_str()
            )
        })
        .collect();
    joined(messages)
}

fn joined(messages: Vec<String>) -> String {
    if messages.is_empty() {
        return "jq: invalid program".to_string();
    }
    messages.iter().fold(String::new(), |mut out, message| {
        if !out.is_empty() {
            out.push_str("; ");
        }
        let _ = write!(out, "jq: {message}");
        out
    })
}

/// Compiles first so a bad program fails before the file is read, then stops
/// at the first runtime error as jq does.
pub fn cmd_q(file: &Path, program: &str, raw_strings: bool) -> Result<(), Failure> {
    let program = compile_program(program).map_err(Failure::program)?;
    let doc = crate::open(file)?;
    let results = run_program(&program, gaebboss_core::json::to_json(&doc));
    let stdout = std::io::stdout();
    let mut w = std::io::BufWriter::new(stdout.lock());
    let io_failure = |e: std::io::Error| Failure::new(e.to_string());
    for result in results {
        let value = result.map_err(|message| Failure::new(format!("jq: {message}")))?;
        if let (true, Value::String(text)) = (raw_strings, &value) {
            writeln!(w, "{text}").map_err(io_failure)?;
            continue;
        }
        let text = serde_json::to_string_pretty(&value).map_err(|e| Failure::new(e.to_string()))?;
        writeln!(w, "{text}").map_err(io_failure)?;
    }
    w.flush().map_err(io_failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn outputs(code: &str, input: Value) -> Vec<Result<Value, String>> {
        run_program(&compile_program(code).unwrap(), input)
    }

    #[test]
    fn runs_paths_and_filters() {
        let input = json!({"items": [{"oz": "01.0010", "kind": "Standard"}, {"oz": "01.0020", "kind": "LumpSum"}]});
        assert_eq!(
            outputs(".items[] | select(.kind == \"LumpSum\") | .oz", input),
            vec![Ok(json!("01.0020"))]
        );
    }

    #[test]
    fn bad_programs_report_byte_positions() {
        let error = compile_program(".items[").err().unwrap();
        assert!(error.starts_with("jq: "), "{error}");
        let undefined = compile_program("nosuchfn").err().unwrap();
        assert!(undefined.contains("undefined"), "{undefined}");
    }

    #[test]
    fn runtime_errors_are_items() {
        let results = outputs("1, error(\"boom\"), 2", Value::Null);
        assert_eq!(results.len(), 3);
        assert!(results[1].is_err());
    }
}
