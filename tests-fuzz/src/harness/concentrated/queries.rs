use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;

use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    ConcentratedPoolResponse, PositionResponse, ProtocolFeesResponse,
    QueryMsg as ConcentratedQueryMsg, Slot0Response, TickResponse, TicksResponse,
};

use concentrated_vlp::ConcentratedVlpContract;
use cw_orch::mock::MockBase;

use crate::helpers::chains::get_concentrated_vlp;
use crate::invariants::PoolSnapshot;
use crate::strategies::concentrated::PoolState;

use super::ConcentratedPool;

impl ConcentratedPool {
    /// Get the VLP contract address.
    pub(crate) fn vlp_address(&self) -> Addr {
        let vlp = self
            .router
            .get_vlp_by_pool_key(self.pool_key())
            .expect("VLP should exist for pool key");
        Addr::unchecked(vlp.vlp)
    }

    /// Get a typed handle to the VLP contract for queries.
    pub(crate) fn vlp(&self) -> ConcentratedVlpContract<MockBase> {
        get_concentrated_vlp(self.router.environment(), &self.vlp_address())
    }

    /// Get the chain UID from the factory state.
    pub(crate) fn chain_uid(&self) -> euclid::chain::ChainUid {
        self.factory
            .get_state()
            .expect("factory state should exist")
            .chain_uid
    }

    /// Query on-chain liquidity for a position.
    /// Panics if the query fails unexpectedly (corrupted state should not be silent).
    /// Returns zero only when the position genuinely has zero liquidity.
    pub(crate) fn query_position_liquidity(&self, position_id: Uint128) -> Uint128 {
        match self
            .vlp()
            .query::<PositionResponse>(&ConcentratedQueryMsg::Position { position_id })
        {
            Ok(pos) => pos.liquidity,
            Err(e) => {
                let msg = format!("{:?}", e);
                // Position genuinely deleted (zero liquidity, zero owed) → not found is OK
                if msg.contains("not found") || msg.contains("Not found") {
                    Uint128::zero()
                } else {
                    panic!(
                        "query_position_liquidity: unexpected error for position {}: {}",
                        position_id, msg
                    );
                }
            }
        }
    }

    // =========================================================================
    // Building-block queries used by snapshots and pool_state
    // =========================================================================

    /// Query slot0 (current tick, price, liquidity, fee growth globals).
    fn query_slot0(&self) -> Slot0Response {
        self.vlp()
            .query(&ConcentratedQueryMsg::Slot0 {})
            .expect("slot0 query should succeed")
    }

    /// Query pool reserves. Returns (reserve_0, reserve_1) in token_a/token_b order.
    fn query_reserves(&self) -> (Uint128, Uint128) {
        let pool: ConcentratedPoolResponse = self
            .vlp()
            .query(&ConcentratedQueryMsg::Pool {
                chain_uid: self.chain_uid(),
                pool_key: self.pool_key(),
            })
            .expect("pool query should succeed");
        // Pool response uses (reserve_1, reserve_2) keyed by canonical pair ordering,
        // while we use (reserve_0, reserve_1) matching token_a/token_b order.
        (pool.reserve_1, pool.reserve_2)
    }

    /// Query protocol fees.
    fn query_protocol_fees(&self) -> ProtocolFeesResponse {
        self.vlp()
            .query(&ConcentratedQueryMsg::ProtocolFees {})
            .expect("protocol fees query should succeed")
    }

    /// Query all tracked positions from the VLP contract.
    /// Panics if any tracked position fails to query (corrupted state).
    fn query_tracked_positions(&self) -> Vec<PositionResponse> {
        let vlp = self.vlp();
        self.positions
            .iter()
            .map(|tracked| {
                vlp.query::<PositionResponse>(&ConcentratedQueryMsg::Position {
                    position_id: tracked.position_id,
                })
                .unwrap_or_else(|e| {
                    panic!(
                        "snapshot: failed to query tracked position {}: {}",
                        tracked.position_id, e
                    )
                })
            })
            .collect()
    }

    /// Query all initialized ticks via paginated iteration.
    fn query_all_ticks(&self) -> Vec<TickResponse> {
        let vlp = self.vlp();
        let mut all_ticks = Vec::new();
        let mut start_after: Option<i64> = None;

        loop {
            let page: TicksResponse = vlp
                .query(&ConcentratedQueryMsg::Ticks {
                    start_after,
                    limit: Some(100),
                })
                .expect("ticks query should succeed");

            if page.ticks.is_empty() {
                break;
            }
            start_after = Some(page.ticks.last().expect("non-empty page").index);
            all_ticks.extend(page.ticks);
        }

        all_ticks
    }

    // =========================================================================
    // Composite queries
    // =========================================================================

    /// Query lightweight pool state for strategy decisions (current tick + reserves).
    /// Much cheaper than a full snapshot — no position/tick enumeration.
    pub(crate) fn pool_state(&self) -> PoolState {
        let slot0 = self.query_slot0();
        let (reserve_0, reserve_1) = self.query_reserves();
        PoolState {
            current_tick: slot0.tick,
            reserve_0,
            reserve_1,
        }
    }

    /// Capture lightweight snapshot: slot0 + reserves + protocol fees only.
    /// Skips position and tick enumeration (~3 queries vs hundreds).
    /// Used for cheap per-op invariant checks (C1, C7, C8, S1-S3).
    pub(crate) fn light_snapshot(&self) -> PoolSnapshot {
        let (reserve_0, reserve_1) = self.query_reserves();
        PoolSnapshot {
            slot0: self.query_slot0(),
            positions: Vec::new(),
            ticks: Vec::new(),
            protocol_fees: self.query_protocol_fees(),
            reserve_0,
            reserve_1,
        }
    }

    /// Capture full pool state: slot0, all positions, all ticks, reserves, and protocol fees.
    pub(crate) fn full_snapshot(&self) -> PoolSnapshot {
        let (reserve_0, reserve_1) = self.query_reserves();
        PoolSnapshot {
            slot0: self.query_slot0(),
            positions: self.query_tracked_positions(),
            ticks: self.query_all_ticks(),
            protocol_fees: self.query_protocol_fees(),
            reserve_0,
            reserve_1,
        }
    }
}
