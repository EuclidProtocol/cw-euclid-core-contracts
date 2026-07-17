#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{from_json, Uint128};
    use cw20::{Cw20Coin, TokenInfoResponse};
    use euclid::msgs::lp_token::msg::{InstantiateMsg, QueryMsg};
    use euclid::token::{Pair, Token};
    use euclid::voucher::LP_TOKEN_DECIMAL;

    use crate::contract::{instantiate, query};

    fn sample_instantiate_msg(
        deps: &cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
    ) -> InstantiateMsg {
        InstantiateMsg {
            name: "Euclid LP Token".to_string(),
            symbol: "euclidLP".to_string(),
            decimals: LP_TOKEN_DECIMAL,
            initial_balances: vec![Cw20Coin {
                address: deps.api.addr_make("holder").to_string(),
                amount: Uint128::new(1_000_000),
            }],
            mint: None,
            marketing: None,
            vlp: deps.api.addr_make("vlp").to_string(),
            factory: deps.api.addr_make("factory"),
            token_pair: Pair::new(
                Token::create("tokena".to_string()).unwrap(),
                Token::create("tokenb".to_string()).unwrap(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn instantiate_honors_supplied_decimals() {
        // The lp_token contract is a flexible cw20: it does not pin decimals
        // itself, it reports whatever the instantiate msg supplies. Pinning to
        // LP_TOKEN_DECIMAL (18) is the factory's job.
        for decimals in [LP_TOKEN_DECIMAL, 6u8] {
            let mut deps = mock_dependencies();
            let env = mock_env();
            let factory = deps.api.addr_make("factory");
            let mut msg = sample_instantiate_msg(&deps);
            msg.decimals = decimals;

            instantiate(deps.as_mut(), env.clone(), message_info(&factory, &[]), msg).unwrap();

            let res = query(deps.as_ref(), env, QueryMsg::TokenInfo {}).unwrap();
            let token_info: TokenInfoResponse = from_json(&res).unwrap();
            assert_eq!(
                token_info.decimals, decimals,
                "LP token must report the supplied decimals"
            );
            assert_eq!(token_info.name, "Euclid LP Token");
            assert_eq!(token_info.symbol, "euclidLP");
            assert_eq!(token_info.total_supply, Uint128::new(1_000_000));
        }
    }

    #[test]
    fn instantiate_rejects_invalid_symbol() {
        let mut deps = mock_dependencies();
        let factory = deps.api.addr_make("factory");
        let mut msg = sample_instantiate_msg(&deps);
        msg.symbol = "L1".to_string();

        let err =
            instantiate(deps.as_mut(), mock_env(), message_info(&factory, &[]), msg).unwrap_err();
        assert!(
            err.to_string()
                .contains("Ticker symbol is not in expected format"),
            "unexpected error: {err}"
        );
    }
}
