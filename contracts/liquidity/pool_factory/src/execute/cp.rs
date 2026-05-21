use cosmwasm_std::{
    ensure, to_json_binary, Addr, CosmosMsg, DepsMut, Env, MessageInfo, Response, SubMsg, WasmMsg,
};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::{cross_chain_config::CrossChainConfig, factory::ExecuteMsg as FactoryExecuteMsg},
    token::PairWithDenomAndAmount,
};

use crate::{
    outbound,
    state::{PoolCreateRequest, MAIN_FACTORY_ADDRESS, PAIR_TO_VLP, PENDING_POOL_REQUESTS},
};

/// Handles a CP/Stable `RequestPoolCreation` delegated from main factory.
///
/// Caller MUST be the configured main factory. Pool factory:
///   1. validates the request and records it in `PENDING_POOL_REQUESTS`,
///   2. builds the outbound `RouterCrossChainExecuteMsg::RequestPoolCreation`
///      via `outbound::request_pool_creation`,
///   3. hands that binary to `main_factory::ProxySendPacket` for dispatch.
#[allow(clippy::too_many_arguments)]
pub fn on_request_pool_creation(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    tx_id: String,
    sender: Addr,
    pair_with_denom_and_amount: PairWithDenomAndAmount,
    pool_config: euclid::msgs::vlp::base::PoolConfig,
    _lp_token_name: String,
    _lp_token_symbol: String,
    _lp_token_decimal: u8,
    _lp_token_marketing: Option<cw20_base::msg::InstantiateMarketingInfo>,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let main_factory = MAIN_FACTORY_ADDRESS.load(deps.storage)?;
    ensure!(info.sender == main_factory, ContractError::Unauthorized {});

    let pair = pair_with_denom_and_amount.get_pair()?;
    pair.validate()?;

    ensure!(
        !PENDING_POOL_REQUESTS.has(deps.storage, (sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    ensure!(
        !PAIR_TO_VLP.has(deps.storage, pair.get_tupple()),
        ContractError::PoolAlreadyExists {}
    );

    // Build a CrossChainUser for the outbound packet using main factory's
    // chain_uid (queried via the existing state surface). Pool factory does
    // not maintain its own chain identity — main factory is the source of
    // truth.
    let main_state: euclid::msgs::factory::StateResponse = deps.querier.query_wasm_smart(
        main_factory.clone(),
        &euclid::msgs::factory::QueryMsg::GetState {},
    )?;
    let cross_chain_sender = CrossChainUser::new(main_state.chain_uid.clone(), sender.to_string());

    PENDING_POOL_REQUESTS.save(
        deps.storage,
        (sender.clone(), tx_id.clone()),
        &PoolCreateRequest {
            tx_id: tx_id.clone(),
            sender: sender.clone(),
            pair_info: pair_with_denom_and_amount.clone(),
            lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg {
                name: _lp_token_name,
                symbol: _lp_token_symbol,
                decimals: _lp_token_decimal,
                initial_balances: vec![],
                mint: None,
                marketing: _lp_token_marketing,
            },
        },
    )?;

    let packet = outbound::request_pool_creation(
        cross_chain_sender,
        tx_id.clone(),
        pair_with_denom_and_amount,
        pool_config,
        slippage_tolerance_bps,
    )?;

    let proxy_msg = FactoryExecuteMsg::ProxySendPacket {
        msg: packet,
        timeout: cross_chain_config.timeout,
        ack_response: cross_chain_config.ack_response,
        sender,
    };
    let proxy_wasm = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: main_factory.into_string(),
        msg: to_json_binary(&proxy_msg)?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_attribute("method", "on_request_pool_creation")
        .add_attribute("tx_id", tx_id)
        .add_submessage(SubMsg::new(proxy_wasm)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, ContractResult, SystemResult, Uint256, WasmQuery,
    };
    use euclid::{
        chain::ChainUid,
        msgs::factory::StateResponse,
        token::{Token, TokenType, TokenWithDenomAndAmount},
    };

    use crate::testing::helpers::init_with_main_factory;

    fn sample_pair() -> PairWithDenomAndAmount {
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                    decimals: None,
                },
                amount: Uint256::from(100u128),
            },
        }
    }

    fn install_main_factory_state_querier(deps: &mut crate::testing::helpers::MockDeps) {
        let chain_uid = ChainUid::create("testchain".to_string()).unwrap();
        deps.querier.update_wasm(move |q| match q {
            WasmQuery::Smart { .. } => {
                let resp = StateResponse {
                    chain_uid: chain_uid.clone(),
                    router_contract: "router".to_string(),
                    relayer_contract: cosmwasm_std::Addr::unchecked("relayer"),
                    admin: euclid::admin::EuclidAdmin::default(cosmwasm_std::Addr::unchecked(
                        "admin",
                    )),
                    escrow_code_id: 10,
                    lp_code_id: 11,
                    is_native: false,
                };
                SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
            }
            _ => panic!("unexpected query"),
        });
    }

    #[test]
    fn test_on_request_pool_creation_rejects_non_main_factory_caller() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);

        let stranger = deps.api.addr_make("stranger");
        let user = deps.api.addr_make("user");
        let info = message_info(&stranger, &[]);
        let res = on_request_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx1".to_string(),
            user,
            sample_pair(),
            euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            "LP".to_string(),
            "LP".to_string(),
            6,
            None,
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_on_request_pool_creation_happy_path_emits_proxy_send() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        let res = on_request_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_happy".to_string(),
            user.clone(),
            sample_pair(),
            euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            "LP".to_string(),
            "LP".to_string(),
            6,
            None,
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        let pending = PENDING_POOL_REQUESTS
            .load(deps.as_ref().storage, (user, "tx_happy".to_string()))
            .unwrap();
        assert_eq!(pending.tx_id, "tx_happy");
    }

    #[test]
    fn test_on_request_pool_creation_duplicate_tx_id_rejected() {
        let mut deps = mock_dependencies();
        let main_factory = deps.api.addr_make("main_factory");
        init_with_main_factory(&mut deps, &main_factory);
        install_main_factory_state_querier(&mut deps);

        let user = deps.api.addr_make("user");
        let info = message_info(&main_factory, &[]);
        on_request_pool_creation(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            "tx_dup".to_string(),
            user.clone(),
            sample_pair(),
            euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            "LP".to_string(),
            "LP".to_string(),
            6,
            None,
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        )
        .unwrap();
        let res = on_request_pool_creation(
            deps.as_mut(),
            mock_env(),
            info,
            "tx_dup".to_string(),
            user,
            sample_pair(),
            euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            "LP".to_string(),
            "LP".to_string(),
            6,
            None,
            50,
            euclid::msgs::cross_chain_config::CrossChainConfig::new(None, None, None),
        );
        assert_eq!(res.unwrap_err(), ContractError::TxAlreadyExist {});
    }
}
