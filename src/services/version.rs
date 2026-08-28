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
