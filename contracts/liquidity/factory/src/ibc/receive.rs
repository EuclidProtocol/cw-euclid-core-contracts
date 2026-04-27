use cosmwasm_std::Uint256;
#[cfg(not(feature = "library"))]
use cosmwasm_std::{ensure, to_json_binary, CosmosMsg, DepsMut, Env, Response, SubMsg, WasmMsg};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{tx_event, TxType},
    msgs::{
        escrow::ExecuteMsg as EscrowExecuteMsg, factory::RegisterFactoryResponse,
        router::RegisterFactoryChainType,
    },
    token::{Token, TokenType},
};
use euclid_ibc::{ack::AcknowledgementMsg, factory_ibc::FactoryCrossChainExecuteMsg};

use crate::{
    reply::RELEASE_ESCROW_REPLY_ID,
    state::{STATE, TOKEN_TO_ESCROW},
};

pub fn reusable_internal_call(
    deps: &mut DepsMut,
    env: Env,
    msg: FactoryCrossChainExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_uid,
            chain_type,
            tx_id,
        } => execute_register_router(deps.branch(), env, chain_uid, chain_type, tx_id),
        FactoryCrossChainExecuteMsg::ReleaseEscrow {
            sender,
            token,
            amount,
            denom,
            forwarding_message,
            tx_id,
            recipient,
        } => execute_release_escrow(
            deps.branch(),
            env,
            sender,
            token,
            amount,
            denom,
            forwarding_message,
            tx_id,
            recipient,
        ),
    }
}

fn execute_register_router(
    deps: DepsMut,
    env: Env,
    chain_uid: ChainUid,
    chain_type: RegisterFactoryChainType,
    tx_id: String,
) -> Result<Response, ContractError> {
    match chain_type {
        RegisterFactoryChainType::Cosmos(cosmos_info) => {
            ensure!(
                cosmos_info.factory_address == env.contract.address.to_string(),
                ContractError::new("Factory address mismatch")
            );
            ensure!(
                cosmos_info.factory_chain_id == env.block.chain_id,
                ContractError::new("Factory chain ID mismatch")
            );
        }
        RegisterFactoryChainType::Native(native_info) => {
            ensure!(
                native_info.factory_address == env.contract.address.to_string(),
                ContractError::new("Factory address mismatch")
            );
            ensure!(
                native_info.factory_chain_id == env.block.chain_id,
                ContractError::new("Factory chain ID mismatch")
            );
        }
        _ => {
            return Err(ContractError::new("Invalid chain type"));
        }
    }
    let chain_uid = chain_uid.validate()?.to_owned();
    let ack_msg = RegisterFactoryResponse {
        factory_address: env.contract.address.to_string(),
        chain_id: env.block.chain_id,
    };
    let state = STATE.load(deps.storage)?;

    ensure!(
        state.chain_uid == chain_uid,
        ContractError::new("Chain UID mismatch")
    );

    let ack = to_json_binary(&AcknowledgementMsg::Ok(ack_msg))?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &state.router_contract,
            TxType::RegisterFactory,
        ))
        .add_attribute("action", "register_factory")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "register_router")
        .add_attribute("router", state.router_contract)
        .set_data(ack))
}

fn execute_release_escrow(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: Token,
    amount: Uint256,
    denom: TokenType,
    forwarding_message: Option<String>,
    tx_id: String,
    recipient: String,
) -> Result<Response, ContractError> {
    // Get escrow address
    let escrow_address = TOKEN_TO_ESCROW
        .load(deps.storage, token.validate()?.to_owned())?
        .into_string();

    let response = Response::new();
    let recipient = deps.api.addr_validate(&recipient)?;

    let user_withdraw_msg = EscrowExecuteMsg::Withdraw {
        recipient: recipient.clone(),
        amount,
        denom,
        forwarding_message,
    };

    let user_withdraw_msg = SubMsg::reply_always(
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: escrow_address.clone(),
            msg: to_json_binary(&user_withdraw_msg)?,
            funds: vec![],
        }),
        RELEASE_ESCROW_REPLY_ID,
    );

    Ok(response
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::EscrowRelease,
        ))
        .add_submessage(user_withdraw_msg)
        .add_attribute("action", "escrow_release")
        .add_attribute("method", "release escrow_execute")
        .add_attribute("sender", sender.to_sender_string())
        .add_attribute("token", token.to_string())
        .add_attribute("amount", amount.to_string())
        .add_attribute("tx_id", tx_id)
        .add_attribute("to_address", recipient))
}

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::reusable_internal_call;
    use crate::testing::helpers::{
        assert_attribute, assert_tx_event_full, get_attribute, init, seed_escrow, TEST_CHAIN_UID,
        TEST_ESCROW, TEST_ROUTER,
    };
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env},
        CosmosMsg, ReplyOn, Uint128, WasmMsg,
    };
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        msgs::router::{
            RegisterFactoryChainCosmos, RegisterFactoryChainNative, RegisterFactoryChainType,
        },
        token::{Token, TokenType},
    };
    use euclid_ibc::{ack::AcknowledgementMsg, factory_ibc::FactoryCrossChainExecuteMsg};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// `mock_env()` in cosmwasm-std v2.x supplies:
    ///   `contract.address = MockApi::default().addr_make("cosmos2contract")`
    ///   `block.chain_id   = "cosmos-testnet-14002"`
    ///
    /// The resolved bech32 value of `addr_make("cosmos2contract")` is the constant below.
    const MOCK_CONTRACT_ADDRESS: &str =
        "cosmwasm1jpev2csrppg792t22rn8z8uew8h3sjcpglcd0qv9g8gj8ky922tscp8avs";
    const MOCK_CHAIN_ID: &str = "cosmos-testnet-14002";

    fn cosmos_register_msg(
        factory_address: &str,
        factory_chain_id: &str,
        chain_uid: &str,
    ) -> FactoryCrossChainExecuteMsg {
        FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_uid: ChainUid::create(chain_uid.to_string()).unwrap(),
            chain_type: RegisterFactoryChainType::Cosmos(RegisterFactoryChainCosmos {
                factory_address: factory_address.to_string(),
                factory_chain_id: factory_chain_id.to_string(),
            }),
            tx_id: "tx-cosmos-1".to_string(),
        }
    }

    fn native_register_msg(
        factory_address: &str,
        factory_chain_id: &str,
        chain_uid: &str,
    ) -> FactoryCrossChainExecuteMsg {
        FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_uid: ChainUid::create(chain_uid.to_string()).unwrap(),
            chain_type: RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                factory_address: factory_address.to_string(),
                factory_chain_id: factory_chain_id.to_string(),
            }),
            tx_id: "tx-native-1".to_string(),
        }
    }

    fn release_escrow_msg(
        token_id: &str,
        recipient: &str,
        amount: u128,
    ) -> FactoryCrossChainExecuteMsg {
        FactoryCrossChainExecuteMsg::ReleaseEscrow {
            sender: CrossChainUser {
                chain_uid: ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
                address: "senderaddr".to_string(),
            },
            token: Token::create(token_id.to_string()).unwrap(),
            recipient: recipient.to_string(),
            amount: Uint128::new(amount),
            denom: TokenType::Native {
                denom: "uusdc".to_string(),
            },
            forwarding_message: None,
            tx_id: "tx-release-1".to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // execute_register_router — happy paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_factory_cosmos_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let msg = cosmos_register_msg(MOCK_CONTRACT_ADDRESS, MOCK_CHAIN_ID, TEST_CHAIN_UID);
        let res = reusable_internal_call(&mut deps.as_mut(), env.clone(), msg).unwrap();

        // Response has expected attributes
        assert_attribute(&res, "action", "register_factory");
        assert_eq!(get_attribute(&res, "method"), "register_router");
        assert_eq!(get_attribute(&res, "tx_id"), "tx-cosmos-1");
        assert_tx_event_full(&res, "register_factory", "tx-cosmos-1", TEST_ROUTER);

        // Response data is an Ok acknowledgement with factory address and chain id
        let ack: AcknowledgementMsg<euclid::msgs::factory::RegisterFactoryResponse> =
            cosmwasm_std::from_json(res.data.unwrap()).unwrap();
        match ack {
            AcknowledgementMsg::Ok(inner) => {
                assert_eq!(inner.factory_address, MOCK_CONTRACT_ADDRESS);
                assert_eq!(inner.chain_id, MOCK_CHAIN_ID);
            }
            _ => panic!("expected Ok acknowledgement"),
        }
    }

    #[test]
    fn test_register_factory_native_happy_path() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let msg = native_register_msg(MOCK_CONTRACT_ADDRESS, MOCK_CHAIN_ID, TEST_CHAIN_UID);
        let res = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap();

        let method_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "method")
            .expect("missing method attribute");
        assert_eq!(method_attr.value, "register_router");

        let ack: AcknowledgementMsg<euclid::msgs::factory::RegisterFactoryResponse> =
            cosmwasm_std::from_json(res.data.unwrap()).unwrap();
        assert!(matches!(ack, AcknowledgementMsg::Ok(_)));
    }

    // -----------------------------------------------------------------------
    // execute_register_router — error paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_factory_cosmos_wrong_factory_address_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let msg = cosmos_register_msg("wrong_address", MOCK_CHAIN_ID, TEST_CHAIN_UID);
        let err = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap_err();

        assert_eq!(
            err,
            euclid::error::ContractError::new("Factory address mismatch")
        );
    }

    #[test]
    fn test_register_factory_cosmos_wrong_chain_id_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let msg = cosmos_register_msg(MOCK_CONTRACT_ADDRESS, "wrong-chain-id", TEST_CHAIN_UID);
        let err = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap_err();

        assert_eq!(
            err,
            euclid::error::ContractError::new("Factory chain ID mismatch")
        );
    }

    #[test]
    fn test_register_factory_native_wrong_factory_address_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let msg = native_register_msg("wrong_address", MOCK_CHAIN_ID, TEST_CHAIN_UID);
        let err = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap_err();

        assert_eq!(
            err,
            euclid::error::ContractError::new("Factory address mismatch")
        );
    }

    #[test]
    fn test_register_factory_native_wrong_chain_id_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let msg = native_register_msg(MOCK_CONTRACT_ADDRESS, "wrong-chain-id", TEST_CHAIN_UID);
        let err = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap_err();

        assert_eq!(
            err,
            euclid::error::ContractError::new("Factory chain ID mismatch")
        );
    }

    #[test]
    fn test_register_factory_evm_chain_type_returns_invalid_chain_type_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let msg = FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_uid: ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
            chain_type: RegisterFactoryChainType::Evm(
                euclid::msgs::router::RegisterFactoryChainEvm {
                    factory_address: MOCK_CONTRACT_ADDRESS.to_string(),
                    factory_chain_id: MOCK_CHAIN_ID.to_string(),
                },
            ),
            tx_id: "tx-evm-1".to_string(),
        };
        let err = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap_err();

        assert_eq!(err, euclid::error::ContractError::new("Invalid chain type"));
    }

    #[test]
    fn test_register_factory_chain_uid_mismatch_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        // "otherchain" does not match STATE.chain_uid which is TEST_CHAIN_UID ("testchain")
        let msg = cosmos_register_msg(MOCK_CONTRACT_ADDRESS, MOCK_CHAIN_ID, "otherchain");
        let err = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap_err();

        assert_eq!(err, euclid::error::ContractError::new("Chain UID mismatch"));
    }

    // -----------------------------------------------------------------------
    // execute_release_escrow — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_release_escrow_happy_path_emits_submsg_and_attributes() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        // Seed the escrow mapping so the handler can find it
        seed_escrow(&mut deps, "usdc", TEST_ESCROW);

        // `deps.api.addr_make("recipient")` produces a valid bech32 address
        let recipient = deps.api.addr_make("recipient");
        let msg = release_escrow_msg("usdc", recipient.as_str(), 1000);

        let res = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap();

        // One SubMsg with reply_always and correct reply ID
        assert_eq!(res.messages.len(), 1);
        let sub = &res.messages[0];
        assert_eq!(sub.reply_on, ReplyOn::Always);
        assert_eq!(sub.id, crate::reply::RELEASE_ESCROW_REPLY_ID);

        // The inner message targets the escrow contract
        if let CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, .. }) = &sub.msg {
            assert_eq!(contract_addr, TEST_ESCROW);
        } else {
            panic!("expected Wasm Execute message");
        }

        // Attributes — parse each field and bind to input values
        assert_attribute(&res, "action", "escrow_release");
        assert_eq!(get_attribute(&res, "method"), "release escrow_execute");
        assert_eq!(get_attribute(&res, "tx_id"), "tx-release-1");
        assert_eq!(get_attribute(&res, "token"), "usdc");
        assert_eq!(get_attribute(&res, "amount"), "1000");
        assert_eq!(get_attribute(&res, "to_address"), recipient.as_str());
        assert_tx_event_full(
            &res,
            "escrow_release",
            "tx-release-1",
            &format!("{}:{}", TEST_CHAIN_UID, "senderaddr"),
        );
    }

    // -----------------------------------------------------------------------
    // execute_release_escrow — error paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_release_escrow_token_not_in_escrow_map_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        // No escrow seeded — TOKEN_TO_ESCROW lookup should fail
        let recipient = deps.api.addr_make("recipient");
        let msg = release_escrow_msg("unknowntoken", recipient.as_str(), 500);

        let err = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap_err();

        // The error comes from cw-storage-plus not finding the key, which surfaces as a StdError
        // wrapped in ContractError; just verify it is indeed an error.
        let _ = err; // presence of error is the assertion
    }

    #[test]
    fn test_release_escrow_invalid_recipient_address_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        seed_escrow(&mut deps, "usdc", TEST_ESCROW);

        // "not-a-valid-bech32" will fail addr_validate on MockApi
        let msg = release_escrow_msg("usdc", "not-a-valid-bech32", 100);

        let err = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap_err();

        let _ = err;
    }

    #[test]
    fn test_release_escrow_sender_attribute_uses_chain_and_address() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        seed_escrow(&mut deps, "atom", TEST_ESCROW);

        let recipient = deps.api.addr_make("recipient");
        let msg = FactoryCrossChainExecuteMsg::ReleaseEscrow {
            sender: CrossChainUser {
                chain_uid: ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
                address: "myaddr".to_string(),
            },
            token: Token::create("atom".to_string()).unwrap(),
            recipient: recipient.to_string(),
            amount: Uint128::new(250),
            denom: TokenType::Native {
                denom: "uatom".to_string(),
            },
            forwarding_message: None,
            tx_id: "tx-sender-attr-1".to_string(),
        };

        let res = reusable_internal_call(&mut deps.as_mut(), env, msg).unwrap();

        let sender_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "sender")
            .expect("missing sender attribute");
        // CrossChainUser::to_sender_string() returns "chain:address"
        assert_eq!(
            sender_attr.value,
            format!("{}:{}", TEST_CHAIN_UID, "myaddr")
        );
    }
}
