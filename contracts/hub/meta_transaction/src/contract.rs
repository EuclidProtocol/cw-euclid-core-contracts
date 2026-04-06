#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use cw2::set_contract_version;
use euclid::admin::EuclidAdmin;
use euclid::error::ContractError;
use euclid::msgs::meta_transaction::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, State};

use crate::execute::execute_meta_transaction;
use crate::query::get_nonce;
use crate::{
    execute::execute_update_admin,
    query::get_state,
    state::{ADMIN, STATE},
};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:meta-transaction";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        router_contract: msg.router_contract.clone(),
    };
    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &EuclidAdmin::default(info.sender))?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", msg.router_contract))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateAdmin(msg) => execute_update_admin(&mut deps, env, &info, msg),
        ExecuteMsg::ExecuteMetaTransaction(msg) => {
            execute_meta_transaction(&mut deps, &env, &info, msg)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => Ok(to_json_binary(&get_state(&deps)?)?),
        QueryMsg::NonceRelayed { nonce } => Ok(to_json_binary(&get_nonce(&deps, nonce)?)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ADMIN, STATE};
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
    use cosmwasm_std::{attr, to_json_binary, Addr, ContractResult, SystemResult};
    use euclid::admin::{AdminType, EuclidAdmin};
    use euclid::chain::{Chain, ChainType, ChainUid, CosmosChain};
    use euclid::msgs::meta_transaction::msg::{
        ExecuteMsg, InstantiateMsg, MetaTransaction, MetaTransactionData, UpdateAdminMsg,
    };
    use euclid::msgs::router::ChainResponse;

    type MockDeps = cosmwasm_std::OwnedDeps<
        cosmwasm_std::MemoryStorage,
        cosmwasm_std::testing::MockApi,
        MockQuerier,
    >;

    fn router_contract() -> Addr {
        Addr::unchecked("router")
    }

    fn make_instantiate_msg() -> InstantiateMsg {
        InstantiateMsg {
            router_contract: router_contract(),
        }
    }

    fn initialized() -> MockDeps {
        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        instantiate(deps.as_mut(), mock_env(), info, make_instantiate_msg()).unwrap();
        deps
    }

    fn set_cosmos_chain_query(deps: &mut MockDeps, chain_uid_str: &str) {
        let chain_uid = ChainUid::create(chain_uid_str.to_string()).unwrap();
        let chain = Chain {
            chain_uid: chain_uid.clone(),
            factory_address: "factory".to_string(),
            chain_type: ChainType::Cosmos(CosmosChain {
                chain_id: "testchain-1".to_string(),
            }),
        };
        let response = ChainResponse { chain, chain_uid };
        let bin = to_json_binary(&response).unwrap();
        deps.querier
            .update_wasm(move |_| SystemResult::Ok(ContractResult::Ok(bin.clone())));
    }

    #[test]
    fn test_instantiate_stores_state_and_admin() {
        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("creator");
        let info = message_info(&sender, &[]);

        let res = instantiate(deps.as_mut(), mock_env(), info, make_instantiate_msg()).unwrap();

        assert_eq!(res.attributes[0], attr("method", "instantiate"));
        assert_eq!(
            res.attributes[1],
            attr("router_contract", router_contract().as_str())
        );

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.router_contract, router_contract());

        let admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(admin, EuclidAdmin::default(sender));
    }

    #[test]
    fn test_instantiate_sets_cw2_contract_version() {
        use cw2::get_contract_version;

        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("creator");
        let info = message_info(&sender, &[]);

        instantiate(deps.as_mut(), mock_env(), info, make_instantiate_msg()).unwrap();

        let version = get_contract_version(&deps.storage).unwrap();
        assert_eq!(version.contract, CONTRACT_NAME);
        assert_eq!(version.version, CONTRACT_VERSION);
    }

    // -----------------------------------------------------------------------
    // execute() dispatch: UpdateAdmin variant reaches execute_update_admin
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_dispatch_update_admin_routes_correctly() {
        let mut deps = initialized();
        let sender = deps.api.addr_make("sender");
        let new_admin = deps.api.addr_make("new_admin");
        let info = message_info(&sender, &[]);

        let res = execute(
            deps.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::UpdateAdmin(UpdateAdminMsg {
                new_admin: new_admin.to_string(),
                admin_type: AdminType::GeneralAdmin,
            }),
        )
        .unwrap();

        // The execute_update_admin handler adds a "method" = "update_admin" attribute.
        assert!(
            res.attributes.iter().any(|a| a.key == "method"),
            "dispatch should reach update_admin handler"
        );
        let stored = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(stored.general_admin, new_admin);
    }

    // -----------------------------------------------------------------------
    // execute() dispatch: ExecuteMetaTransaction variant is routed; an expired
    // transaction surfaces the ContractError from execute_meta_transaction rather
    // than a routing error, proving dispatch reached the correct handler.
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_dispatch_meta_transaction_routes_correctly() {
        let mut deps = initialized();
        set_cosmos_chain_query(&mut deps, "testchain");

        let broadcaster = deps.api.addr_make("broadcaster");
        let info = message_info(&broadcaster, &[]);
        let mut env = mock_env();
        // Use a block time well past a known expiry to force the timestamp guard.
        env.block.time = cosmwasm_std::Timestamp::from_seconds(9_999_999);

        let data = MetaTransactionData {
            signer_address: "euclid1someaddress".to_string(),
            signer_prefix: "euclid".to_string(),
            signer_chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            call_data: vec![],
            expiry: env.block.time.seconds() - 1, // already expired
            nonce: "dispatch_test_nonce".to_string(),
        };
        let meta_tx = MetaTransaction {
            data,
            signature: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            signer_pubkey: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".to_string(),
        };

        let res = execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::ExecuteMetaTransaction(meta_tx),
        );
        // Routing succeeded; the error is from execute_meta_transaction itself.
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            euclid::error::ContractError::new("Timestamp limit exceeded"),
            "dispatch should reach execute_meta_transaction handler"
        );
    }
}
