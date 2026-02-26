use regex::Regex;
use std::path::Path;
use std::process::Command;

use crate::output;

const ROBLOX_YML_PATH: &str = "roblox.yml";

pub fn generate_selene_roblox_std() {
    let output_result = Command::new("selene").arg("generate-roblox-std").output();

    let output = match output_result {
        Ok(output) => output,
        Err(e) => {
            output::warn(&format!("Failed to run selene generate-roblox-std: {}", e));
            return;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() {
            output::warn("selene generate-roblox-std failed");
        } else {
            output::warn(&format!(
                "selene generate-roblox-std failed: {}",
                stderr.trim()
            ));
        }
        return;
    }

    if let Err(e) = patch_roblox_yml(Path::new(ROBLOX_YML_PATH)) {
        output::warn(&format!("Failed to update roblox.yml: {}", e));
    }
}

fn patch_roblox_yml(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string(path)?;
    let patched = patch_require_signature(&contents);

    if patched != contents {
        std::fs::write(path, patched)?;
    }

    Ok(())
}

fn patch_require_signature(contents: &str) -> String {
    let requires_string = contents.contains("- type: string");
    if requires_string {
        return contents.to_string();
    }

    let newline = if contents.contains("\r\n") { "\r\n" } else { "\n" };
    let replacement = format!(
        "  require:{nl}    args:{nl}    - type: number{nl}  require:{nl}    args:{nl}    - type: string",
        nl = newline
    );

    let require_number = Regex::new(r"(?m)^  require:\r?\n    args:\r?\n    - type: number")
        .expect("regex must compile");

    require_number
        .replace(contents, replacement.as_str())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::patch_require_signature;

    #[test]
    fn adds_string_require_signature_after_number_signature() {
        let input = "globals:\n  require:\n    args:\n    - type: number\n";
        let output = patch_require_signature(input);

        assert!(output.contains("- type: number"));
        assert!(output.contains("- type: string"));
    }

    #[test]
    fn does_not_duplicate_string_signature() {
        let input = "globals:\n  require:\n    args:\n    - type: number\n  require:\n    args:\n    - type: string\n";
        let output = patch_require_signature(input);

        let string_count = output.matches("- type: string").count();
        assert_eq!(string_count, 1);
    }
}