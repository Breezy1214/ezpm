use std::cmp::Ordering;

// ─── Public functions ─────────────────────────────────────────────────────────

/// Compare two semver version strings.
///
/// Splits on `.` and compares major, minor, patch components as u64.
/// Missing components default to 0. Leading `v` prefix is stripped if present.
///
/// This is sufficient for EZPM's version check in Phase 5 — Rokit tool
/// versions are simple semver strings (e.g. "1.2.3" or "v1.2.3").
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts_a = parse_version(a);
    let parts_b = parse_version(b);

    // Compare major, then minor, then patch
    for i in 0..3 {
        let comp = parts_a[i].cmp(&parts_b[i]);
        if comp != Ordering::Equal {
            return comp;
        }
    }

    Ordering::Equal
}

/// Returns true if `latest` is newer (greater) than `current`.
///
/// Convenience wrapper around `compare_versions`.
pub fn is_newer(current: &str, latest: &str) -> bool {
    compare_versions(latest, current) == Ordering::Greater
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Parse a version string into [major, minor, patch] as u64 values.
///
/// Strips a leading `v` or `V` prefix before parsing.
/// Missing or unparseable components default to 0.
fn parse_version(version: &str) -> [u64; 3] {
    // Strip optional leading 'v' or 'V' prefix
    let trimmed = version.trim_start_matches(|c| c == 'v' || c == 'V');

    let mut parts = [0u64; 3];
    for (i, component) in trimmed.splitn(3, '.').enumerate() {
        if i >= 3 {
            break;
        }
        // Parse the component as u64, defaulting to 0 on failure
        parts[i] = component.parse().unwrap_or(0);
    }
    parts
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn test_equal_versions() {
        assert_eq!(
            compare_versions("1.2.3", "1.2.3"),
            Ordering::Equal,
            "identical versions must be Equal"
        );
    }

    #[test]
    fn test_major_difference() {
        assert_eq!(
            compare_versions("2.0.0", "1.0.0"),
            Ordering::Greater,
            "2.0.0 must be Greater than 1.0.0"
        );
    }

    #[test]
    fn test_minor_difference() {
        assert_eq!(
            compare_versions("1.3.0", "1.2.0"),
            Ordering::Greater,
            "1.3.0 must be Greater than 1.2.0"
        );
    }

    #[test]
    fn test_patch_difference() {
        assert_eq!(
            compare_versions("1.2.4", "1.2.3"),
            Ordering::Greater,
            "1.2.4 must be Greater than 1.2.3"
        );
    }

    #[test]
    fn test_newer_returns_true() {
        assert!(
            is_newer("1.0.0", "1.1.0"),
            "is_newer(1.0.0, 1.1.0) must be true"
        );
    }

    #[test]
    fn test_not_newer_returns_false() {
        assert!(
            !is_newer("1.1.0", "1.0.0"),
            "is_newer(1.1.0, 1.0.0) must be false"
        );
    }

    #[test]
    fn test_handles_missing_patch() {
        assert_eq!(
            compare_versions("1.2", "1.2.0"),
            Ordering::Equal,
            "1.2 and 1.2.0 must be Equal (missing patch defaults to 0)"
        );
    }

    #[test]
    fn test_handles_v_prefix() {
        assert_eq!(
            compare_versions("v1.2.3", "1.2.3"),
            Ordering::Equal,
            "v-prefixed and non-prefixed must compare equal"
        );
        assert_eq!(
            compare_versions("v2.0.0", "v1.0.0"),
            Ordering::Greater,
            "v2.0.0 must be Greater than v1.0.0"
        );
    }
}
