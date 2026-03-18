#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{from_json, Addr};

    use crate::contract::{execute, instantiate, query};
    use euclid::msgs::position_token::{
        ExecuteMsg, InstantiateMsg, OwnerOfResponse, QueryMsg, StateResponse, TokensResponse,
    };

    fn setup() -> (
        cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        Addr,
        Addr,
        Addr,
    ) {
        let mut deps = mock_dependencies();

        let minter = deps.api.addr_make("minter");
        let admin = deps.api.addr_make("admin");
        let creator = deps.api.addr_make("creator");

        instantiate(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            InstantiateMsg {
                name: "Position Token".to_string(),
                symbol: "POS".to_string(),
                minter: minter.clone(),
                admin: admin.clone(),
            },
        )
        .unwrap();

        (deps, minter, admin, creator)
    }

    #[test]
    fn instantiate_and_query_state() {
        let (deps, minter, admin, _) = setup();

        let state: StateResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap()).unwrap();

        assert_eq!(state.name, "Position Token");
        assert_eq!(state.symbol, "POS");
        assert_eq!(state.minter, minter);
        assert_eq!(state.admin, admin);
        assert_eq!(state.total_tokens, 0);
    }

    #[test]
    fn mint_transfer_burn_flow() {
        let (mut deps, minter, _, _) = setup();

        let owner = deps.api.addr_make("owner");
        let recipient = deps.api.addr_make("recipient");

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&minter, &[]),
            ExecuteMsg::Mint {
                token_id: "position-1".to_string(),
                owner: owner.to_string(),
                token_uri: Some("ipfs://position-1".to_string()),
            },
        )
        .unwrap();

        let owner_of: OwnerOfResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::OwnerOf {
                    token_id: "position-1".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(owner_of.owner, owner.to_string());

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&owner, &[]),
            ExecuteMsg::Transfer {
                token_id: "position-1".to_string(),
                recipient: recipient.to_string(),
            },
        )
        .unwrap();

        let recipient_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: recipient.to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(recipient_tokens.tokens, vec!["position-1".to_string()]);

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&recipient, &[]),
            ExecuteMsg::Burn {
                token_id: "position-1".to_string(),
            },
        )
        .unwrap();

        let all_tokens: TokensResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::AllTokens {}).unwrap()).unwrap();
        assert!(all_tokens.tokens.is_empty());

        let state: StateResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap()).unwrap();
        assert_eq!(state.total_tokens, 0);
    }

    #[test]
    fn multi_token_transfer_and_burn() {
        let (mut deps, minter, _, _) = setup();

        let owner = deps.api.addr_make("owner");
        let recipient = deps.api.addr_make("recipient");

        // Mint 3 tokens to the same owner
        for id in ["pos-1", "pos-2", "pos-3"] {
            execute(
                deps.as_mut(),
                mock_env(),
                message_info(&minter, &[]),
                ExecuteMsg::Mint {
                    token_id: id.to_string(),
                    owner: owner.to_string(),
                    token_uri: None,
                },
            )
            .expect("mint should succeed");
        }

        // Verify owner has all 3
        let owner_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: owner.to_string(),
                },
            )
            .expect("query should succeed"),
        )
        .expect("deserialize should succeed");
        assert_eq!(owner_tokens.tokens, vec!["pos-1", "pos-2", "pos-3"]);

        // Transfer pos-2 to recipient
        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&owner, &[]),
            ExecuteMsg::Transfer {
                token_id: "pos-2".to_string(),
                recipient: recipient.to_string(),
            },
        )
        .expect("transfer should succeed");

        // Burn pos-1
        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&owner, &[]),
            ExecuteMsg::Burn {
                token_id: "pos-1".to_string(),
            },
        )
        .expect("burn should succeed");

        // Owner should only have pos-3
        let owner_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: owner.to_string(),
                },
            )
            .expect("query should succeed"),
        )
        .expect("deserialize should succeed");
        assert_eq!(owner_tokens.tokens, vec!["pos-3"]);

        // Recipient should have pos-2
        let recipient_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: recipient.to_string(),
                },
            )
            .expect("query should succeed"),
        )
        .expect("deserialize should succeed");
        assert_eq!(recipient_tokens.tokens, vec!["pos-2"]);

        // AllTokens should have pos-2 and pos-3
        let all_tokens: TokensResponse = from_json(
            query(deps.as_ref(), mock_env(), QueryMsg::AllTokens {}).expect("query should succeed"),
        )
        .expect("deserialize should succeed");
        assert_eq!(all_tokens.tokens, vec!["pos-2", "pos-3"]);

        // Total should be 2
        let state: StateResponse = from_json(
            query(deps.as_ref(), mock_env(), QueryMsg::State {}).expect("query should succeed"),
        )
        .expect("deserialize should succeed");
        assert_eq!(state.total_tokens, 2);
    }
}
