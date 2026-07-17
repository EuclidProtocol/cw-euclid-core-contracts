/// Protocol version stamped into the send-packet event (see docs/send-packet-encoded.md).
/// Bump rules: any change to an ABI tuple shape, an enum tag table, or the JSON
/// schema of a wire message is a version bump.
pub const PROTOCOL_VERSION: &str = "0.0.1";

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression pin against an accidental bump: docs/send-packet-encoded.md
    /// §3 stamps this literal into the `version` attribute of every
    /// `euclid-send-packet-encoded` event, so a change here is a wire
    /// protocol change and must be deliberate, not a drive-by edit.
    #[test]
    fn protocol_version_is_pinned() {
        assert_eq!(PROTOCOL_VERSION, "0.0.1");
    }
}
