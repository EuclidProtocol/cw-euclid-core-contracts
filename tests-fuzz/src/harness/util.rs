use std::error::Error as StdError;

/// Extract the innermost error message from an error chain.
#[allow(dead_code)]
pub(crate) fn root_cause(e: &dyn StdError) -> String {
    let mut current: &dyn StdError = e;
    while let Some(source) = current.source() {
        current = source;
    }
    current.to_string()
}
