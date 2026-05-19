use cosmwasm_std::{DepsMut, Env};
use cw_storage_plus::Map;

use crate::{cross_chain_user::CrossChainUser, error::ContractError};

/// Per-sender monotonic counter keyed by `CrossChainUser::to_sender_string()`.
///
/// A previous version used `Item<u128>` at key `"tx_nonce"` (a single global
/// counter). That value is no longer read or written and is left as orphan
/// storage; the new namespace prevents any collision with cw-storage-plus's
/// internal layout.
const TX_NONCES: Map<String, u128> = Map::new("tx_nonces");

/// Generates a deterministic identifier for a cross-chain transaction.
///
/// Format: `{sender}:{chain_id}:{nonce}`
///
/// # Reorg safety
///
/// The id is intentionally free of block-derived components (`block.height`,
/// `transaction.index`). All inputs are either external (`sender`), constant
/// per chain (`chain_id`), or backed by contract storage (`TX_NONCES`, which
/// rolls back atomically with the rest of state on reorg). The same logical
/// transaction therefore reproduces the same id across any replay, so an
/// inbound ack always finds its `PENDING_*` entry.
///
/// The nonce is keyed per-sender (`Map<String, u128>` indexed by
/// `sender.to_sender_string()`), which makes determinism robust against
/// cross-sender reordering during a reorg replay: a single sender's txs
/// cannot reorder within a chain because Cosmos SDK enforces strict
/// per-account sequence ordering at the mempool level, so each sender's
/// nonce stream is invariant under any realistic replay.
pub fn generate_tx(
    deps: &mut DepsMut,
    env: &Env,
    sender: &CrossChainUser,
) -> Result<String, ContractError> {
    let sender_key = sender.to_sender_string();
    let chain_id = env.block.chain_id.clone();
    let mut nonce = TX_NONCES
        .may_load(deps.storage, sender_key.clone())?
        .unwrap_or_default();
    nonce = nonce.wrapping_add(1);
    TX_NONCES.save(deps.storage, sender_key.clone(), &nonce)?;
    Ok(format!("{sender_key}:{chain_id}:{nonce}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::ChainUid;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use rstest::rstest;

    /// `(sender_chain_uid, sender_address, env.block.chain_id, env.block.height, env.transaction.index)`.
    type IdInputs = (&'static str, &'static str, &'static str, u64, u32);

    /// Generates a single tx_id on a fresh `MockDeps` (so nonce always starts at 1).
    /// Reorg-safety properties are exercised across the format inputs, not across
    /// storage state — see `nonce_disambiguates_back_to_back_calls` for the
    /// storage-bound case.
    fn call_once(inputs: IdInputs) -> String {
        let (chain_uid, address, chain_id, height, tx_index) = inputs;
        let mut deps = mock_dependencies();
        let sender = CrossChainUser::new(
            ChainUid::create(chain_uid.to_string()).unwrap(),
            address.to_string(),
        );
        let mut env = mock_env();
        env.block.chain_id = chain_id.to_string();
        env.block.height = height;
        if let Some(ref mut tx) = env.transaction {
            tx.index = tx_index;
        }
        generate_tx(&mut deps.as_mut(), &env, &sender).unwrap()
    }

    /// Reorg-safety contract for `generate_tx`: identical
    /// `(sender, chain_id, per-sender nonce)` state must produce identical ids
    /// regardless of `block.height` / `transaction.index`, and any change to
    /// `sender` or `chain_id` must change the id.
    #[rstest]
    #[case::height_and_tx_index_do_not_affect_id(
        ("chaina", "alice", "cosmos-test", 100, 0),
        ("chaina", "alice", "cosmos-test", 999_999, 42),
        true,
    )]
    #[case::different_senders_disambiguate(
        ("chaina", "alice", "cosmos-test", 0, 0),
        ("chaina", "bob", "cosmos-test", 0, 0),
        false,
    )]
    #[case::different_chain_ids_disambiguate(
        ("chaina", "alice", "chain-1", 0, 0),
        ("chaina", "alice", "chain-2", 0, 0),
        false,
    )]
    fn id_equality_under_input_variation(
        #[case] a: IdInputs,
        #[case] b: IdInputs,
        #[case] expect_eq: bool,
    ) {
        let id_a = call_once(a);
        let id_b = call_once(b);
        if expect_eq {
            assert_eq!(id_a, id_b, "expected ids to match");
        } else {
            assert_ne!(id_a, id_b, "expected ids to differ");
        }
    }

    /// Regression guard: two calls within the same `Env` must produce distinct
    /// ids, proving the nonce is doing disambiguation work and protecting
    /// against a future change that silently drops it.
    #[test]
    fn nonce_disambiguates_back_to_back_calls() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let sender = CrossChainUser::new(
            ChainUid::create("chaina".to_string()).unwrap(),
            "alice".to_string(),
        );

        let id_1 = generate_tx(&mut deps.as_mut(), &env, &sender).unwrap();
        let id_2 = generate_tx(&mut deps.as_mut(), &env, &sender).unwrap();

        assert_ne!(id_1, id_2);
    }

    /// Reorg-safety guard: the format must not regress to include block-derived
    /// components. Sender expands to `chain_uid:address`, so the full expansion
    /// is 4 colon-separated segments; the top-level `format!` template is
    /// `{sender}:{chain_id}:{nonce}`, i.e. 3 fields.
    #[test]
    fn output_format_has_three_top_level_segments() {
        let id = call_once(("chaina", "alice", "cosmos-test", 0, 0));
        let segments: Vec<&str> = id.split(':').collect();
        assert_eq!(segments.len(), 4, "id={id}");
    }

    /// Per-sender nonce isolation: two senders' nonce streams are independent.
    /// On the same `Env` and same `MockDeps`, Alice's first two calls produce
    /// nonces 1 and 2 regardless of how many calls Bob has made interleaved.
    /// This is the property that closes the cross-sender reorg-reordering
    /// hole: replaying a block with senders reordered does not perturb any
    /// individual sender's nonce stream.
    #[test]
    fn per_sender_nonce_is_independent_across_senders() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let alice = CrossChainUser::new(
            ChainUid::create("chaina".to_string()).unwrap(),
            "alice".to_string(),
        );
        let bob = CrossChainUser::new(
            ChainUid::create("chaina".to_string()).unwrap(),
            "bob".to_string(),
        );

        let alice_1 = generate_tx(&mut deps.as_mut(), &env, &alice).unwrap();
        let bob_1 = generate_tx(&mut deps.as_mut(), &env, &bob).unwrap();
        let alice_2 = generate_tx(&mut deps.as_mut(), &env, &alice).unwrap();
        let bob_2 = generate_tx(&mut deps.as_mut(), &env, &bob).unwrap();

        // Both senders' first calls land at nonce 1, second calls at nonce 2.
        assert!(alice_1.ends_with(":1"), "alice_1={alice_1}");
        assert!(bob_1.ends_with(":1"), "bob_1={bob_1}");
        assert!(alice_2.ends_with(":2"), "alice_2={alice_2}");
        assert!(bob_2.ends_with(":2"), "bob_2={bob_2}");
    }

    /// Reordering Alice's and Bob's calls in a replayed block does not shift
    /// either sender's nonce stream. This is the determinism property the
    /// per-sender map was introduced to preserve — invariant under any
    /// permutation of cross-sender interleaving.
    #[test]
    fn per_sender_nonce_stable_under_cross_sender_reordering() {
        let env = mock_env();
        let alice = CrossChainUser::new(
            ChainUid::create("chaina".to_string()).unwrap(),
            "alice".to_string(),
        );
        let bob = CrossChainUser::new(
            ChainUid::create("chaina".to_string()).unwrap(),
            "bob".to_string(),
        );

        let (alice_first_original, bob_first_original) = {
            let mut deps = mock_dependencies();
            let a = generate_tx(&mut deps.as_mut(), &env, &alice).unwrap();
            let b = generate_tx(&mut deps.as_mut(), &env, &bob).unwrap();
            (a, b)
        };

        let (alice_first_replay, bob_first_replay) = {
            let mut deps = mock_dependencies();
            // Bob is replayed first this time; Alice's id must not shift.
            let b = generate_tx(&mut deps.as_mut(), &env, &bob).unwrap();
            let a = generate_tx(&mut deps.as_mut(), &env, &alice).unwrap();
            (a, b)
        };

        assert_eq!(alice_first_original, alice_first_replay);
        assert_eq!(bob_first_original, bob_first_replay);
    }
}
