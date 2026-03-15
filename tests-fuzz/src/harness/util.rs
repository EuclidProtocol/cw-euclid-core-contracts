use std::error::Error as StdError;

use cw_orch::prelude::CwOrchError;

/// Extract the innermost error message from a CwOrchError chain.
/// Walks the full debug string to find the deepest "Error - " message,
/// since anyhow wraps contract errors as strings inside WasmMsg layers.
///
/// NOTE: This intentionally parses the Debug format because CwOrchError
/// wraps contract errors through multiple layers of anyhow::Error and
/// WasmMsg, with no structured API to reach the original contract error.
/// The source() chain stops at the anyhow boundary, so Debug parsing is
/// the only reliable way to extract the actual contract error message.
#[allow(dead_code)]
pub(crate) fn root_cause(e: &CwOrchError) -> String {
    let full = format!("{:?}", e);
    // Look for the last "Error - <message>" that isn't a WasmMsg wrapper
    let mut best = String::new();
    for line in full.lines() {
        let trimmed = line.trim();
        if let Some(msg) = trimmed.strip_prefix("Error - ") {
            if !msg.starts_with("Error executing WasmMsg") {
                best = msg.to_string();
            }
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
    // Trim trailing ')' from anyhow Debug formatting artifacts
    best.trim_end_matches(')').trim().to_string()
}
