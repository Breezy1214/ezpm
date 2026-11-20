use owo_colors::{OwoColorize, Stream};
use serde::Serialize;

use crate::output;

const DOCS_URL: &str = "https://github.com/Breezy1214/ezpm/blob/main/docs/diagnostics.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

pub struct CodeInfo {
    pub code: &'static str,
    pub severity: Severity,
    pub summary: &'static str,
    pub explanation: &'static str,
}

pub const CODES: &[CodeInfo] = &[
    CodeInfo {
        code: "config.unknownField",
        severity: Severity::Warning,
        summary: "ezpm.toml has a key ezpm does not know",
        explanation: "ezpm ignores the key, so the setting it was meant to change keeps its default. It is usually a typo; the message suggests the closest known key when there is one. Rename the key or remove it.",
    },
    CodeInfo {
        code: "config.invalid",
        severity: Severity::Error,
        summary: "ezpm.toml could not be read or parsed",
        explanation: "The file is not valid TOML, or a value has the wrong type (for example a string where `[serve] port` needs a number). Fix the line the message names. Commands other than `init` stop instead of running on defaults, so they never overwrite `.luaurc` from a broken config.",
    },
    CodeInfo {
        code: "check.cycle",
        severity: Severity::Error,
        summary: "modules require each other in a loop",
        explanation: "Luau cannot load a module that is still loading, so a require cycle fails at runtime with a recursive-require error. Break the loop: move what both modules need into a third module, or pass the dependency in instead of requiring it.",
    },
    CodeInfo {
        code: "check.forbiddenImport",
        severity: Severity::Error,
        summary: "a require crosses a layer boundary that `[[check.forbid]]` forbids",
        explanation: "The requiring module is in one `[check.layers]` layer and the required module in another, and a `[[check.forbid]]` rule forbids that direction (for example client requiring server). Move the shared code into a layer both may use, or remove the require.",
    },
    CodeInfo {
        code: "check.featureBoundary",
        severity: Severity::Error,
        summary: "one feature requires another feature",
        explanation: "With `[check] feature_roots`, every folder directly under a root is a feature, and a feature may only require itself and the features listed in `shared_features`. Move the shared code into a shared feature, list the target in `shared_features`, or remove the require.",
    },
    CodeInfo {
        code: "check.unusedModule",
        severity: Severity::Warning,
        summary: "no entry point reaches this module",
        explanation: "Nothing reachable from `[check] entry_points` (or `src/<layer>/init.luau` when unset) requires the module, so it never runs. Delete it, require it from where it is needed, or add it to `entry_points` if it is loaded another way.",
    },
    CodeInfo {
        code: "require.ambiguous",
        severity: Severity::Error,
        summary: "a bare require matches more than one module",
        explanation: "`require(\"Name\")` names a module by its file name, and several modules share that name, so ezpm cannot tell which one is meant and changes nothing. Use an alias (`@Shared/Name`) or the full `@game/...` path for this require.",
    },
    CodeInfo {
        code: "require.wouldRewrite",
        severity: Severity::Error,
        summary: "a require is not in the form `fix-requires` writes",
        explanation: "`ezpm fix-requires --check` found a require that `ezpm fix-requires` would rewrite, such as a bare name or an alias, or a path to a module that moved. Run `ezpm fix-requires` (or `ezpm serve`) and commit the result.",
    },
    CodeInfo {
        code: "where.unmapped",
        severity: Severity::Error,
        summary: "the file is not in the Rojo project",
        explanation: "Rojo's sourcemap has no instance for this path, so the file is not synced into the game. Check that the file exists, that its folder is mounted by a `$path` in the Rojo project, and that it is not ignored by `globIgnorePaths`.",
    },
    CodeInfo {
        code: "tool.missing",
        severity: Severity::Error,
        summary: "a required tool is not installed",
        explanation: "ezpm runs Rojo, Selene, StyLua, Wally and Rokit as separate programs, and one of them could not be started. Install Rokit, then run `ezpm install` to fetch the tools that `rokit.toml` pins.",
    },
    CodeInfo {
        code: "tool.failed",
        severity: Severity::Error,
        summary: "an external tool exited with an error",
        explanation: "A tool ezpm ran (Rojo, Selene, StyLua, Wally or Rokit) reported a failure. Its own output, printed above or in `stderr` of the JSON document, says what went wrong.",
    },
];

pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    CODES
        .iter()
        .find(|info| info.code.eq_ignore_ascii_case(code))
}

pub fn docs_url(code: &str) -> String {
    let anchor: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    format!("{DOCS_URL}#{anchor}")
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub url: String,
}

impl Diagnostic {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        let severity = lookup(code)
            .map(|info| info.severity)
            .unwrap_or(Severity::Error);
        Self {
            code,
            severity,
            message: message.into(),
            file: None,
            line: None,
            hint: None,
            url: docs_url(code),
        }
    }

    pub fn at(mut self, file: impl Into<String>, line: Option<usize>) -> Self {
        self.file = Some(file.into());
        self.line = line;
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// `file - severity: message (code)`, with `file:line` when the line is known.
    pub fn render(&self, severity: &str) -> String {
        let prefix = self
            .file
            .as_deref()
            .map(|file| format!("{} - ", location(file, self.line)))
            .unwrap_or_default();
        format!("{prefix}{severity}: {} ({})", self.message, self.code)
    }
}

/// `file:line`, or `file` when the line is unknown.
pub fn location(file: &str, line: Option<usize>) -> String {
    match line {
        Some(line) => format!("{file}:{line}"),
        None => file.to_string(),
    }
}

/// Prints each diagnostic as `file:line - severity: message (code)` on stderr, and as a GitHub
/// workflow command when running under GitHub Actions. Prints nothing in JSON mode.
pub fn report(diagnostics: &[Diagnostic]) {
    if output::is_json() {
        return;
    }
    for diagnostic in diagnostics {
        if !diagnostic.is_error() && output::is_quiet() {
            continue;
        }
        let severity = diagnostic.severity.label();
        let severity = match diagnostic.severity {
            Severity::Error => severity
                .if_supports_color(Stream::Stderr, |t| t.red())
                .to_string(),
            Severity::Warning => severity
                .if_supports_color(Stream::Stderr, |t| t.yellow())
                .to_string(),
        };
        eprintln!("{}", diagnostic.render(&severity));
        if let Some(hint) = &diagnostic.hint {
            output::hint(hint);
        }
    }
    annotate(diagnostics);
}

/// Writes GitHub Actions workflow commands so diagnostics show on the pull request's files.
pub fn annotate(diagnostics: &[Diagnostic]) {
    if output::is_json() || std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        return;
    }
    for diagnostic in diagnostics {
        if !diagnostic.is_error() && output::is_quiet() {
            continue;
        }
        eprintln!("{}", workflow_command(diagnostic));
    }
}

fn workflow_command(diagnostic: &Diagnostic) -> String {
    let mut properties = Vec::new();
    if let Some(file) = &diagnostic.file {
        let file = workspace_path(file);
        properties.push(format!("file={}", escape_property(&file)));
    }
    if let Some(line) = diagnostic.line {
        properties.push(format!("line={line}"));
    }
    properties.push(format!("title={}", escape_property(diagnostic.code)));
    format!(
        "::{} {}::{}",
        diagnostic.severity.label(),
        properties.join(","),
        escape_data(&diagnostic.message)
    )
}

/// GitHub resolves annotation paths against `GITHUB_WORKSPACE`, so a project in a subfolder of
/// the repository needs that folder in front of its project-relative paths.
fn workspace_path(file: &str) -> String {
    let prefix = std::env::var_os("GITHUB_WORKSPACE").and_then(|workspace| {
        let workspace = std::fs::canonicalize(workspace).ok()?;
        let project = std::env::current_dir().ok()?.canonicalize().ok()?;
        let relative = project.strip_prefix(workspace).ok()?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        (!relative.is_empty()).then_some(relative)
    });
    match prefix {
        Some(prefix) => format!("{prefix}/{file}"),
        None => file.to_string(),
    }
}

fn escape_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_property(value: &str) -> String {
    escape_data(value).replace(':', "%3A").replace(',', "%2C")
}

/// The candidate closest to `input`, when it is close enough to be a likely typo.
pub fn closest<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let input = input.to_ascii_lowercase();
    let length = input.chars().count();
    // One edit for very short keys, so `id` doesn't become `to`; a transposition otherwise.
    let max_distance = if length <= 3 { 1 } else { (length / 3).max(2) };
    candidates
        .iter()
        .map(|candidate| {
            (
                levenshtein(&input, &candidate.to_ascii_lowercase()),
                *candidate,
            )
        })
        .filter(|(distance, _)| *distance <= max_distance)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != *cb);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_a_docs_section() {
        let docs = include_str!("../docs/diagnostics.md");
        for info in CODES {
            assert!(
                docs.contains(&format!("## `{}`", info.code)),
                "docs/diagnostics.md has no section for {}",
                info.code
            );
        }
    }

    #[test]
    fn closest_suggests_typos_only() {
        let keys = ["entry_points", "layers", "forbid"];
        assert_eq!(closest("entry_point", &keys), Some("entry_points"));
        assert_eq!(closest("layer", &keys), Some("layers"));
        assert_eq!(closest("unrelated", &keys), None);
        assert_eq!(closest("foo", &["from", "to"]), None);
        assert_eq!(closest("id", &["from", "to"]), None);
    }

    #[test]
    fn workflow_command_escapes_properties_and_data() {
        let diagnostic = Diagnostic::new("check.cycle", "a: b\n100%").at("src/a,b.luau", Some(3));
        assert_eq!(
            workflow_command(&diagnostic),
            "::error file=src/a%2Cb.luau,line=3,title=check.cycle::a: b%0A100%25"
        );
    }

    #[test]
    fn docs_url_uses_github_anchor() {
        assert!(docs_url("check.cycle").ends_with("#checkcycle"));
    }
}
