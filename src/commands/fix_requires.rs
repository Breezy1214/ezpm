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
        require_fixer::{self, Ambiguity, FileChange, FixContext, FixPlan},
        rojo_project::RojoProjectSettings,
        sourcemap::{self, SourcemapIndex},
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
    let rojo = RojoProjectSettings::from_config(config);
    let index = sourcemap::generate_index(&project_dir, &rojo.project)?;
    let context = fix_context(config, &project_dir, index);
    let review = review(&context, &project_dir, mode, paths)?;

    if mode == Mode::Write && review.diagnostics.is_empty() {
        let result = require_fixer::apply_plan(review.plan)?;
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

    let changes = review.changes();
    let rewrite_count: usize = changes.iter().map(|change| change.rewrites.len()).sum();
    if output::is_json() {
        output::document(&json!({
            "changed": false,
            "files_scanned": review.plan.total_files_scanned,
            "would_change": changes.len(),
            "changes": changes_json(&project_dir, changes.iter().copied()),
            "diagnostics": review.diagnostics,
        }));
    } else {
        if mode == Mode::DryRun {
            print_planned(&project_dir, &changes);
        }
        diagnostics::report(&review.diagnostics);
        if rewrite_count == 0 && review.diagnostics.is_empty() {
            output::success(&format!(
                "All requires up to date across {} files.",
                review.plan.total_files_scanned
            ));
        } else if mode == Mode::DryRun && rewrite_count > 0 {
            output::info(&format!(
                "Would fix {} requires across {} files. Nothing was written.",
                rewrite_count,
                changes.len()
            ));
        }
    }

    let errors = review.diagnostics.iter().filter(|d| d.is_error()).count();
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

/// What `fix-requires --check` reports about `paths` (everything when empty), printing nothing.
pub fn check_diagnostics(
    config: &EzpmConfig,
    project_dir: &Path,
    index: SourcemapIndex,
    paths: &[String],
) -> Result<Vec<Diagnostic>> {
    let context = fix_context(config, project_dir, index);
    Ok(review(&context, project_dir, Mode::Check, paths)?.diagnostics)
}

fn fix_context(config: &EzpmConfig, project_dir: &Path, index: SourcemapIndex) -> FixContext {
    let aliases = config.aliases.clone().unwrap_or_default();
    FixContext::new(project_dir, &config.src_dir(), &aliases, index)
}

/// The plan, which of its changes are about `paths`, and the problems to report: ambiguities
/// always, and each rewrite as an error in `--check` mode.
struct Review {
    plan: FixPlan,
    selected: Vec<usize>,
    diagnostics: Vec<Diagnostic>,
}

impl Review {
    fn changes(&self) -> Vec<&FileChange> {
        self.selected
            .iter()
            .map(|&i| &self.plan.pending[i].0)
            .collect()
    }
}

fn review(ctx: &FixContext, project_dir: &Path, mode: Mode, paths: &[String]) -> Result<Review> {
    let filter = PathFilter::new(project_dir, paths)?;
    let plan = require_fixer::plan_fixes(ctx)?;
    let selected: Vec<usize> = (0..plan.pending.len())
        .filter(|&i| filter.matches(project_dir, &plan.pending[i].0.file))
        .collect();

    let mut diagnostics: Vec<Diagnostic> = plan
        .ambiguities
        .iter()
        .filter(|ambiguity| filter.matches(project_dir, &ambiguity.file))
        .map(|ambiguity| ambiguity_diagnostic(project_dir, ambiguity))
        .collect();
    if mode == Mode::Check {
        for &i in &selected {
            let change = &plan.pending[i].0;
            let file = path_filter::display(project_dir, &change.file);
            for rewrite in &change.rewrites {
                diagnostics.push(
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

    Ok(Review {
        plan,
        selected,
        diagnostics,
    })
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
    .at(
        path_filter::display(project_dir, &ambiguity.file),
        Some(ambiguity.line),
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
