use std::cmp::Ordering;

pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts_a = parse_version(a);
    let parts_b = parse_version(b);

    for i in 0..3 {
        let comp = parts_a[i].cmp(&parts_b[i]);
        if comp != Ordering::Equal {
            return comp;
        }
    }

    Ordering::Equal
}

pub fn is_newer(current: &str, latest: &str) -> bool {
    compare_versions(latest, current) == Ordering::Greater
}

fn parse_version(version: &str) -> [u64; 3] {
    let trimmed = version.trim_start_matches(['v', 'V']);

    let mut parts = [0u64; 3];
    for (i, component) in trimmed.splitn(3, '.').enumerate() {
        if i >= 3 {
            break;
        }
        parts[i] = component.parse().unwrap_or(0);
    }
    parts
}

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
