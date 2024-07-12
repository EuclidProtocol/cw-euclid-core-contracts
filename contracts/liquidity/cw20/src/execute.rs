use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response, Uint128};
use euclid::{
    error::ContractError,
    msgs::vcoin::{ExecuteBurn, ExecuteMint, ExecuteTransfer},
    vcoin::BalanceKey,
};
