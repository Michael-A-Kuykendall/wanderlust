use std::collections::HashSet;
use std::sync::Mutex;
use lazy_static::lazy_static;
use log::{error, info};

lazy_static! {
    /// Stores the set of unique invariant keys (descriptions) that have been successfully asserted.
    static ref CHECKED_INVARIANTS: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

/// Asserts that a critical system invariant holds true.
/// 
/// If the condition is false, this will panic (in debug/test) or log a critical error (in prod).
/// If true, it records that this invariant was explicitly checked, allowing for "Contract Tests".
///
/// # Arguments
/// * `condition` - The boolean result of the check.
/// * `description` - A human-readable description of the invariant (e.g., "PATH must contain System32").
/// * `component` - Optional component tag (e.g., "Cleaner", "Discovery").
pub fn assert_invariant(condition: bool, description: &str, component: Option<&str>) {
    if !condition {
        let msg = format!(
            "CRITICAL INVARIANT VIOLATION [{}]: {}", 
            component.unwrap_or("General"), 
            description
        );
        error!("{}", msg);
        
        // In test/debug, we want to crash immediately to catch this.
        // In production, we might want to survive, but for now, let's panic to be safe.
        // "Fail Closed" is usually safer for system tools.
        if cfg!(debug_assertions) || cfg!(test) {
            panic!("{}", msg);
        }
    } else {
        // Record that we checked this
        if let Ok(mut set) = CHECKED_INVARIANTS.lock() {
            set.insert(description.to_string());
        }
    }
}

/// A "Contract Test" verifies that specific invariants were actually checked during execution.
/// 
/// This ensures that your business logic actually enforces the rules you think it does.
///
/// # Arguments
/// * `context` - Name of the test context.
/// * `required_invariants` - List of description strings that MUST have been asserted.
#[allow(dead_code)]
pub fn contract_test(context: &str, required_invariants: &[&str]) {
    let checked = CHECKED_INVARIANTS.lock().unwrap();
    let mut missing = Vec::new();

    for &req in required_invariants {
        if !checked.contains(req) {
            missing.push(req);
        }
    }

    if !missing.is_empty() {
        panic!(
            "Contract Test Failed for '{}'. The following invariants were NOT checked:\n{:#?}",
            context, missing
        );
    } else {
        info!("Contract Test Passed: {}", context);
    }
}

/// Clears the invariant log. Call this before running a new isolated test.
#[allow(dead_code)]
pub fn clear_invariant_log() {
    if let Ok(mut set) = CHECKED_INVARIANTS.lock() {
        set.clear();
    }
}
