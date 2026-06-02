use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Uint256};

use crate::{
    admin::AdminType,
    chain::{ChainType, ChainUid, CosmosChain, EvmChain},
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::{cross_chain_config::CrossChainConfig, hook::MetaReceive},
    recipient::Recipient,
    token::Token,
};

#[cw_serde]
#[cfg_attr(not(target_arch = "wasm32"), derive(cw_orch::ExecuteFns))]
pub enum ExecuteMsg {
    ManageRouterState(ManageRouterState),
    RegisterFactory {
        chain_uid: ChainUid,
        chain_info: RegisterFactoryChainType,
    },
    WithdrawVoucher {
        token: Token,
        amount: Uint256,
        recipient: Recipient,
        cross_chain_config: CrossChainConfig,
    },
    TransferVoucher {
        token: Token,
        amount: Uint256,
        recipient: Vec<Recipient>,
    },

    MetaReceive(MetaReceive),

    // NATIVE RECEIVE CALLBACK
    NativeReceiveCallback {
        msg: Binary,
        chain_uid: ChainUid,
    },

    SendPacket {
        sender: String,
        msg: Binary,
        chain: crate::chain::Chain,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
    },

    ReceivePacket {
        source_port: String,
        destination_port: String,
        msg: Binary,
        sequence: u128,
        timeout: u64,
    },

    ReceivePacketInternalCallback {
        msg: Binary,
        chain_uid: ChainUid,
        timeout: u64,
    },

    AcknowledgePacket {
        source_port: String,
        destination_port: String,
        msg: Binary,
        sequence: u128,
        ack: Binary,
    },
}

#[cw_serde]
pub enum ManageRouterState {
    // Contract admin
    Admins {
        admin_type: AdminType,
        admin: String,
    },
    // Pool Code ID
    Vlp {
        vlp_code_id: Option<u64>,
        stable_vlp_code_id: Option<u64>,
        concentrated_vlp_code_id: Option<u64>,
    },
    LockState {
        locked: bool,
    },
    RelayerContract {
        relayer_contract: Addr,
    },
    MetaTransactionContract {
        meta_transaction_contract: Addr,
    },
    UpdateFeeState {
        release_fee_recipient: Option<Addr>,
        default_fee_recipient: Option<Addr>,
    },
    UpdateReleaseFee {
        token: Token,
        chain_uid: ChainUid,
        release_fee: Uint256,
    },
    UpdateDefaultReleaseFee {
        default_release_fee: Uint256,
    },
    LockChain {
        chain: ChainUid,
    },
    UnlockChain {
        chain: ChainUid,
    },
    UpdateChainTimeout {
        chain_uid: ChainUid,
        timeout: u64,
    },
    /// Fee-admin-gated. `Some(bps)` upserts a per-wallet Euclid-fee override
    /// (validated against the max-fee bound); `None` removes the entry. An
    /// absent entry means the wallet uses the pool's configured Euclid fee.
    SetEuclidFeeOverride {
        user: CrossChainUser,
        euclid_fee_bps: Option<u64>,
    },
}

#[cw_serde]
pub enum RegisterFactoryChainType {
    Native(RegisterFactoryChainNative),
    Cosmos(RegisterFactoryChainCosmos),
    Evm(RegisterFactoryChainEvm),
}

impl RegisterFactoryChainType {
    pub fn tmp_chain_type(&self) -> Result<ChainType, ContractError> {
        match self {
            RegisterFactoryChainType::Native(_native_info) => Ok(ChainType::Native {}),
            RegisterFactoryChainType::Cosmos(cosmos_info) => Ok(ChainType::Cosmos(CosmosChain {
                chain_id: cosmos_info.factory_chain_id.clone(),
            })),
            RegisterFactoryChainType::Evm(evm_info) => Ok(ChainType::Evm(EvmChain {
                chain_id: evm_info.factory_chain_id.clone(),
            })),
        }
    }

    pub fn factory_address(&self) -> String {
        match self {
            RegisterFactoryChainType::Native(native_info) => native_info.factory_address.clone(),
            RegisterFactoryChainType::Cosmos(cosmos_info) => cosmos_info.factory_address.clone(),
            RegisterFactoryChainType::Evm(evm_info) => evm_info.factory_address.clone(),
        }
    }
}

#[cw_serde]
pub struct RegisterFactoryChainNative {
    pub factory_address: String,
    pub factory_chain_id: String,
}

#[cw_serde]
pub struct RegisterFactoryChainEvm {
    pub factory_address: String,
    pub factory_chain_id: String,
}

#[cw_serde]
pub struct RegisterFactoryChainCosmos {
    pub factory_address: String,
    pub factory_chain_id: String,
}
