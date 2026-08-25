use anyhow::{Context, Result};
use std::path::Path;
use std::sync::OnceLock;

const EMBEDDED_ROKIT_TOML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/rokit.toml"));
const REQUIRED_RUNTIME_TOOL_NAMES: &[&str] = &["wally-package-types", "wally", "rojo", "darklua"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: String,
    pub spec: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolUpdate {
    pub name: String,
    pub old_spec: String,
    pub new_spec: String,
}

pub fn embedded_rokit_toml() -> &'static str {
    EMBEDDED_ROKIT_TOML
}

pub fn default_tool_specs() -> &'static [ToolSpec] {
    static DEFAULT_TOOL_SPECS: OnceLock<Vec<ToolSpec>> = OnceLock::new();

    DEFAULT_TOOL_SPECS
        .get_or_init(|| {
            parse_tool_specs(EMBEDDED_ROKIT_TOML)
                .expect("repo rokit.toml must contain a valid [tools] manifest")
        })
        .as_slice()
}

pub fn required_runtime_tool_specs() -> Vec<&'static ToolSpec> {
    REQUIRED_RUNTIME_TOOL_NAMES
        .iter()
        .filter_map(|name| tool_spec_by_name(name))
        .collect()
}

pub fn tool_spec_by_name(name: &str) -> Option<&'static ToolSpec> {
    default_tool_specs().iter().find(|tool| tool.name == name)
}

pub fn parse_tool_specs(contents: &str) -> Result<Vec<ToolSpec>> {
    let parsed: toml::Value =
        toml::from_str(contents).context("Failed to parse rokit.toml contents")?;
    let tools_table = parsed
        .get("tools")
        .and_then(toml::Value::as_table)
        .context("rokit.toml is missing a [tools] table")?;

    let ordered_names = parse_ordered_tool_names(contents)?;
    let mut tools = Vec::with_capacity(ordered_names.len());

    for name in ordered_names {
        let spec = tools_table
            .get(&name)
            .and_then(toml::Value::as_str)
            .with_context(|| format!("tool '{name}' must have a string spec"))?;

        tools.push(ToolSpec {
            name,
            spec: spec.to_string(),
        });
    }

    Ok(tools)
}

pub fn find_tool_spec_in_contents(contents: &str, name: &str) -> Option<String> {
    parse_tool_specs(contents)
        .ok()?
        .into_iter()
        .find(|tool| tool.name == name)
        .map(|tool| tool.spec)
}

pub fn find_tool_spec_in_path(path: &Path, name: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    find_tool_spec_in_contents(&contents, name)
}

pub fn spec_version(spec: &str) -> Option<&str> {
    spec.rsplit_once('@').map(|(_, version)| version)
}

pub fn outdated_tools(user_contents: &str) -> Result<Vec<ToolUpdate>> {
    let user_tools = parse_tool_specs(user_contents)?;
    let mut updates = Vec::new();

    for bundled in default_tool_specs() {
        if let Some(user_tool) = user_tools.iter().find(|t| t.name == bundled.name) {
            if let (Some(user_version), Some(bundled_version)) =
                (spec_version(&user_tool.spec), spec_version(&bundled.spec))
            {
                if crate::services::version::is_newer(user_version, bundled_version) {
                    updates.push(ToolUpdate {
                        name: bundled.name.clone(),
                        old_spec: user_tool.spec.clone(),
                        new_spec: bundled.spec.clone(),
                    });
                }
            }
        }
    }

    Ok(updates)
}

pub fn apply_tool_updates(contents: &str, updates: &[ToolUpdate]) -> String {
    if updates.is_empty() {
        return contents.to_string();
    }

    let mut in_tools = false;

    contents
        .split('\n')
        .map(|line| {
            let trimmed = line.trim();

            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_tools = trimmed == "[tools]";
                return line.to_string();
            }

            if in_tools && !trimmed.is_empty() && !trimmed.starts_with('#') {
                if let Some((name_part, _)) = trimmed.split_once('=') {
                    let name = name_part.trim();
                    if let Some(update) = updates.iter().find(|u| u.name == name) {
                        return line.replace(&update.old_spec, &update.new_spec);
                    }
                }
            }

            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_rokit_toml<'a, I>(tools: I, header: Option<&str>) -> String
where
    I: IntoIterator<Item = &'a ToolSpec>,
{
    let mut output = String::new();

    if let Some(header) = header {
        output.push_str(header.trim_end());
        output.push('\n');
    }

    output.push_str("[tools]\n");
    for tool in tools {
        output.push_str(&format!("{} = \"{}\"\n", tool.name, tool.spec));
    }

    output
}

pub fn render_default_rokit_toml(header: Option<&str>) -> String {
    render_rokit_toml(default_tool_specs().iter(), header)
}

pub fn rokit_bootstrap_hint() -> &'static str {
    "Install Rokit, then run `ezpm install`."
}

pub fn missing_tool_context(binary: &str) -> String {
    format!("Failed to run {binary}. {}", rokit_bootstrap_hint())
}

pub fn tool_install_hint(tool_name: &str) -> String {
    match tool_spec_by_name(tool_name) {
        Some(tool) => format!(
            "Install Rokit, then run `rokit add {}` or `ezpm install`.",
            tool.spec
        ),
        None => "Install this tool with Rokit, then try again.".to_string(),
    }
}

fn parse_ordered_tool_names(contents: &str) -> Result<Vec<String>> {
    let mut in_tools = false;
    let mut names = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_tools = line == "[tools]";
            continue;
        }

        if !in_tools {
            continue;
        }

        let (name, _) = line
            .split_once('=')
            .with_context(|| format!("invalid tool entry in [tools]: {line}"))?;
        names.push(name.trim().to_string());
    }

    if names.is_empty() {
        anyhow::bail!("rokit.toml [tools] table must contain at least one tool");
    }

    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_rokit_manifest_parses() {
        let parsed = parse_tool_specs(embedded_rokit_toml()).expect("embedded rokit.toml parses");

        assert!(
            !parsed.is_empty(),
            "embedded manifest must contain at least one tool"
        );
        assert!(
            parsed.iter().all(|tool| !tool.spec.is_empty()),
            "every embedded tool must have a spec"
        );
    }

    #[test]
    fn required_runtime_tools_are_known_and_exclude_lune() {
        let names: Vec<&str> = required_runtime_tool_specs()
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();

        assert_eq!(
            names,
            vec!["wally-package-types", "wally", "rojo", "darklua"],
            "required runtime tools should come from the embedded manifest in canonical order"
        );
        assert!(
            !names.contains(&"lune"),
            "lune should not be part of the required runtime toolchain"
        );
    }

    #[test]
    fn bundled_rojo_version_tracks_7_7_0() {
        let rojo = tool_spec_by_name("rojo").expect("rojo is in the bundled manifest");

        assert_eq!(
            rojo.spec, "rojo-rbx/rojo@7.7.0",
            "bundled Rojo should track the requested 7.7.0 release"
        );
    }

    #[test]
    fn rendered_default_rokit_toml_matches_embedded_manifest_order() {
        let rendered = render_default_rokit_toml(Some("# Generated by ezpm init"));
        let parsed = parse_tool_specs(&rendered).expect("rendered rokit.toml parses");

        assert_eq!(
            parsed,
            default_tool_specs(),
            "rendered rokit.toml should preserve the embedded manifest order and contents"
        );
        assert!(
            rendered.starts_with("# Generated by ezpm init\n[tools]\n"),
            "generated rokit.toml should include the init header"
        );
    }

    #[test]
    fn parse_tool_specs_preserves_custom_order() {
        let custom = r#"
[tools]
rojo = "rojo-rbx/rojo@7.7.0"
stylua = "JohnnyMorganz/StyLua@2.4.1"
darklua = "seaofvoices/darklua@0.18.0"
"#;

        let parsed = parse_tool_specs(custom).expect("custom rokit.toml parses");
        let names: Vec<&str> = parsed.iter().map(|tool| tool.name.as_str()).collect();

        assert_eq!(
            names,
            vec!["rojo", "stylua", "darklua"],
            "tool order should follow the source manifest"
        );
    }

    #[test]
    fn spec_version_extracts_version_suffix() {
        assert_eq!(spec_version("rojo-rbx/rojo@7.7.0"), Some("7.7.0"));
        assert_eq!(spec_version("owner/repo"), None);
    }

    #[test]
    fn outdated_tools_flags_older_pinned_version() {
        let rojo = tool_spec_by_name("rojo").expect("rojo is in the bundled manifest");
        let user = "[tools]\nrojo = \"rojo-rbx/rojo@0.0.1\"\n";

        let updates = outdated_tools(user).expect("user rokit.toml parses");

        assert_eq!(updates.len(), 1, "only the outdated rojo should be flagged");
        assert_eq!(updates[0].name, "rojo");
        assert_eq!(updates[0].old_spec, "rojo-rbx/rojo@0.0.1");
        assert_eq!(
            updates[0].new_spec, rojo.spec,
            "the update target should be ezpm's bundled spec"
        );
    }

    #[test]
    fn outdated_tools_ignores_equal_and_ahead_versions() {
        let rojo = tool_spec_by_name("rojo").expect("rojo is in the bundled manifest");

        let equal = format!("[tools]\nrojo = \"{}\"\n", rojo.spec);
        assert!(
            outdated_tools(&equal).expect("parses").is_empty(),
            "a version equal to the bundled spec must not be flagged"
        );

        let ahead = "[tools]\nrojo = \"rojo-rbx/rojo@999.0.0\"\n";
        assert!(
            outdated_tools(ahead).expect("parses").is_empty(),
            "a version newer than the bundled spec must not be downgraded"
        );
    }

    #[test]
    fn outdated_tools_ignores_project_only_tools() {
        let user = "[tools]\nmytool = \"acme/mytool@0.0.1\"\n";
        assert!(
            outdated_tools(user).expect("parses").is_empty(),
            "tools absent from the bundled manifest must be left untouched"
        );
    }

    #[test]
    fn apply_tool_updates_rewrites_only_targeted_specs() {
        let contents = "\
# leading comment
[tools]
rojo = \"rojo-rbx/rojo@7.0.0\"
wally = \"UpliftGames/wally@0.3.2\"
# trailing comment
";
        let updates = vec![ToolUpdate {
            name: "rojo".to_string(),
            old_spec: "rojo-rbx/rojo@7.0.0".to_string(),
            new_spec: "rojo-rbx/rojo@7.7.0".to_string(),
        }];

        let result = apply_tool_updates(contents, &updates);

        assert!(result.contains("rojo = \"rojo-rbx/rojo@7.7.0\""));
        assert!(!result.contains("7.0.0"), "old spec must be gone");
        assert!(
            result.contains("wally = \"UpliftGames/wally@0.3.2\""),
            "untouched tools must be preserved"
        );
        assert!(result.contains("# leading comment"));
        assert!(result.contains("# trailing comment"));
        assert!(result.ends_with('\n'), "trailing newline must be preserved");
    }

    #[test]
    fn apply_tool_updates_only_touches_tools_table() {
        let contents = "\
[other]
rojo = \"rojo-rbx/rojo@7.0.0\"

[tools]
rojo = \"rojo-rbx/rojo@7.0.0\"
";
        let updates = vec![ToolUpdate {
            name: "rojo".to_string(),
            old_spec: "rojo-rbx/rojo@7.0.0".to_string(),
            new_spec: "rojo-rbx/rojo@7.7.0".to_string(),
        }];

        let result = apply_tool_updates(contents, &updates);

        assert_eq!(
            result.matches("7.0.0").count(),
            1,
            "the look-alike entry outside [tools] must be left untouched"
        );
        assert_eq!(
            result.matches("7.7.0").count(),
            1,
            "only the [tools] entry should be bumped"
        );
    }

    #[test]
    fn apply_tool_updates_is_noop_without_updates() {
        let contents = "[tools]\nrojo = \"rojo-rbx/rojo@7.7.0\"\n";
        assert_eq!(apply_tool_updates(contents, &[]), contents);
    }
}
