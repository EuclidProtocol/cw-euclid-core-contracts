#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env, MockQuerier},
        DepsMut, Response,
    };
    use euclid::{
        chain::ChainUid,
        error::ContractError,
        fee::DenomFees,
        msgs::factory::{ExecuteMsg, InstantiateMsg},
    };

    use crate::{
        contract::{execute, instantiate},
        state::{State, HUB_CHANNEL, STATE},
    };

    fn _initialize_state(deps: &mut DepsMut) {
        let state = State {
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            router_contract: "router_contract".to_string(),
            admin: "admin".to_string(),
            escrow_code_id: 1,
            cw20_code_id: 2,
            is_native: true,
            partner_fees_collected: DenomFees {
                totals: HashMap::default(),
            },
        };
        STATE.save(deps.storage, &state).unwrap();
    }

    fn init(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            MockQuerier,
        >,
    ) -> Response {
        let msg = InstantiateMsg {
            router_contract: "router".to_string(),
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            escrow_code_id: 1,
            cw20_code_id: 2,
            is_native: true,
            mock_relayer_address: None,
        };
        let owner = deps.api.addr_make("owner");
        let info = message_info(&owner, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);
        assert_eq!(0, res.messages.len());
        let owner = deps.api.addr_make("owner");
        let expected_state = State {
            router_contract: "router".to_string(),
            admin: owner.to_string(),
            escrow_code_id: 1,
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            cw20_code_id: 2,
            is_native: true,
            partner_fees_collected: DenomFees {
                totals: HashMap::default(),
            },
        };
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
    }
    #[test]
    fn test_update_hub_channel() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let not_owner = deps.api.addr_make("not_owner");
        let info = message_info(&not_owner, &[]);
        init(&mut deps);

        HUB_CHANNEL
            .save(deps.as_mut().storage, &"1".to_string())
            .unwrap();
        let msg = ExecuteMsg::UpdateHubChannel {
            new_channel: "2".to_string(),
        };
        // Unauthorized
        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let owner = deps.api.addr_make("owner");
        let info = message_info(&owner, &[]);
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        assert_eq!(HUB_CHANNEL.load(&deps.storage).unwrap(), "2".to_string());
    }
}
