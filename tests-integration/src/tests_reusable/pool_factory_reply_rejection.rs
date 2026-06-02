#![cfg(not(target_arch = "wasm32"))]

//! Defence-in-depth test: with the configured pool factory address pointing
//! at a malicious stub that returns a non-pool `RouterCrossChainExecuteMsg`
//! in its `Response::data`, main factory's `on_pool_factory_delegate_reply`
//! MUST reject the tx before any IBC packet or native router callback is
//! emitted.

#[cfg(test)]
mod tests {
    use cosmwasm_std::Uint256;
    use cw_orch::prelude::*;
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::factory::ExecuteMsgFns as FactoryExecuteMsgFns;
    use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::msgs::vlp::base::PoolConfig;
    use euclid::token::{
        PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
    };

    use crate::helpers::chains::{setup_interchain, setup_router};
    use crate::helpers::factory::faucet;
    use crate::helpers::malicious_pool_factory::MaliciousPoolFactoryContract;
    use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID};
    use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;

    /// With main factory pointed at a malicious stub that returns a non-pool
    /// packet via `Response::data`, the `RequestPoolCreation` user flow must
    /// fail in the reply handler — the defence-in-depth `is_pool_variant`
    /// check rejects the decoded `RouterCrossChainExecuteMsg`.
    #[test]
    fn test_pool_factory_delegate_reply_rejects_non_pool_variant() {
        let sender = "sender_for_all_chains";
        let factory_chain_id = FACTORY_CHAIN_ID_LOCAL;
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

        let factory = setup_factory_with_mode(
            &interchain,
            factory_chain_id,
            &router,
            FactorySetupMode::Native,
        )
        .unwrap();
        let chain = interchain.get_chain(factory_chain_id).unwrap();

        // Deploy and instantiate the malicious stub.
        let stub = MaliciousPoolFactoryContract::new(chain.clone());
        stub.upload().unwrap();
        stub.instantiate(
            &euclid::msgs::pool_factory::InstantiateMsg {
                main_factory_address: factory.address().unwrap().to_string(),
            },
            None,
            &[],
        )
        .unwrap();

        // Bootstrap: wire the stub as pool factory so main factory takes the
        // delegated code path.
        factory
            .set_pool_factory(stub.address().unwrap().to_string())
            .unwrap();

        let pf_resp = factory.query_pool_factory_address().unwrap();
        assert!(pf_resp.initialised, "stub must be wired as pool_factory");

        // Pool creation requires registered denoms upstream.
        let token_a = TokenWithDenom {
            token: Token::create("tokena".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokena".to_string(),
                decimals: Some(6),
            },
        };
        let token_b = TokenWithDenom {
            token: Token::create("tokenb".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokenb".to_string(),
                decimals: Some(6),
            },
        };
        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint256::from(10_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint256::from(10_000u128),
            },
        };

        // Fund the sender with both native tokens so the request reaches the
        // delegate dispatch before erroring in the reply handler.
        let mut funds = vec![];
        for token in pair.get_vec_token_info() {
            faucet(
                &chain,
                chain.sender.as_str(),
                cosmwasm_std::Uint128::try_from(token.amount)
                    .unwrap()
                    .u128(),
                token.token_type.clone(),
                &mut funds,
            );
        }

        let res = factory.request_pool_creation(
            CrossChainConfig::default(),
            6,
            "LPSYMBOL".to_string(),
            "LPNAME".to_string(),
            pair,
            PoolConfig::ConstantProduct {},
            500,
            None,
            &funds,
        );

        let err = res.expect_err("malicious reply data must cause the tx to error");
        // anyhow's `{:#}` formatter walks the full chain so the underlying
        // reply-handler error (wrapped through cw-multi-test) is visible.
        let err_chain = format!("{err:#}");
        assert!(
            err_chain.contains("non-pool")
                || err_chain.contains("is_pool_variant")
                || err_chain.contains("on_pool_factory_delegate_reply"),
            "expected reply-handler pool-variant rejection, got: {err_chain}"
        );
    }
}
