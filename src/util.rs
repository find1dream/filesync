use std::cmp::Ordering;
use std::collections::HashSet;

pub fn sort_dir_first(a_is_dir: bool, a_name: &str, b_is_dir: bool, b_name: &str) -> Ordering {
    match (a_is_dir, b_is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a_name.to_lowercase().cmp(&b_name.to_lowercase()),
    }
}

/// Pure alphabetical — dirs and files interleaved.
pub fn sort_name_mixed(a_name: &str, b_name: &str) -> Ordering {
    a_name.to_lowercase().cmp(&b_name.to_lowercase())
}

/// Largest size first; entries with equal size fall back to name.
pub fn sort_size_desc(a_size: u64, a_name: &str, b_size: u64, b_name: &str) -> Ordering {
    b_size.cmp(&a_size).then_with(|| a_name.to_lowercase().cmp(&b_name.to_lowercase()))
}

pub fn selected_entries<'a, T>(
    entries: &'a [T],
    cursor: usize,
    selected: &HashSet<usize>,
) -> Vec<&'a T> {
    if selected.is_empty() {
        entries.get(cursor).into_iter().collect()
    } else {
        let mut v: Vec<usize> = selected.iter().cloned().collect();
        v.sort_unstable();
        v.iter().filter_map(|i| entries.get(*i)).collect()
    }
}
