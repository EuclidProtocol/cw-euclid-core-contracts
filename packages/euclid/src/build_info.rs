//! Build provenance embedded into every contract via the `euclid` dependency.
//! Values are injected at compile time by `build.rs`. See that file for the
//! resolution precedence (env var > git > "unknown").

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{StdResult, Storage};
use cw_storage_plus::Item;

/// Full git commit the artifact was built from, e.g. `"a1b2c3d...f0"` or
/// `"a1b2c3d...f0 (dirty)"` when built from a worktree with uncommitted changes.
pub const BUILD_COMMIT: &str = env!("EUCLID_BUILD_COMMIT");

/// Commit timestamp of [`BUILD_COMMIT`] in ISO-8601 (e.g. `"2026-06-29T16:42:00+00:00"`).
/// This is the commit time, not wall-clock build time, so the same commit
/// always yields the same value (and thus a reproducible wasm checksum).
pub const BUILD_TIME: &str = env!("EUCLID_BUILD_TIME");

/// Build provenance persisted into contract state at instantiate (and migrate).
/// Unlike the compile-time [`BUILD_COMMIT`]/[`BUILD_TIME`] constants, which always
/// reflect the *currently deployed* code, this records the values captured when the
/// contract was instantiated, so a later migration cannot silently rewrite them.
#[cw_serde]
pub struct StoredBuildInfo {
    /// See [`BUILD_COMMIT`].
    pub build_commit: String,
    /// See [`BUILD_TIME`].
    pub build_time: String,
}

/// State item holding the persisted build provenance.
pub const BUILD_INFO: Item<StoredBuildInfo> = Item::new("euclid_build_info");

/// Persist compile-time build provenance into contract state. Call from every
/// contract's `instantiate` (and `migrate`), alongside `set_contract_version`.
pub fn set_build_info(storage: &mut dyn Storage) -> StdResult<()> {
    BUILD_INFO.save(
        storage,
        &StoredBuildInfo {
            build_commit: BUILD_COMMIT.to_string(),
            build_time: BUILD_TIME.to_string(),
        },
    )
}

/// Provenance of a deployed contract: its semver plus the commit it was built
/// from. Returned by each contract's `GetBuildInfo {}` query.
#[cw_serde]
pub struct BuildInfoResponse {
    /// The contract's `CARGO_PKG_VERSION`.
    pub contract_version: String,
    /// See [`BUILD_COMMIT`].
    pub build_commit: String,
    /// See [`BUILD_TIME`].
    pub build_time: String,
}

/// Build a [`BuildInfoResponse`] for a contract. Pass the contract's
/// `CONTRACT_VERSION` const (its `CARGO_PKG_VERSION`).
///
/// The commit and timestamp are read from state ([`BUILD_INFO`]), i.e. the values
/// captured at instantiate/migrate. Contracts predating this feature (no stored
/// value) fall back to the compile-time constants.
pub fn build_info(storage: &dyn Storage, contract_version: &str) -> BuildInfoResponse {
    let (build_commit, build_time) = match BUILD_INFO.may_load(storage).ok().flatten() {
        Some(stored) => (stored.build_commit, stored.build_time),
        None => (BUILD_COMMIT.to_string(), BUILD_TIME.to_string()),
    };
    BuildInfoResponse {
        contract_version: contract_version.to_string(),
        build_commit,
        build_time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::MockStorage;

    #[test]
    fn build_info_falls_back_to_constants_when_unset() {
        let storage = MockStorage::new();
        let res = build_info(&storage, "9.9.9");
        assert_eq!(res.contract_version, "9.9.9");
        // Injected by build.rs (env var, git, or the "unknown" fallback); never empty.
        assert_eq!(res.build_commit, BUILD_COMMIT);
        assert_eq!(res.build_time, BUILD_TIME);
    }

    #[test]
    fn build_info_reads_persisted_state() {
        let mut storage = MockStorage::new();
        set_build_info(&mut storage).unwrap();
        let res = build_info(&storage, "9.9.9");
        assert_eq!(res.contract_version, "9.9.9");
        assert_eq!(res.build_commit, BUILD_COMMIT);
        assert_eq!(res.build_time, BUILD_TIME);
        assert!(!res.build_commit.is_empty());
        assert!(!res.build_time.is_empty());
    }
}
