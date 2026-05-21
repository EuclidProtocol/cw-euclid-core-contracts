#![cfg(not(target_arch = "wasm32"))]

use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::factory::ExecuteMsgFns as FactoryExecuteMsgFns;
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
