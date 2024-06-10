use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use euclid::{msgs::escrow::AmountAndType, token::Token};

pub const TOKEN_ID: Item<Token> = Item::new("token_id");
pub const ALLOWED_DENOMS: Item<Vec<String>> = Item::new("allowed_denoms");
pub const FACTORY_ADDRESS: Item<Addr> = Item::new("factory_address");

pub const DENOM_TO_AMOUNT: Map<String, AmountAndType> = Map::new("denom_to_amount");
