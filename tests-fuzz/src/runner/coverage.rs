use std::collections::HashMap;

use crate::invariants::InvariantResult;

/// Counts how many times each invariant was checked during a fuzz run.
/// Printed after runs to reveal which invariants are undertested.
#[derive(Debug, Clone, Default)]
pub struct InvariantCoverage {
    counts: HashMap<String, u64>,
}

impl InvariantCoverage {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    /// Record all checks from an invariant result.
    pub fn record(&mut self, result: &InvariantResult) {
        for check in &result.checks {
            *self.counts.entry(check.name.clone()).or_default() += 1;
        }
    }

    /// Merge another accumulator into this one.
    pub fn merge(&mut self, other: &InvariantCoverage) {
        for (name, count) in &other.counts {
            *self.counts.entry(name.clone()).or_default() += count;
        }
    }

    /// Print a sorted table of invariant check counts.
    pub fn print_report(&self) {
        if self.counts.is_empty() {
            return;
        }

        let mut entries: Vec<_> = self.counts.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));

        println!("\nInvariant Coverage:");
        println!("  {:<42} {:>7}", "invariant", "checked");
        println!("  {}", "-".repeat(51));
        for (name, count) in &entries {
            println!("  {:<42} {:>7}", name, count);
        }
    }
}
