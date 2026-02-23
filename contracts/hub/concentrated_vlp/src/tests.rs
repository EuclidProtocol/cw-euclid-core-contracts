#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{coin, Addr};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    fee::{DenomFees, Fee, TotalFees},
    msgs::vlp::{
        base::{PoolConfig, PoolType, VlpConcentratedRegisterPoolMsg},
        concentrated::msg::{ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg},
    },
    token::{Pair, Token},
};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};

use crate::mock::mock_concentrated_vlp;

#[test]
fn test_proper_instantiation() {
    let mut app = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut app, "admin")
        .with_wallets(vec![("owner", vec![coin(1000, "eucl")])])
        .with_contracts(vec![("concentrated_vlp", mock_concentrated_vlp())])
        .build(&mut app);

    let owner = andr.get_wallet("owner");

    let pair = Pair::new(
        Token::create("tokena".to_string()).unwrap(),
        Token::create("tokenb".to_string()).unwrap(),
    )
    .unwrap();

    let fee = Fee::new(
        10,
        10,
        CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "recipient".to_string()),
    );

    let contract = app
        .instantiate_contract(
            1,
            owner.clone(),
            &InstantiateMsg {
                router: owner.clone(),
                virtual_balance_contract: owner.clone(),
                pair: pair.clone(),
                fee: fee.clone(),
                execute: Some(ExecuteMsg::RegisterPool(VlpConcentratedRegisterPoolMsg {
                    sender: CrossChainUser::new(
                        ChainUid::create("chain1".to_string()).unwrap(),
                        "user".to_string(),
                    ),
                    pool_key: euclid::msgs::vlp::base::PoolKey {
                        pair: pair.clone(),
                        pool_type: PoolType::Concentrated {
                            fee_tier_bps: 500,
                            tick_spacing: 10,
                        },
                    },
                    tx_id: "tx1".to_string(),
                })),
                admin: owner.clone(),
                fee_tier_bps: 500,
                tick_spacing: 10,
            },
            &[],
            "Concentrated VLP",
            None,
        )
        .unwrap();

    let state: GetStateResponse = app
        .wrap()
        .query_wasm_smart(contract, &QueryMsg::State {})
        .unwrap();

    assert_eq!(state.pair, pair);
    assert_eq!(state.router, owner);
    assert_eq!(state.fee, fee);
    assert_eq!(
        state.pool_config,
        PoolConfig::Concentrated {
            fee_tier_bps: 500,
            tick_spacing: 10
        }
    );
    assert_eq!(
        state.total_fees_collected,
        TotalFees {
            lp_fees: DenomFees {
                totals: Default::default()
            },
            euclid_fees: DenomFees {
                totals: Default::default()
            }
        }
    );
}
