use std::error::Error as StdError;

use cw_orch::prelude::CwOrchError;

/// Extract the innermost error message from a CwOrchError chain.
///
/// Uses cw-orch's built-in `.root()` for `AnyError` variants (contract errors),
/// which delegates to `anyhow::Error::root_cause()`. Falls back to walking the
/// `source()` chain for other variants (chain-level errors like missing addresses
/// in submsg calls) where `.root()` would panic.
#[allow(dead_code)]
pub(crate) fn root_cause(e: &CwOrchError) -> String {
    // CwEnvError::root() only handles the AnyError variant and panics otherwise.
    // Contract execution errors always flow through AnyError, so try that first.
    if let CwOrchError::AnyError(_) = e {
        return e.root().to_string();
    }

    // Fallback for non-AnyError variants: walk the source chain manually.
    let mut current: &dyn StdError = e;
    while let Some(source) = current.source() {
        current = source;
    }
    current.to_string()
}
