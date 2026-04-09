use cosmwasm_std::{ensure, DepsMut, Empty, Int256, MessageInfo, Response, Uint128};
use euclid::{
    error::ContractError,
    msgs::position_token::{PositionInfo, TokenInfo},
};

use crate::state::{OWNER_TOKEN_SET, POSITION_INFO, STATE, TOKENS};

pub(crate) fn execute_mint(
    deps: DepsMut,
    info: &MessageInfo,
    token_id: Uint128,
    token_info: TokenInfo,
    position_info: PositionInfo,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let token_id_str = token_id.to_string();

    ensure!(info.sender == state.factory, ContractError::Unauthorized {});
    ensure!(
        !TOKENS.has(deps.storage, &token_id_str),
        ContractError::TokenAlreadyExist {}
    );

    let owner_addr = deps.api.addr_validate(token_info.owner.as_str())?;
    TOKENS.save(
        deps.storage,
        &token_id_str,
        &TokenInfo {
            owner: owner_addr.clone(),
            token_uri: token_info.token_uri,
        },
    )?;
    POSITION_INFO.save(deps.storage, &token_id_str, &position_info)?;

    OWNER_TOKEN_SET.save(deps.storage, (&owner_addr, &token_id_str), &Empty {})?;
    state.total_tokens += 1;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "mint_position")
        .add_attribute("token_id", token_id)
        .add_attribute("owner", owner_addr)
        .add_attribute("vlp_address", position_info.vlp_address)
        .add_attribute("liquidity", position_info.liquidity.to_string())
        .add_attribute("total_tokens", state.total_tokens.to_string()))
}

pub(crate) fn execute_burn(
    deps: DepsMut,
    info: MessageInfo,
    token_id: Uint128,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let token_id_str = token_id.to_string();

    let token = TOKENS
        .may_load(deps.storage, &token_id_str)?
        .ok_or(ContractError::NotFound {
            msg: format!("token {token_id} not found"),
        })?;

    let position =
        POSITION_INFO
            .may_load(deps.storage, &token_id_str)?
            .ok_or(ContractError::NotFound {
                msg: format!("position {token_id} not found"),
            })?;

    // Only factory can burn a position when liquidity is zero
    ensure!(info.sender == state.factory, ContractError::Unauthorized {});

    ensure!(
        position.liquidity.is_zero(),
        ContractError::new("Cannot burn a position with liquidity")
    );

    TOKENS.remove(deps.storage, &token_id_str);
    POSITION_INFO.remove(deps.storage, &token_id_str);
    OWNER_TOKEN_SET.remove(deps.storage, (&token.owner, &token_id_str));
    state.total_tokens -= 1;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "burn_position")
        .add_attribute("vlp_address", position.vlp_address)
        .add_attribute("token_id", token_id)
        .add_attribute("total_tokens", state.total_tokens.to_string()))
}

pub(crate) fn execute_transfer(
    deps: DepsMut,
    info: MessageInfo,
    token_id: Uint128,
    recipient: String,
) -> Result<Response, ContractError> {
    let token_id_str = token_id.to_string();
    let mut token =
        TOKENS
            .may_load(deps.storage, &token_id_str)?
            .ok_or(ContractError::NotFound {
                msg: format!("token {token_id} not found"),
            })?;

    let position =
        POSITION_INFO
            .may_load(deps.storage, &token_id_str)?
            .ok_or(ContractError::NotFound {
                msg: format!("position {token_id} not found"),
            })?;

    // Only token owner can transfer a position
    ensure!(info.sender == token.owner, ContractError::Unauthorized {});

    let recipient_addr = deps.api.addr_validate(recipient.as_str())?;
    ensure!(recipient_addr != token.owner, ContractError::SameAddress {});

    OWNER_TOKEN_SET.remove(deps.storage, (&token.owner, &token_id_str));
    OWNER_TOKEN_SET.save(deps.storage, (&recipient_addr, &token_id_str), &Empty {})?;

    token.owner = recipient_addr.clone();
    TOKENS.save(deps.storage, &token_id_str, &token)?;

    Ok(Response::new()
        .add_attribute("action", "transfer_position")
        .add_attribute("vlp_address", position.vlp_address)
        .add_attribute("token_id", token_id)
        .add_attribute("recipient", recipient_addr))
}

pub(crate) fn execute_update_position(
    deps: DepsMut,
    info: MessageInfo,
    token_id: Uint128,
    liquidity_change: Int256,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let token_id_str = token_id.to_string();
    let mut position =
        POSITION_INFO
            .may_load(deps.storage, &token_id_str)?
            .ok_or(ContractError::NotFound {
                msg: format!("position {token_id} not found"),
            })?;
    ensure!(info.sender == state.factory, ContractError::Unauthorized {});

    let abs_delta = convert_int256_to_uint128(liquidity_change)?;

    if liquidity_change.is_negative() {
        position.liquidity = position
            .liquidity
            .checked_sub(abs_delta)
            .map_err(|_| ContractError::new("liquidity change overflow"))?;
    } else {
        position.liquidity = position
            .liquidity
            .checked_add(abs_delta)
            .map_err(|_| ContractError::new("liquidity change overflow"))?;
    }
    POSITION_INFO.save(deps.storage, &token_id_str, &position)?;
    Ok(Response::new()
        .add_attribute("action", "update_position")
        .add_attribute("token_id", token_id)
        .add_attribute("vlp_address", position.vlp_address)
        .add_attribute("new_liquidity", position.liquidity.to_string()))
}

fn convert_int256_to_uint128(liquidity_change: Int256) -> Result<Uint128, ContractError> {
    liquidity_change
        .unsigned_abs()
        .try_into()
        .map_err(|_| ContractError::new("liquidity change overflow"))
}

#[cfg(test)]
mod tests {
    use std::ops::Add;

    use cosmwasm_std::testing::{message_info, mock_dependencies};
    use cosmwasm_std::{Int256, Uint128};
    use euclid::msgs::position_token::{PositionInfo, State};
    use rstest::rstest;

    use crate::state::{POSITION_INFO, STATE};

    use super::execute_update_position;

    fn setup_position(
        initial_liquidity: u128,
    ) -> (
        cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        cosmwasm_std::Addr,
        Uint128,
    ) {
        let mut deps = mock_dependencies();
        let factory = deps.api.addr_make("factory");
        let token_id = Uint128::new(1);

        STATE
            .save(
                deps.as_mut().storage,
                &State {
                    name: "Position Token".to_string(),
                    symbol: "POS".to_string(),
                    factory: factory.clone(),
                    total_tokens: 1,
                },
            )
            .unwrap();
        POSITION_INFO
            .save(
                deps.as_mut().storage,
                &token_id.to_string(),
                &PositionInfo {
                    liquidity: Uint128::new(initial_liquidity),
                    vlp_address: "vlp-1".to_string(),
                },
            )
            .unwrap();

        (deps, factory, token_id)
    }

    #[rstest]
    #[case::int256_zero(Int256::from(0), Uint128::zero(), false)]
    #[case::int256_positive(Int256::from(10), Uint128::new(10), false)]
    #[case::int256_positive_overflow(Int256::from(u128::MAX).add(Int256::from(1)), Uint128::MAX, true)]
    #[case::int256_negative(Int256::from(-10), Uint128::new(10), false)]
    #[case::int256_negative_overflow(-Int256::from(u128::MAX) - Int256::from(1), Uint128::MAX, true)]
    fn convert_int256_to_uint128(
        #[case] int_value: Int256,
        #[case] uint_value: Uint128,
        #[case] expect_overflow: bool,
    ) {
        let result = super::convert_int256_to_uint128(int_value);
        match result {
            Ok(value) => {
                if expect_overflow {
                    panic!("expected overflow, got {value:?}");
                } else {
                    assert_eq!(value, uint_value);
                }
            }
            Err(err) => {
                if expect_overflow {
                    assert!(err.to_string().contains("liquidity change overflow"));
                } else {
                    panic!("expected success, got {err:?}");
                }
            }
        }
    }

    #[rstest]
    #[case::zero_delta(Int256::zero(), 1_000u128)]
    #[case::positive_small(Int256::from(1i128), 1_001u128)]
    #[case::positive_medium(Int256::from(250i128), 1_250u128)]
    #[case::negative_small(Int256::from(-1i128), 999u128)]
    #[case::negative_medium(Int256::from(-250i128), 750u128)]
    #[case::negative_to_zero(Int256::from(-1_000i128), 0u128)]
    fn execute_update_position_updates_liquidity_across_range(
        #[case] liquidity_change: Int256,
        #[case] expected_liquidity: u128,
    ) {
        let (mut deps, factory, token_id) = setup_position(1_000);

        let response = execute_update_position(
            deps.as_mut(),
            message_info(&factory, &[]),
            token_id.clone(),
            liquidity_change,
        )
        .unwrap();

        let token_id_str = token_id.to_string();
        let updated_position = POSITION_INFO
            .load(deps.as_ref().storage, &token_id_str)
            .unwrap();
        assert_eq!(updated_position.liquidity, Uint128::new(expected_liquidity));
        assert_eq!(response.attributes[0].value, "update_position");
    }

    #[rstest]
    #[case::just_below_zero(Int256::from(-1_001i128))]
    #[case::int256_min(Int256::MIN)]
    fn execute_update_position_rejects_underflow(#[case] liquidity_change: Int256) {
        let (mut deps, factory, token_id) = setup_position(1_000);

        let result = execute_update_position(
            deps.as_mut(),
            message_info(&factory, &[]),
            token_id,
            liquidity_change,
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::increment_from_u128_max(u128::MAX, Int256::from(1i128))]
    fn execute_update_position_rejects_positive_overflow_at_u128_max(
        #[case] start_liquidity: u128,
        #[case] liquidity_change: Int256,
    ) {
        let (mut deps, factory, token_id) = setup_position(start_liquidity);
        let token_id_str = token_id.to_string();

        let result = execute_update_position(
            deps.as_mut(),
            message_info(&factory, &[]),
            token_id,
            liquidity_change,
        );
        match result {
            Err(err) => assert!(
                err.to_string().contains("liquidity change overflow"),
                "got {err:?}"
            ),
            _ => {
                let updated_position = POSITION_INFO
                    .load(deps.as_ref().storage, &token_id_str)
                    .unwrap();
                panic!("expected overflow error, got {updated_position:?}");
            }
        }
    }

    #[rstest]
    #[case::int256_max(1_000u128, Int256::from(u128::MAX))]
    #[case::int256_min(1_000u128, -Int256::from(u128::MAX))]
    fn execute_update_position_rejects_delta_bigger_than_u128(
        #[case] start_liquidity: u128,
        #[case] liquidity_change: Int256,
    ) {
        let (mut deps, factory, token_id) = setup_position(start_liquidity);
        let token_id_str = token_id.to_string();

        let result = execute_update_position(
            deps.as_mut(),
            message_info(&factory, &[]),
            token_id,
            liquidity_change,
        );

        match result {
            Err(err) => assert!(
                err.to_string().contains("liquidity change overflow"),
                "got {err:?}"
            ),
            _ => {
                let updated_position = POSITION_INFO
                    .load(deps.as_ref().storage, &token_id_str)
                    .unwrap();
                panic!("expected overflow error, got {updated_position:?}");
            }
        }
    }

    #[rstest]
    #[case::upper_boundary_no_wrap(
        u128::MAX - 1,
        Int256::from(1i128),
        Uint128::MAX,
        Int256::from(1i128)
    )]
    #[case::lower_boundary_no_wrap(
        1u128,
        Int256::from(-1i128),
        Uint128::zero(),
        Int256::from(-1i128)
    )]
    fn execute_update_position_does_not_wrap_at_boundaries(
        #[case] start_liquidity: u128,
        #[case] first_delta: Int256,
        #[case] expected_after_first: Uint128,
        #[case] second_delta: Int256,
    ) {
        let (mut deps, factory, token_id) = setup_position(start_liquidity);
        let token_id_str = token_id.to_string();

        // First step reaches the boundary exactly.
        execute_update_position(
            deps.as_mut(),
            message_info(&factory, &[]),
            token_id,
            first_delta,
        )
        .unwrap();
        let at_boundary = POSITION_INFO
            .load(deps.as_ref().storage, &token_id_str)
            .unwrap();
        assert_eq!(at_boundary.liquidity, expected_after_first);

        // Second step must error instead of wrapping.
        let wrap_attempt = execute_update_position(
            deps.as_mut(),
            message_info(&factory, &[]),
            token_id,
            second_delta,
        );
        assert!(wrap_attempt.is_err());
    }
}
