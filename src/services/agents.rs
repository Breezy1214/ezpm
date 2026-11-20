use anyhow::{Context, Result};
use serde_json::{json, Value};

pub const AGENTS_FILE: &str = "AGENTS.md";
pub const CLAUDE_FILE: &str = "CLAUDE.md";
pub const CLAUDE_SETTINGS_FILE: &str = ".claude/settings.json";
pub const HOOK_COMMAND: &str = "ezpm hook stop";

const START_MARKER: &str = "<!-- ezpm -->";
const END_MARKER: &str = "<!-- /ezpm -->";

pub const RULES_BLOCK: &str = r#"<!-- ezpm -->
## ezpm

This repo uses ezpm, which runs Rojo, Wally, Selene and StyLua behind one CLI. `ezpm.toml` is the source of truth: its `[aliases]` generate `.luaurc`, and the Rojo project file decides where each file lands in the game.

After you add, move, rename or delete Luau files, or change their requires, run `ezpm check <paths>` and `ezpm fix-requires --check <paths>` from the repo root. Fix each line they print, or tell the user why it stays. When `fix-requires --check` reports a rewrite, run `ezpm fix-requires`.

### Requires

- Require a module by its `@game/...` path or an alias from `ezpm.toml`, such as `@Shared/Signal`. `ezpm fix-requires` rewrites bare names like `require("Signal")` to `@game/...`, and stops when two modules share a name.
- Run `ezpm where <path>` to see where a file lands in the game and the exact require for it.
- Respect the `[[check.forbid]]` layer rules (for example, client never requires server) and, when set, `[check] feature_roots`: a feature requires only itself and the `shared_features`.

### Setup

- Edit `ezpm.toml`, never `.luaurc`, which ezpm rewrites. Add an alias with `ezpm alias add <Name> <path>`.
- Don't edit `Packages/` or `ServerPackages/`; `ezpm install` replaces them.
- `ezpm explain <code>` says what a diagnostic code means, and every command takes `--json`.
<!-- /ezpm -->
"#;

/// `AGENTS.md` with ezpm's rules block added, or an outdated block replaced in place.
/// `None` when the file already holds the current block.
pub fn upsert_rules(existing: Option<&str>) -> Option<String> {
    let Some(existing) = existing else {
        return Some(RULES_BLOCK.to_string());
    };

    let updated = match (existing.find(START_MARKER), existing.find(END_MARKER)) {
        (Some(start), Some(end)) if start < end => {
            let mut after = &existing[end + END_MARKER.len()..];
            after = after.strip_prefix('\n').unwrap_or(after);
            format!("{}{RULES_BLOCK}{after}", &existing[..start])
        }
        _ => {
            let separator = match existing {
                "" => "",
                text if text.ends_with("\n\n") => "",
                text if text.ends_with('\n') => "\n",
                _ => "\n\n",
            };
            format!("{existing}{separator}{RULES_BLOCK}")
        }
    };
    (updated != existing).then_some(updated)
}

/// `CLAUDE.md` with an `@AGENTS.md` import, so Claude Code reads the rules.
/// `None` when it already imports them.
pub fn with_claude_import(existing: Option<&str>) -> Option<String> {
    match existing {
        Some(text) if text.lines().any(|line| line.trim() == "@AGENTS.md") => None,
        Some(text) if text.is_empty() || text.ends_with('\n') => {
            Some(format!("{text}@AGENTS.md\n"))
        }
        Some(text) => Some(format!("{text}\n@AGENTS.md\n")),
        None => Some("@AGENTS.md\n".to_string()),
    }
}

/// `.claude/settings.json` with `ezpm hook stop` registered as a Stop hook, keeping every other
/// setting. `None` when it is already registered.
pub fn with_claude_stop_hook(existing: Option<&str>) -> Result<Option<String>> {
    let mut settings: Value = match existing {
        Some(text) if !text.trim().is_empty() => {
            serde_json::from_str(text).context("could not parse .claude/settings.json")?
        }
        _ => json!({}),
    };

    let stop = settings
        .as_object_mut()
        .context(".claude/settings.json must be a JSON object")?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("\"hooks\" in .claude/settings.json must be an object")?
        .entry("Stop")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .context("\"hooks.Stop\" in .claude/settings.json must be an array")?;

    let registered = stop.iter().any(|group| {
        group["hooks"]
            .as_array()
            .is_some_and(|hooks| hooks.iter().any(|hook| hook["command"] == HOOK_COMMAND))
    });
    if registered {
        return Ok(None);
    }
    stop.push(json!({ "hooks": [{ "type": "command", "command": HOOK_COMMAND }] }));

    let mut text = serde_json::to_string_pretty(&settings)?;
    text.push('\n');
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_are_appended_once_and_replaced_in_place() {
        let first = upsert_rules(Some("# Notes\n")).unwrap();
        assert!(first.starts_with("# Notes\n\n<!-- ezpm -->"));
        assert_eq!(upsert_rules(Some(&first)), None);

        let outdated = "intro\n<!-- ezpm -->\nold rules\n<!-- /ezpm -->\noutro\n";
        let replaced = upsert_rules(Some(outdated)).unwrap();
        assert!(replaced.starts_with("intro\n<!-- ezpm -->\n## ezpm"));
        assert!(replaced.ends_with("<!-- /ezpm -->\noutro\n"));
        assert!(!replaced.contains("old rules"));
    }

    #[test]
    fn claude_import_is_added_once() {
        assert_eq!(with_claude_import(None).unwrap(), "@AGENTS.md\n");
        assert_eq!(
            with_claude_import(Some("# Me")).unwrap(),
            "# Me\n@AGENTS.md\n"
        );
        assert_eq!(with_claude_import(Some("x\n@AGENTS.md\n")), None);
    }

    #[test]
    fn stop_hook_keeps_other_settings_and_is_idempotent() {
        let existing = r#"{"permissions":{"allow":["Bash(ls)"]},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other"}]}]}}"#;
        let updated = with_claude_stop_hook(Some(existing)).unwrap().unwrap();
        let value: Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(value["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(value["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(with_claude_stop_hook(Some(&updated)).unwrap(), None);
    }
}
