use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, IbcTimeout, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{
    error::ContractError,
    liquidity::{self, LiquidityTxInfo},
    pool::{generate_id, PoolRequest},
    swap::{self, SwapInfo},
    token::{PairInfo, Token, TokenInfo},
};

pub const TOKEN_ID: Item<Token> = Item::new("token_id");
pub const ALLOWED_DENOMS: Item<Vec<String>> = Item::new("allowed_denoms");
pub const FACTORY_ADDRESS: Item<Addr> = Item::new("factory_address");
