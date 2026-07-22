use lazy_static::lazy_static;
use log::error;
use std::collections::HashSet;
use std::sync::Mutex;

lazy_static! {
    static ref CHECKED_INVARIANTS: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

pub fn assert_invariant(condition: bool, description: &str, component: Option<&str>) {
    if !condition {
        let msg = format!(
            "CRITICAL INVARIANT VIOLATION [{}]: {}",
            component.unwrap_or("General"),
            description
        );
        error!("{}", msg);

        if cfg!(debug_assertions) || cfg!(test) {
            panic!("{}", msg);
        }
    } else if let Ok(mut set) = CHECKED_INVARIANTS.lock() {
        set.insert(description.to_string());
    }
}

#[cfg(test)]
pub fn clear_invariant_log() {
    if let Ok(mut set) = CHECKED_INVARIANTS.lock() {
        set.clear();
    }
}
