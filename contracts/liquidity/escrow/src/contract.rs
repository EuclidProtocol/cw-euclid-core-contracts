#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, Uint128,
};

use cw2::set_contract_version;
use euclid::error::ContractError;

// use cw2::set_contract_version;

use crate::execute::{
    self, execute_add_allowed_denom, execute_deposit_native, execute_disallow_denom,
    execute_withdraw, receive_cw20,
};
use crate::query::{self, query_denom_balance, query_token_id};
use crate::state::{State, STATE};

use euclid::msgs::escrow::{EscrowInstantiateResponse, ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:escrow";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        token_id: msg.token_id.clone(),
        // Set the sender as the factory address, since we want the factory to instantiate the escrow.
        factory_address: info.sender.clone(),
        total_amount: Uint128::zero(),
    };
    STATE.save(deps.storage, &state)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let data = EscrowInstantiateResponse {
        token: msg.token_id.clone(),
        address: env.contract.address.to_string(),
    };
    let mut res = Response::new();

    if let Some(denom) = msg.allowed_denom {
        res = execute::execute_add_allowed_denom(deps, env, info.clone(), denom)?;
    }

    Ok(res
        .add_attribute("method", "instantiate")
        .add_attribute("token_id", msg.token_id.as_str())
        .add_attribute("factory_address", info.sender)
        .set_data(to_json_binary(&data)?))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::DepositNative {} => execute_deposit_native(deps, env, info),
        ExecuteMsg::AddAllowedDenom { denom } => execute_add_allowed_denom(deps, env, info, denom),
        ExecuteMsg::DisallowDenom { denom } => execute_disallow_denom(deps, env, info, denom),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Withdraw {
            recipient,
            amount,
            denom,
            forwarding_message,
        } => execute_withdraw(
            deps,
            env,
            info,
            recipient,
            amount,
            denom,
            forwarding_message,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query::query_state(deps),
        QueryMsg::TokenId {} => query_token_id(deps),
        QueryMsg::TokenAllowed { denom } => query::query_token_allowed(deps, denom),
        QueryMsg::AllowedDenoms {} => query::query_allowed_denoms(deps),
        QueryMsg::GetDenomBalance { denom } => query_denom_balance(deps, denom),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let id = msg.id;
    Err(ContractError::Std(StdError::generic_err(format!(
        "Unknown reply id: {}",
        id
    ))))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        attr,
        testing::{message_info, mock_dependencies, mock_env},
        Addr, Uint128,
    };
    use euclid::{
        error::ContractError,
        msgs::escrow::{ExecuteMsg, InstantiateMsg},
        token::TokenType,
    };

    use crate::{
        state::{ALLOWED_DENOMS, DENOM_TO_AMOUNT, STATE},
        testing::helpers::{init, init_no_denom, native_denom, token, NATIVE_DENOM, TOKEN_ID},
    };

    use super::{execute, instantiate};

    // -----------------------------------------------------------------------
    // Instantiate
    // -----------------------------------------------------------------------

    #[test]
    fn test_instantiate_sets_state() {
        let mut deps = mock_dependencies();
        let factory = deps.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let msg = InstantiateMsg {
            token_id: token(),
            allowed_denom: None,
        };
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.attributes[0], attr("method", "instantiate"));
        assert_eq!(res.attributes[1], attr("token_id", TOKEN_ID));
        assert_eq!(
            res.attributes[2],
            attr("factory_address", factory.to_string())
        );

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.token_id, token());
        assert_eq!(state.factory_address, factory);
        assert_eq!(state.total_amount, Uint128::zero());
    }

    #[test]
    fn test_instantiate_with_allowed_denom_adds_to_list() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let allowed = ALLOWED_DENOMS.load(&deps.storage).unwrap();
        assert_eq!(allowed.len(), 1);
        assert!(allowed.contains(&native_denom()));

        let bal = DENOM_TO_AMOUNT
            .load(&deps.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::zero());
    }

    #[test]
    fn test_instantiate_without_allowed_denom_leaves_list_empty() {
        let mut deps = mock_dependencies();
        init_no_denom(&mut deps);
        let allowed = ALLOWED_DENOMS
            .may_load(&deps.storage)
            .unwrap()
            .unwrap_or_default();
        assert!(allowed.is_empty());
    }

    #[test]
    fn test_instantiate_response_data_contains_token_and_address() {
        let mut deps = mock_dependencies();
        let factory = deps.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let msg = InstantiateMsg {
            token_id: token(),
            allowed_denom: None,
        };
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert!(res.data.is_some());
    }

    #[test]
    fn test_instantiate_voucher_denom_rejected() {
        let mut deps = mock_dependencies();
        let factory = deps.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let msg = InstantiateMsg {
            token_id: token(),
            allowed_denom: Some(TokenType::Voucher {}),
        };
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::CannotEscrowVoucher {});
    }

    // -----------------------------------------------------------------------
    // State invariants / composite sequences
    // -----------------------------------------------------------------------

    #[test]
    fn test_total_amount_tracks_multiple_deposits_and_a_partial_withdraw() {
        use cosmwasm_std::coin;
        let mut deps = mock_dependencies();
        init(&mut deps);

        let factory = deps.api.addr_make("factory");

        for _ in 0..2 {
            let info = message_info(&factory, &[coin(300, NATIVE_DENOM)]);
            execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::DepositNative {},
            )
            .unwrap();
        }

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(600));

        let info = message_info(&factory, &[]);
        execute(
            deps.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::new(150),
                denom: native_denom(),
                forwarding_message: None,
            },
        )
        .unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(450));

        let bal = DENOM_TO_AMOUNT
            .load(&deps.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::new(450));
    }

    #[test]
    fn test_full_lifecycle_add_deposit_disallow_withdraw() {
        use cosmwasm_std::coin;
        let mut deps = mock_dependencies();
        init(&mut deps);
        let factory = deps.api.addr_make("factory");

        let info = message_info(&factory, &[coin(500, NATIVE_DENOM)]);
        execute(
            deps.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap();

        let finfo = message_info(&factory, &[]);
        execute(
            deps.as_mut(),
            mock_env(),
            finfo.clone(),
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let info = message_info(&factory, &[coin(100, NATIVE_DENOM)]);
        let err = execute(
            deps.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::UnsupportedDenomination {});

        execute(
            deps.as_mut(),
            mock_env(),
            finfo.clone(),
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::new(500),
                denom: native_denom(),
                forwarding_message: None,
            },
        )
        .unwrap();

        let bal = DENOM_TO_AMOUNT
            .load(&deps.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::zero());

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::zero());
    }
}
