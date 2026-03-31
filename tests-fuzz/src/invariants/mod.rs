pub mod concentrated;
pub mod shared;

use cosmwasm_std::Uint128;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, ProtocolFeesResponse, Slot0Response, TickResponse,
};

/// Snapshot of pool state at a point in time, used for invariant checking
#[derive(Debug, Clone)]
pub struct PoolSnapshot {
    pub slot0: Slot0Response,
    pub positions: Vec<PositionResponse>,
    pub ticks: Vec<TickResponse>,
    pub protocol_fees: ProtocolFeesResponse,
    pub reserve_0: Uint128,
    pub reserve_1: Uint128,
}


/// Result of a single invariant check
#[derive(Debug, Clone)]
pub struct InvariantCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

impl InvariantCheck {
    /// Create a passing check result.
    pub fn pass(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            detail: String::new(),
        }
    }

    /// Create a failing check result with a detail message.
    pub fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            detail: detail.into(),
        }
    }
}

/// Collection of invariant check results
#[derive(Debug, Clone, Default)]
pub struct InvariantResult {
    pub checks: Vec<InvariantCheck>,
}

impl InvariantResult {
    /// Create an empty result with no checks.
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Append a single invariant check result.
    pub fn add(&mut self, check: InvariantCheck) {
        self.checks.push(check);
    }

    /// Combine another result's checks into this one.
    pub fn merge(&mut self, other: InvariantResult) {
        self.checks.extend(other.checks);
    }

    /// True if every check passed.
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    /// Panic with a detailed message if any check failed.
    pub fn assert_all_pass(&self) {
        let failures: Vec<_> = self.checks.iter().filter(|c| !c.passed).collect();
        if !failures.is_empty() {
            let msgs: Vec<String> = failures
                .iter()
                .map(|f| format!("  FAIL [{}]: {}", f.name, f.detail))
                .collect();
            panic!(
                "Invariant violations detected ({}/{} failed):\n{}",
                failures.len(),
                self.checks.len(),
                msgs.join("\n")
            );
        }
    }

    /// Collect the names of all failed checks.
    pub fn failed_names(&self) -> Vec<&str> {
        self.checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.name.as_str())
            .collect()
    }
}
