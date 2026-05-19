use cosmwasm_std::{DepsMut, Env};
use cw_storage_plus::Item;

use crate::{cross_chain_user::CrossChainUser, error::ContractError};

const TX_NONCE: Item<u128> = Item::new("tx_nonce");

/// Generates a deterministic identifier for a cross-chain transaction.
///
/// Format: `{sender}:{chain_id}:{nonce}`
///
/// # Reorg safety
///
/// The id is intentionally free of block-derived components (`block.height`,
/// `transaction.index`). All inputs are either external (`sender`), constant
/// per chain (`chain_id`), or backed by contract storage (`TX_NONCE`, which
/// rolls back atomically with the rest of state on reorg). The same logical
/// transaction therefore reproduces the same id across any replay, so an
/// inbound ack always finds its `PENDING_*` entry.
///
/// # Known limitation: global nonce under cross-sender reordering
///
/// `TX_NONCE` is a single global counter per contract, not per-sender. If a
/// reorg replays a block containing multiple senders' transactions in a
/// different order (e.g. Alice then Bob originally, replayed as Bob then
/// Alice), each sender's nonce shifts and the determinism property breaks
/// for that case. The mitigation is per-sender nonce
/// (`Map<String, u128>` keyed by `sender.to_sender_string()`), which is
/// reorg-deterministic because Cosmos SDK enforces strict per-account
/// sequence ordering at the mempool level. Deferred follow-up; see
/// `CHANGELOG.md` under `[euclid]`.
pub fn generate_tx(
    deps: &mut DepsMut,
    env: &Env,
    sender: &CrossChainUser,
) -> Result<String, ContractError> {
    let sender = sender.to_sender_string();
    let chain_id = env.block.chain_id.clone();
    let mut nonce = TX_NONCE.may_load(deps.storage)?.unwrap_or_default();
    nonce = nonce.wrapping_add(1);
    TX_NONCE.save(deps.storage, &nonce)?;
    Ok(format!("{sender}:{chain_id}:{nonce}"))
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
    /// `(sender, chain_id, TX_NONCE)` state must produce identical ids
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
}
