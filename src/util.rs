use std::cmp::Ordering;
use std::collections::HashSet;

pub fn sort_dir_first(a_is_dir: bool, a_name: &str, b_is_dir: bool, b_name: &str) -> Ordering {
    match (a_is_dir, b_is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a_name.to_lowercase().cmp(&b_name.to_lowercase()),
    }
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
