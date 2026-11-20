use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

use crate::{
    config::EzpmConfig,
    diagnostics::{self, Diagnostic},
    error::EzpmError,
    output,
    services::{
        path_filter::{self, PathFilter},
        require_fixer::{self, Ambiguity, FileChange, FixContext},
        rojo_project::RojoProjectSettings,
        sourcemap,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Rewrite the requires.
    Write,
    /// List the rewrites, write nothing.
    DryRun,
    /// List the rewrites as errors and exit 1 if there are any (for CI).
    Check,
}

pub fn run(config: &EzpmConfig, mode: Mode, paths: &[String]) -> Result<()> {
    if mode == Mode::Write && !paths.is_empty() {
        return Err(
            EzpmError::usage("file paths only work with --check or --dry-run")
                .with_hint("run `ezpm fix-requires` without paths to fix the whole project")
                .into(),
        );
    }

    let project_dir = std::env::current_dir().context("could not determine current directory")?;
    let aliases = config.aliases.as_ref().cloned().unwrap_or_default();
    let src = config.src_dir();
    let rojo = RojoProjectSettings::from_config(config);
    let index = sourcemap::generate_index(&project_dir, &rojo.project)?;
    let context = FixContext::new(&project_dir, &src, &aliases, index);

    if mode == Mode::Write {
        let result = require_fixer::fix_requires_with_context(&context)?;
        if output::is_json() {
            output::document(&json!({
                "changed": result.files_changed > 0,
                "files_scanned": result.total_files_scanned,
                "changes": changes_json(&project_dir, &result.changes),
            }));
        } else {
            print_result(&project_dir, &result);
        }
        return Ok(());
    }

    let plan = require_fixer::plan_fixes(&context)?;
    let filter = PathFilter::new(&project_dir, paths);
    let changes: Vec<&FileChange> = plan
        .pending
        .iter()
        .map(|(change, _)| change)
        .filter(|change| filter.matches(&project_dir, &change.file))
        .collect();

    let mut found = Vec::new();
    for ambiguity in &plan.ambiguities {
        if filter.is_empty()
            || ambiguity
                .candidates
                .iter()
                .any(|path| filter.matches(&project_dir, path))
        {
            found.push(ambiguity_diagnostic(&project_dir, ambiguity));
        }
    }
    if mode == Mode::Check {
        for change in &changes {
            let file = path_filter::display(&project_dir, &change.file);
            for rewrite in &change.rewrites {
                found.push(
                    Diagnostic::new(
                        "require.wouldRewrite",
                        format!(
                            "require(\"{}\") would become require(\"{}\")",
                            rewrite.old, rewrite.new
                        ),
                    )
                    .at(file.clone(), Some(rewrite.line))
                    .hint("run `ezpm fix-requires`"),
                );
            }
        }
    }

    let rewrite_count: usize = changes.iter().map(|change| change.rewrites.len()).sum();
    if output::is_json() {
        output::document(&json!({
            "changed": false,
            "files_scanned": plan.total_files_scanned,
            "would_change": changes.len(),
            "changes": changes_json(&project_dir, changes.iter().copied()),
            "diagnostics": found,
        }));
    } else {
        if mode == Mode::DryRun {
            print_planned(&project_dir, &changes);
        }
        diagnostics::report(&found);
        if rewrite_count == 0 && found.is_empty() {
            output::success(&format!(
                "All requires up to date across {} files.",
                plan.total_files_scanned
            ));
        } else if mode == Mode::DryRun && rewrite_count > 0 {
            output::info(&format!(
                "Would fix {} requires across {} files. Nothing was written.",
                rewrite_count,
                changes.len()
            ));
        }
    }

    let errors = found.iter().filter(|d| d.is_error()).count();
    if errors > 0 {
        return Err(EzpmError::reported(
            format!(
                "{errors} require problem{} found",
                if errors == 1 { "" } else { "s" }
            ),
            1,
        )
        .into());
    }
    Ok(())
}

fn ambiguity_diagnostic(project_dir: &Path, ambiguity: &Ambiguity) -> Diagnostic {
    let candidates = ambiguity
        .candidates
        .iter()
        .map(|path| path_filter::display(project_dir, path))
        .collect::<Vec<_>>()
        .join(", ");
    Diagnostic::new(
        "require.ambiguous",
        format!("require(\"{}\") matches {candidates}", ambiguity.name),
    )
    .hint("use an alias or the full @game/... path")
}

fn changes_json<'a>(
    project_dir: &Path,
    changes: impl IntoIterator<Item = &'a FileChange>,
) -> Vec<Value> {
    changes
        .into_iter()
        .map(|change| {
            json!({
                "file": path_filter::display(project_dir, &change.file),
                "rewrites": change.rewrites.iter().map(|rewrite| json!({
                    "line": rewrite.line,
                    "old": rewrite.old,
                    "new": rewrite.new,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn print_planned(project_dir: &Path, changes: &[&FileChange]) {
    for change in changes {
        let file = path_filter::display(project_dir, &change.file);
        for rewrite in &change.rewrites {
            output::print_line(&format!(
                "{file}:{}  {} -> {}",
                rewrite.line, rewrite.old, rewrite.new
            ));
        }
    }
    if !changes.is_empty() {
        output::print_line("");
    }
}

fn print_result(project_dir: &Path, result: &require_fixer::FixResult) {
    if result.files_changed == 0 {
        output::success(&format!(
            "All requires up to date. 0 changes across {} files.",
            result.total_files_scanned
        ));
        return;
    }

    let total_rewrites = result
        .changes
        .iter()
        .map(|change| change.rewrites.len())
        .sum::<usize>();
    for change in &result.changes {
        output::print_line(&format!(
            "{}:",
            path_filter::display(project_dir, &change.file)
        ));
        for rewrite in &change.rewrites {
            output::print_line(&format!(
                "  {}: {} -> {}",
                rewrite.line, rewrite.old, rewrite.new
            ));
        }
    }
    output::print_line("");
    output::success(&format!(
        "Fixed {} requires across {} files",
        total_rewrites, result.files_changed
    ));
}
