use cosmwasm_std::Uint128;
use cw_orch::prelude::*;

use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::vlp::concentrated::msg::{PositionResponse, QueryMsg as ConcentratedQueryMsg};

use crate::harness::util::root_cause;
use crate::helpers::factory::{
    add_concentrated_liquidity, collect_concentrated_fees, execute_concentrated_swap,
    list_position_ids, pair_with_amounts, remove_concentrated_liquidity,
};

use super::config::TrackedPosition;
use super::ConcentratedPool;

impl ConcentratedPool {
    /// Execute a swap through the factory→router→VLP pipeline.
    pub(crate) fn swap(&mut self, amount: u128, zero_for_one: bool) -> Result<(), String> {
        let (asset_in, asset_out) = if zero_for_one {
            (self.token_a.clone(), self.token_b.token.clone())
        } else {
            (self.token_b.clone(), self.token_a.token.clone())
        };

        let amount_out = execute_concentrated_swap(
            &self.factory,
            &self.router,
            self.pool_key(),
            asset_in,
            asset_out,
            Uint128::new(amount),
        )
        .map_err(|e| format!("swap: {}", root_cause(&e)))?;

        if amount_out.is_zero() {
            Err("swap returned zero output".to_string())
        } else {
            Ok(())
        }
    }

    /// Add a liquidity position and track it if successful.
    pub(crate) fn add_liquidity(
        &mut self,
        lower_tick: i64,
        upper_tick: i64,
        amount_0: u128,
        amount_1: u128,
    ) -> Result<(), String> {
        let pair = pair_with_amounts(&self.token_a, &self.token_b, amount_0, amount_1);

        add_concentrated_liquidity(
            &self.factory,
            &self.router,
            pair,
            self.pool_key(),
            lower_tick,
            upper_tick,
            None,
            self.config.slippage_tolerance_bps,
        )
        .map_err(|e| format!("add_liquidity: {}", root_cause(&e)))?;

        // Sync from position token NFTs to discover the newly minted position.
        // This is more robust than parsing event attributes, which can fail
        // if the attribute key changes or is missing from relay events.
        self.sync_positions();
        Ok(())
    }

    /// Remove liquidity from a tracked position by fraction (in basis points).
    pub(crate) fn remove_liquidity(
        &mut self,
        position_idx: usize,
        fraction_bps: u64,
    ) -> Result<(), String> {
        if position_idx >= self.positions.len() {
            return Err("position index out of bounds".to_string());
        }

        let position = &self.positions[position_idx];
        // Cache tick bounds before removal — the position may be removed from
        // self.positions during this call (full removal), but the T5 burn
        // invariant needs the tick range to check whether active liquidity
        // should have decreased. Must be set before execute, not after.
        self.last_remove_ticks = Some((position.lower_tick, position.upper_tick));
        let position_id = position.position_id;

        let pos_response: Result<PositionResponse, _> = self
            .vlp()
            .query(&ConcentratedQueryMsg::Position { position_id });

        let current_liquidity = match pos_response {
            Ok(pos) => pos.liquidity,
            Err(_) => return Err("position not found".to_string()),
        };

        if current_liquidity.is_zero() {
            return Err("position has zero liquidity".to_string());
        }

        let remove_amount = current_liquidity.multiply_ratio(fraction_bps as u128, 10_000u128);

        if remove_amount.is_zero() {
            return Err("remove amount rounds to zero".to_string());
        }

        remove_concentrated_liquidity(
            &self.factory,
            &self.router,
            self.pool_key(),
            position_id,
            remove_amount,
        )
        .map_err(|e| format!("remove_liquidity: {}", root_cause(&e)))?;

        // With auto-collect on full removal, the contract deletes the
        // position when liquidity reaches zero (regardless of whether
        // it was a full or partial remove that got there). Check
        // on-chain to keep tracking in sync.
        let remaining = self.query_position_liquidity(position_id);
        if remaining.is_zero() {
            self.positions.remove(position_idx);
        }
        Ok(())
    }

    /// Collect accrued fees for a tracked position.
    pub(crate) fn collect_fees(&mut self, position_idx: usize) -> Result<(), String> {
        if position_idx >= self.positions.len() {
            return Err("position index out of bounds".to_string());
        }

        let position = &self.positions[position_idx];
        let chain_uid = self.chain_uid();
        let recipient =
            CrossChainUser::new(chain_uid, self.factory.environment().sender.to_string());

        collect_concentrated_fees(
            &self.factory,
            &self.router,
            self.pool_key(),
            position.position_id,
            recipient,
        )
        .map_err(|e| format!("collect_fees: {}", root_cause(&e)))
    }

    /// Sync tracked positions with on-chain state by querying the position
    /// token contract and adding any positions not yet in self.positions.
    /// New positions are attributed to the current factory sender.
    pub(crate) fn sync_positions(&mut self) {
        let position_ids = list_position_ids(&self.factory).unwrap_or_default();
        let vlp = self.vlp();
        let current_sender = self.factory.environment().sender.to_string();

        for id_str in &position_ids {
            if let Ok(id) = id_str.parse::<u128>() {
                if self.positions.iter().any(|p| p.position_id.u128() == id) {
                    continue;
                }
                if let Ok(pos) = vlp.query::<PositionResponse>(&ConcentratedQueryMsg::Position {
                    position_id: Uint128::new(id),
                }) {
                    self.positions.push(TrackedPosition {
                        position_id: pos.position_id,
                        owner: current_sender.clone(),
                        lower_tick: pos.lower_tick_index,
                        upper_tick: pos.upper_tick_index,
                    });
                }
            }
        }
    }
}
