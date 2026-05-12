use cosmwasm_std::{Deps, Order};
use euclid::{admin::EuclidAdmin, error::ContractError};
use relayer::{msgs::State, ValidatorsResponse, ValidatorsResponseItem};

use crate::state::{ADMIN, NONCES, STATE, VALIDATORS};

pub fn get_state(deps: &Deps) -> Result<State, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

pub fn get_admin(deps: &Deps) -> Result<EuclidAdmin, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    Ok(admin)
}

pub fn get_nonce_relayed(deps: &Deps, nonce: String) -> Result<bool, ContractError> {
    Ok(NONCES.has(deps.storage, nonce))
}

pub fn get_validators(deps: &Deps) -> Result<ValidatorsResponse, ContractError> {
    let mut validators = vec![];
    let iter = VALIDATORS.range(deps.storage, None, None, Order::Ascending);
    for v in iter {
        let (chain_uid, chain_validators) = v?;
        for validator in &chain_validators {
            validators.push(ValidatorsResponseItem {
                validator: validator.clone(),
                chain_uid: chain_uid.clone(),
            });
        }
    }
    Ok(ValidatorsResponse { validators })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{NONCES, VALIDATORS};
    use crate::testing::helpers::{get_signer_key, init, make_validator, test_chain_uid};
    use cosmwasm_std::testing::mock_dependencies;
    use euclid::chain::ChainUid;
    use relayer::msgs::Validator;

    // -----------------------------------------------------------------------
    // get_state
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_state_returns_initialized_state() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let (_, pub_key) = get_signer_key();
        let state = get_state(&deps.as_ref()).unwrap();

        assert_eq!(state.message_signer.pubkey, pub_key);
        assert_eq!(state.signature_threshold, 1);
    }

    // -----------------------------------------------------------------------
    // get_admin
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_admin_returns_sender_as_all_admin_roles() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let admin = get_admin(&deps.as_ref()).unwrap();

        assert_eq!(admin.general_admin, sender);
        assert_eq!(admin.fee_admin, sender);
        assert_eq!(admin.migration_admin, sender);
    }

    // -----------------------------------------------------------------------
    // get_nonce_relayed
    // -----------------------------------------------------------------------

    #[test]
    fn test_nonce_not_relayed() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let relayed = get_nonce_relayed(&deps.as_ref(), "fresh-nonce".to_string()).unwrap();
        assert!(!relayed);
    }

    #[test]
    fn test_nonce_relayed_after_storage_write() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        NONCES
            .save(
                deps.as_mut().storage,
                "used-nonce".to_string(),
                &cosmwasm_std::Uint256::from(100u64),
            )
            .unwrap();

        let relayed = get_nonce_relayed(&deps.as_ref(), "used-nonce".to_string()).unwrap();
        assert!(relayed);
    }

    // -----------------------------------------------------------------------
    // get_validators
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_validators_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let resp = get_validators(&deps.as_ref()).unwrap();
        assert!(resp.validators.is_empty());
    }

    #[test]
    fn test_get_validators_single_chain() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let chain_uid = test_chain_uid();
        let (v, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        VALIDATORS
            .save(deps.as_mut().storage, chain_uid.clone(), &vec![v.clone()])
            .unwrap();

        let resp = get_validators(&deps.as_ref()).unwrap();
        assert_eq!(resp.validators.len(), 1);
        assert_eq!(resp.validators[0].validator, v);
        assert_eq!(resp.validators[0].chain_uid, chain_uid);
    }

    #[test]
    fn test_get_validators_multiple_chains() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let chain_a = ChainUid::create("chaina".to_string()).unwrap();
        let chain_b = ChainUid::create("chainb".to_string()).unwrap();

        let (v1, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        // Use a different valid compressed pubkey for v2
        let v2 = Validator {
            pubkey: cosmwasm_std::Binary::from(vec![0x03u8; 33]),
            address: "validator_other".to_string(),
        };

        VALIDATORS
            .save(deps.as_mut().storage, chain_a.clone(), &vec![v1.clone()])
            .unwrap();
        VALIDATORS
            .save(deps.as_mut().storage, chain_b.clone(), &vec![v2.clone()])
            .unwrap();

        let resp = get_validators(&deps.as_ref()).unwrap();
        // Results are ascending by chain_uid key
        assert_eq!(resp.validators.len(), 2);
        let chain_uids: Vec<ChainUid> = resp
            .validators
            .iter()
            .map(|v| v.chain_uid.clone())
            .collect();
        assert!(chain_uids.contains(&chain_a));
        assert!(chain_uids.contains(&chain_b));
    }

    #[test]
    fn test_get_validators_two_validators_same_chain() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let chain_uid = test_chain_uid();
        let (v1, _) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        let v2 = Validator {
            pubkey: cosmwasm_std::Binary::from(vec![0x02u8; 33]),
            address: "validator_second".to_string(),
        };

        VALIDATORS
            .save(deps.as_mut().storage, chain_uid.clone(), &vec![v1, v2])
            .unwrap();

        let resp = get_validators(&deps.as_ref()).unwrap();
        assert_eq!(resp.validators.len(), 2);
        // All for same chain
        assert!(resp.validators.iter().all(|v| v.chain_uid == chain_uid));
    }
}
