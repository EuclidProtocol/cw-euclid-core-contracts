#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate};
    use crate::state::{State, STATE};
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, Decimal256, DepsMut, Response, Uint128, Uint64};
    use euclid::chain::{ChainUid, CrossChainUser};
    use euclid::error::ContractError;
    use euclid::fee::{DenomFees, Fee, TotalFees};
    use euclid::msgs::stable_vlp::{ExecuteMsg, InstantiateMsg};
    use euclid::token::{Pair, Token};
    use std::collections::HashMap;
}
