use cosmwasm_std::{to_json_binary, Binary, Deps, Uint128};
use euclid::{
    error::ContractError,
    msgs::vcoin::{GetBalanceResponse, GetStateResponse, GetUserBalancesResponse},
    vcoin::BalanceKey,
};
