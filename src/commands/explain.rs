use anyhow::Result;
use serde_json::json;

use crate::diagnostics::{self, CODES};
use crate::error::EzpmError;
use crate::output;

pub fn run(code: Option<&str>, list: bool) -> Result<()> {
    if list {
        if output::is_json() {
            let codes: Vec<_> = CODES
                .iter()
                .map(|info| {
                    json!({
                        "code": info.code,
                        "severity": info.severity,
                        "summary": info.summary,
                        "url": diagnostics::docs_url(info.code),
                    })
                })
                .collect();
            output::document(&json!({ "codes": codes }));
        } else {
            let width = CODES.iter().map(|info| info.code.len()).max().unwrap_or(0);
            for info in CODES {
                println!("{:<width$}  {}", info.code, info.summary);
            }
        }
        return Ok(());
    }

    let code = code.unwrap_or_default();
    let Some(info) = diagnostics::lookup(code) else {
        let names: Vec<&str> = CODES.iter().map(|info| info.code).collect();
        let mut error = EzpmError::usage(format!("unknown diagnostic code '{code}'"));
        error = match diagnostics::closest(code, &names) {
            Some(suggestion) => error.with_hint(format!("did you mean '{suggestion}'?")),
            None => error.with_hint("run `ezpm explain --list` to see every code"),
        };
        return Err(error.into());
    };

    if output::is_json() {
        output::document(&json!({
            "code": info.code,
            "severity": info.severity,
            "summary": info.summary,
            "text": info.explanation,
            "url": diagnostics::docs_url(info.code),
        }));
    } else {
        println!("{} ({})", info.code, info.severity.label());
        println!();
        println!("{}", info.explanation);
        println!();
        println!("{}", diagnostics::docs_url(info.code));
    }
    Ok(())
}
