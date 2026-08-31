use anyhow::{Context, Result};

use crate::{
    config::EzpmConfig,
    output,
    services::{
        require_fixer::{self, FixContext},
        rojo_project::RojoProjectSettings,
        sourcemap,
    },
};

pub fn run(config: &EzpmConfig) -> Result<()> {
    let project_dir = std::env::current_dir().context("could not determine current directory")?;
    let aliases = config.aliases.as_ref().cloned().unwrap_or_default();
    let rojo = RojoProjectSettings::from_config(config);
    let index = sourcemap::generate_index(&project_dir, &rojo.project)?;
    let context = FixContext::new(&project_dir, &aliases, index);
    let result = require_fixer::fix_requires_with_context(&context)?;
    print_result(&result);
    Ok(())
}

fn print_result(result: &require_fixer::FixResult) {
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
        output::print_line(&format!("{}:", change.file.display()));
        for rewrite in &change.rewrites {
            output::print_line(&format!("  {} -> {}", rewrite.old, rewrite.new));
        }
    }
    output::print_line("");
    output::success(&format!(
        "Fixed {} requires across {} files",
        total_rewrites, result.files_changed
    ));
}
