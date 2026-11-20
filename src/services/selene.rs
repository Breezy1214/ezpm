use regex::Regex;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use crate::output;
use crate::services::atomic_file;

const ROBLOX_YML_PATH: &str = "roblox.yml";

const SELENE_CONFIG_PATH: &str = "selene.toml";
const REGENERATE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// Generation downloads the Roblox API dump, so skip it without selene.toml or while
/// roblox.yml is recent. A failed attempt refreshes an existing roblox.yml's timestamp, so
/// an offline machine does not retry the download on every command; with no roblox.yml
/// yet, selene has nothing to lint against, so it keeps trying.
pub fn generate_selene_roblox_std() {
    let roblox_yml = Path::new(ROBLOX_YML_PATH);
    if !Path::new(SELENE_CONFIG_PATH).exists() || is_fresh(roblox_yml) {
        return;
    }

    let failure = match Command::new("selene").arg("generate-roblox-std").output() {
        Err(e) => Some(format!("Failed to run selene generate-roblox-std: {}", e)),
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Some(if stderr.trim().is_empty() {
                "selene generate-roblox-std failed".to_string()
            } else {
                format!("selene generate-roblox-std failed: {}", stderr.trim())
            })
        }
        Ok(_) => None,
    };

    if let Some(message) = failure {
        output::warn(&message);
        let _ = std::fs::File::options()
            .write(true)
            .open(roblox_yml)
            .and_then(|file| file.set_modified(SystemTime::now()));
        return;
    }

    if let Err(e) = patch_roblox_yml(roblox_yml) {
        output::warn(&format!("Failed to update roblox.yml: {}", e));
    }
}

fn is_fresh(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < REGENERATE_AFTER)
}

fn patch_roblox_yml(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string(path)?;
    let patched = patch_require_signature(&contents);

    if patched != contents {
        atomic_file::write(path, patched)?;
    }

    Ok(())
}

/// Selene's generated std types `require` as taking a number, which flags string
/// requires. Widen the argument to `any`, also collapsing the duplicate `require:` key
/// that older ezpm versions emitted.
fn patch_require_signature(contents: &str) -> String {
    static REQUIRE_NUMBER: OnceLock<Regex> = OnceLock::new();
    let re = REQUIRE_NUMBER.get_or_init(|| {
        Regex::new(
            r"(?mR)^  require:(\r?\n)    args:\r?\n    - type: number(?:\r?\n  require:\r?\n    args:\r?\n    - type: string)?$",
        )
        .expect("regex must compile")
    });

    re.replace(contents, "  require:${1}    args:${1}    - type: any")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::patch_require_signature;

    #[test]
    fn widens_number_require_signature() {
        let input =
            "globals:\n  require:\n    args:\n    - type: number\n  script:\n    struct: Script\n";
        let output = patch_require_signature(input);

        assert_eq!(
            output,
            "globals:\n  require:\n    args:\n    - type: any\n  script:\n    struct: Script\n"
        );
    }

    #[test]
    fn patches_even_when_other_globals_take_strings() {
        let input = "globals:\n  print:\n    args:\n    - type: string\n  require:\n    args:\n    - type: number\n";
        let output = patch_require_signature(input);

        assert!(output.contains("  require:\n    args:\n    - type: any\n"));
    }

    #[test]
    fn collapses_legacy_duplicate_require_key() {
        let input = "globals:\n  require:\n    args:\n    - type: number\n  require:\n    args:\n    - type: string\n  script:\n    struct: Script\n";
        let output = patch_require_signature(input);

        assert_eq!(output.matches("  require:").count(), 1);
        assert!(output.contains("    - type: any\n  script:"));
    }

    #[test]
    fn preserves_crlf_and_is_idempotent() {
        let input = "globals:\r\n  require:\r\n    args:\r\n    - type: number\r\n";
        let once = patch_require_signature(input);

        assert_eq!(
            once,
            "globals:\r\n  require:\r\n    args:\r\n    - type: any\r\n"
        );
        assert_eq!(patch_require_signature(&once), once);
    }
}
