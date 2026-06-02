use concentrated_vlp::ConcentratedVlpContract;
use cosmwasm_std::Addr;
use cp_vlp::VlpContract;
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use cw_orch_interchain::core::IbcQueryHandler;
use euclid::msgs::router::execute::ExecuteMsgFns as RouterExecuteMsgFns;
use euclid::msgs::router::{ManageRouterState, RegisterFactoryChainNative};
use euclid_relayer::RelayerContract;
use factory::FactoryContract;
use meta_transaction::MetaTransactionContract;
use relayer::verify::cosmos_address_from_pubkey;
use relayer::{ExecuteMsgFns as RelayerExecuteMsgFns, Validator};
use router::RouterContract;
use stable_vlp::StableVlpContract;
use virtual_balance::VirtualBalanceContract;

use crate::relayer::get_signer_key;

pub const ROUTER_CHAIN_ID: &str = "neuron";

/// Deploy and configure a router contract with all VLP code IDs and a relayer.
pub fn setup_router(
    chain: &MockBase,
    factory_chains: Vec<&str>,
) -> Result<RouterContract<MockBase>, CwOrchError> {
    let router = RouterContract::new(chain.clone());
    let virtual_balance = VirtualBalanceContract::new(chain.clone());
    let vlp = VlpContract::new(chain.clone());
    let stable_vlp = StableVlpContract::new(chain.clone());
    let concentrated_vlp = ConcentratedVlpContract::new(chain.clone());
    let relayer = setup_relayer(chain, factory_chains)?;

    router.upload().expect("router upload should succeed");
    virtual_balance
        .upload()
        .expect("virtual_balance upload should succeed");
    vlp.upload().expect("vlp upload should succeed");
    stable_vlp
        .upload()
        .expect("stable_vlp upload should succeed");
    concentrated_vlp
        .upload()
        .expect("concentrated_vlp upload should succeed");

    router.instantiate(
        &euclid::msgs::router::InstantiateMsg {
            constant_product_vlp_code_id: vlp.code_id().expect("vlp should have code_id"),
            stable_vlp_code_id: stable_vlp
                .code_id()
                .expect("stable_vlp should have code_id"),
            concentrated_vlp_code_id: concentrated_vlp
                .code_id()
                .expect("concentrated_vlp should have code_id"),
            virtual_balance_code_id: virtual_balance
                .code_id()
                .expect("virtual_balance should have code_id"),
            relayer_contract: relayer.address().expect("relayer should have address"),
            release_fee_recipient: chain.addr_make("release_fee_recipient"),
            default_fee_recipient: chain.addr_make("default_fee_recipient"),
        },
        None,
        &[],
    )?;

    let meta_transaction_contract = setup_meta_transaction_contract(&router)?;
    router.manage_router_state(ManageRouterState::MetaTransactionContract {
        meta_transaction_contract: meta_transaction_contract
            .address()
            .expect("meta_transaction should have address"),
    })?;

    Ok(router)
}

/// Deploy and configure a relayer contract with a single validator for given chain UIDs.
pub fn setup_relayer(
    chain: &MockBase,
    chain_uids: Vec<&str>,
) -> Result<RelayerContract<MockBase>, CwOrchError> {
    let relayer = RelayerContract::new(chain.clone());
    let (_, pubkey_binary) = get_signer_key();
    let validator_address = cosmos_address_from_pubkey(&pubkey_binary, "cosmos")
        .expect("pubkey should produce address");

    relayer.upload().expect("relayer upload should succeed");

    let validator = Validator {
        pubkey: pubkey_binary,
        address: validator_address,
    };

    relayer.instantiate(
        &relayer::msgs::InstantiateMsg {
            message_signer: validator.clone(),
            signature_threshold: 1,
        },
        Some(&chain.sender),
        &[],
    )?;

    for chain_uid in chain_uids {
        relayer.add_validator(
            euclid::chain::ChainUid::create(chain_uid.to_string())
                .expect("chain_uid should be valid"),
            validator.clone(),
        )?;
    }
    Ok(relayer)
}

/// Deploy a meta-transaction contract linked to the given router.
pub fn setup_meta_transaction_contract(
    router: &RouterContract<MockBase>,
) -> Result<MetaTransactionContract<MockBase>, CwOrchError> {
    let chain = router.environment().clone();
    let meta_transaction_contract = MetaTransactionContract::new(chain);
    meta_transaction_contract
        .upload()
        .expect("meta_transaction upload should succeed");
    meta_transaction_contract.instantiate(
        &euclid::msgs::meta_transaction::msg::InstantiateMsg {
            router_contract: router.address().expect("router should have address"),
        },
        None,
        &[],
    )?;
    Ok(meta_transaction_contract)
}

/// Deploy a native factory with escrow, LP token, position token, and register it with the router.
pub fn setup_factory_native(
    interchain: &cw_orch_interchain::mock::MockInterchainEnv,
    router: &RouterContract<MockBase>,
) -> Result<FactoryContract<MockBase>, CwOrchError> {
    use cosmwasm_std::Uint256;
    use cw_orch_interchain::core::InterchainEnv;
    use escrow::EscrowContract;
    use euclid::chain::ChainUid;
    use lp_token::LpTokenContract;
    use position_token::PositionTokenContract;

    let chain_uid = ChainUid::create(ROUTER_CHAIN_ID.to_string())
        .expect("ROUTER_CHAIN_ID should be a valid chain UID");
    let vsl_chain_uid = ChainUid::vsl_chain_uid().expect("VSL chain UID should be valid");
    let chain = interchain
        .get_chain(ROUTER_CHAIN_ID)
        .expect("router chain should exist in interchain env");
    let factory = FactoryContract::new(chain.clone());
    let escrow = EscrowContract::new(chain.clone());
    let lp_token = LpTokenContract::new(chain.clone());
    let relayer = setup_relayer(&chain, vec![vsl_chain_uid.as_str(), chain_uid.as_str()])?;

    factory.upload().expect("factory upload should succeed");
    escrow.upload().expect("escrow upload should succeed");
    lp_token.upload().expect("lp_token upload should succeed");
    let position_token = PositionTokenContract::new(chain.clone());
    position_token
        .upload()
        .expect("position_token upload should succeed");

    let string_length = ROUTER_CHAIN_ID.len();
    for _ in 0..string_length {
        factory.instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router
                    .address()
                    .expect("router should have address")
                    .to_string(),
                chain_uid: chain_uid.clone(),
                escrow_code_id: escrow.code_id().expect("escrow should have code_id"),
                lp_code_id: lp_token.code_id().expect("lp_token should have code_id"),
                position_token_code_id: position_token
                    .code_id()
                    .expect("position_token should have code_id"),
                relayer_contract: relayer.address().expect("relayer should have address"),
                rate_limit_fee_recipient: chain.addr_make("rate_limit_fee_recipient"),
                rate_limit_fee_denom: "ufee".to_string(),
                rate_limit_free_limit: Uint256::from(10u128),
                is_native: true,
            },
            None,
            &[],
        )?;
    }

    let chain_info =
        euclid::msgs::router::RegisterFactoryChainType::Native(RegisterFactoryChainNative {
            factory_address: factory
                .address()
                .expect("factory should have address")
                .to_string(),
            factory_chain_id: factory.environment().chain_id(),
        });
    router.register_factory(chain_info, chain_uid)?;

    Ok(factory)
}

/// Get a concentrated VLP contract wrapper by address.
pub fn get_concentrated_vlp(chain: &MockBase, address: &Addr) -> ConcentratedVlpContract<MockBase> {
    let mut concentrated_vlp = ConcentratedVlpContract::new(chain.clone());
    concentrated_vlp.as_instance_mut().id = format!("concentrated_vlp_{}", address);
    concentrated_vlp.set_address(address);
    concentrated_vlp
}

/// Get a virtual balance contract wrapper by address.
pub fn get_virtual_balance(chain: &MockBase, address: &Addr) -> VirtualBalanceContract<MockBase> {
    let mut virtual_balance = VirtualBalanceContract::new(chain.clone());
    virtual_balance.as_instance_mut().id = format!("virtual_balance_{}", address);
    virtual_balance.set_address(address);
    virtual_balance
}

/// Get a relayer contract wrapper by address.
pub fn get_relayer(chain: &MockBase, address: &Addr) -> RelayerContract<MockBase> {
    let mut relayer = RelayerContract::new(chain.clone());
    relayer.as_instance_mut().id = format!("relayer_{}", address);
    relayer.set_address(address);
    relayer
}
