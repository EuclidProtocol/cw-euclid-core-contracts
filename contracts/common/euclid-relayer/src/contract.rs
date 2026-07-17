#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use cw2::set_contract_version;
use euclid::{admin::EuclidAdmin, error::ContractError};
use relayer::msgs::{ExecuteMsg, InstantiateMsg, QueryMsg, State};

use crate::{
    execute::{
        execute_add_validator, execute_meta_transaction, execute_remove_validator,
        execute_update_admin, execute_update_state,
    },
    query::{get_admin, get_nonce_relayed, get_state, get_validators},
    state::{ADMIN, STATE},
};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:euclid-relayer";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let admin = EuclidAdmin::default(info.sender);
    let state = State {
        message_signer: msg.message_signer,
        signature_threshold: msg.signature_threshold,
    };
    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &admin)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    euclid::build_info::set_build_info(deps.storage)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute(
            "message_signer_pubkey",
            state.message_signer.pubkey.to_string(),
        )
        .add_attribute(
            "message_signer_address",
            state.message_signer.address.to_string(),
        )
        .add_attribute("signature_threshold", msg.signature_threshold.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ExecuteMetaTransaction(msg) => {
            execute_meta_transaction(&mut deps, &env, &info, msg)
        }
        ExecuteMsg::UpdateState(msg) => execute_update_state(&mut deps, &info, msg),
        ExecuteMsg::UpdateAdmin(msg) => execute_update_admin(&mut deps, env, &info, msg),
        ExecuteMsg::AddValidator {
            validator,
            chain_uid,
        } => execute_add_validator(&mut deps, &info, validator, chain_uid),
        ExecuteMsg::RemoveValidator {
            validator,
            chain_uid,
        } => execute_remove_validator(&mut deps, &info, validator, chain_uid),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => Ok(to_json_binary(&get_state(&deps)?)?),
        QueryMsg::GetAdmin {} => Ok(to_json_binary(&get_admin(&deps)?)?),
        QueryMsg::NonceRelayed { nonce } => Ok(to_json_binary(&get_nonce_relayed(&deps, nonce)?)?),
        QueryMsg::Validators {} => Ok(to_json_binary(&get_validators(&deps)?)?),
        QueryMsg::GetBuildInfo {} => Ok(to_json_binary(&euclid::build_info::build_info(
            deps.storage,
            CONTRACT_VERSION,
        ))?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ADMIN, NONCES, STATE, VALIDATORS};
    use crate::testing::helpers::{get_signer_key, init, test_chain_uid};
    use cosmwasm_std::{
        attr,
        testing::{message_info, mock_dependencies, mock_env},
    };
    use euclid::admin::EuclidAdmin;
    use relayer::msgs::{InstantiateMsg, Validator};

    // -----------------------------------------------------------------------
    // Instantiate
    // -----------------------------------------------------------------------

    #[test]
    fn test_instantiate_stores_state_and_admin() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);

        assert_eq!(res.attributes[0], attr("method", "instantiate"));

        let (_, pub_key) = get_signer_key();
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.message_signer.pubkey, pub_key);
        assert_eq!(state.signature_threshold, 1);

        let sender = deps.api.addr_make("sender");
        let admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(admin, EuclidAdmin::default(sender));
    }

    #[test]
    fn test_instantiate_emits_expected_attributes() {
        let mut deps = mock_dependencies();
        let (_, pub_key) = get_signer_key();
        let msg = InstantiateMsg {
            message_signer: Validator {
                pubkey: pub_key.clone(),
                address: "signer_address".to_string(),
            },
            signature_threshold: 3,
        };
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let keys: Vec<&str> = res.attributes.iter().map(|a| a.key.as_str()).collect();
        assert!(keys.contains(&"method"));
        assert!(keys.contains(&"message_signer_pubkey"));
        assert!(keys.contains(&"message_signer_address"));
        assert!(keys.contains(&"signature_threshold"));

        let threshold_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "signature_threshold")
            .unwrap();
        assert_eq!(threshold_attr.value, "3");
    }

    #[test]
    fn test_instantiate_no_validators_registered() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let chain_uid = test_chain_uid();
        let validators = VALIDATORS
            .load(&deps.storage, chain_uid)
            .unwrap_or_default();
        assert!(validators.is_empty());
    }

    #[test]
    fn test_instantiate_no_nonces() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let relayed = NONCES.has(&deps.storage, "any_nonce".to_string());
        assert!(!relayed);
    }
}
