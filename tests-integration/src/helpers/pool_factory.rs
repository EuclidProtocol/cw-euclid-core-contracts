#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Addr;
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::factory::ExecuteMsgFns as FactoryExecuteMsgFns;
use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
use euclid::token::Pair;
use factory::FactoryContract;
use pool_factory::PoolFactoryContract;
use router::RouterContract;

use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};

/// Deploys main factory (existing flow), then deploys pool_factory pointing
/// at main factory, then calls `SetPoolFactory` so subsequent
/// `RequestPoolCreation` calls take the delegated path.
///
/// Returns the pair so tests can assert against both surfaces.
pub fn setup_factory_with_pool_factory(
    interchain: &MockInterchainEnv,
    factory_chain_id: &str,
    router: &RouterContract<MockBase>,
    mode: FactorySetupMode,
) -> Result<(FactoryContract<MockBase>, PoolFactoryContract<MockBase>), CwOrchError> {
    let factory = setup_factory_with_mode(interchain, factory_chain_id, router, mode)?;
    let chain = interchain.get_chain(factory_chain_id).unwrap();

    let pool_factory = PoolFactoryContract::new(chain.clone());
    pool_factory.upload().unwrap();
    pool_factory.instantiate(
        &euclid::msgs::pool_factory::InstantiateMsg {
            main_factory_address: factory.address()?.to_string(),
        },
        None,
        &[],
    )?;

    // Bootstrap entry: migration_admin (the default test sender) sets the
    // pool factory address on main factory. One-shot — second call would
    // error.
    factory.set_pool_factory(pool_factory.address()?.to_string())?;

    Ok((factory, pool_factory))
}

/// Slice 2 helper: deploy main factory + pool_factory, then snapshot main
/// factory's existing CP/Stable pool registries and replay them into
/// pool_factory via `MigrateAcceptPoolState`. The replay impersonates main
/// factory (the only authorised caller) via `set_sender`. Used to bridge a
/// factory that already has pools registered to the delegated path for
/// follow-on operations like add-liquidity.
pub fn migrate_pool_state_to_pool_factory(
    factory: &FactoryContract<MockBase>,
    pool_factory: &PoolFactoryContract<MockBase>,
    pairs: &[Pair],
) -> Result<(), CwOrchError> {
    // Gather PAIR_TO_VLP / VLP_TO_LP_TOKEN entries from main factory.
    let mut pair_to_vlp: Vec<(Pair, String)> = Vec::new();
    let mut vlp_to_lp_token: Vec<(String, Addr)> = Vec::new();
    for pair in pairs {
        let vlp_resp = factory.get_vlp(pair.clone())?;
        let vlp_address = vlp_resp.vlp_address;
        pair_to_vlp.push((pair.clone(), vlp_address.clone()));
        let lp_resp = factory.get_lp_token(vlp_address.clone())?;
        vlp_to_lp_token.push((vlp_address, lp_resp.token_address));
    }

    // Impersonate main factory to satisfy the migrate auth check.
    let original_sender = pool_factory.environment().sender.clone();
    let main_factory_addr = factory.address()?;
    let mut pf_handle = pool_factory.clone();
    pf_handle.set_sender(&main_factory_addr);
    pf_handle.execute(
        &euclid::msgs::pool_factory::ExecuteMsg::MigrateAcceptPoolState {
            pair_to_vlp,
            vlp_to_lp_token,
            concentrated_vlps: None,
            position_token_contract: None,
        },
        &[],
    )?;
    // Restore the original test sender so subsequent contract calls in the
    // test continue to act as the developer/test wallet.
    pf_handle.set_sender(&original_sender);
    Ok(())
}
