use std::error::Error as StdError;

use cw_orch::prelude::CwOrchError;

/// Extract the innermost error message from a CwOrchError chain.
///
/// Uses multiple strategies to dig through cw-multi-test's error wrapping:
/// 1. Parse Debug output for the deepest non-WasmMsg "Error - " line
/// 2. Search for "error type: " markers (ContractError, StdError)
/// 3. Fall back to .root() or source() chain walking
pub(crate) fn root_cause(e: &CwOrchError) -> String {
    let full = format!("{:?}", e);

    // The Debug format is a chain of "Caused by:" sections. The actual contract
    // error is the last "Caused by:" entry — a bare message like:
    //   "Caused by:\n            Cannot Sub with given operands)"
    // or with an "Error - " prefix for non-submsg errors.
    //
    // Walk all "Caused by:" sections and take the last non-WasmMsg message.
    let mut best = String::new();
    for section in full.split("Caused by:") {
        let trimmed = section.trim().trim_end_matches(')').trim();
        if trimmed.is_empty() {
            continue;
        }
        // Skip WasmMsg wrapper sections
        if trimmed.starts_with("Error - Error executing WasmMsg")
            || trimmed.starts_with("Error executing WasmMsg")
        {
            continue;
        }
        // Strip "Action - ..., Error - " prefix from router reply sections
        if let Some(idx) = trimmed.find(", Error - ") {
            let after = &trimmed[idx + ", Error - ".len()..];
            if !after.starts_with("Error executing WasmMsg") {
                best = after.to_string();
                continue;
            }
        }
        // Strip "Error - " prefix
        if let Some(msg) = trimmed.strip_prefix("Error - ") {
            best = msg.to_string();
        } else {
            best = trimmed.to_string();
        }
    }

    if best.is_empty() {
        // Fallback: walk source chain
        let mut current: &dyn StdError = e;
        while let Some(source) = current.source() {
            current = source;
        }
        best = current.to_string();
    }

    best.trim_end_matches(')').trim().to_string()
}
