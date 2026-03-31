use cw_orch::prelude::*;
use rand::rngs::StdRng;
use rand::Rng;

use crate::helpers::factory::{
    create_concentrated_pool, faucet, pair_with_amounts, setup_concentrated_env,
};
use crate::invariants::concentrated::{
    check_all_snapshot_invariants, check_light_snapshot, check_post_test,
};
use crate::invariants::shared::{check_shared_snapshot, check_shared_transition};
use crate::invariants::{InvariantResult, PoolSnapshot};
use crate::runner::FuzzPool;
use crate::strategies::concentrated::ConcentratedOp;

use super::config::ConcentratedConfig;
use super::ConcentratedPool;

impl FuzzPool for ConcentratedPool {
    type Op = ConcentratedOp;
    type Snapshot = PoolSnapshot;
    type Config = ConcentratedConfig;

    /// Deploy contracts, create a concentrated pool with the configured number of users.
    /// User 0 is the environment's default sender (pool creator).
    /// Users 1..N are created via `addr_make` and funded with tokens.
    fn setup(config: &ConcentratedConfig) -> Self {
        let num_users = config.num_users.max(1);
        let (interchain, mut factory, router, token_a, token_b) = setup_concentrated_env();

        let pair = pair_with_amounts(
            &token_a,
            &token_b,
            config.initial_amount_0,
            config.initial_amount_1,
        );
        let pool_key = create_concentrated_pool(
            &factory,
            &router,
            pair,
            config.fee_tier_bps,
            config.tick_spacing,
            100,
        )
        .expect("pool creation should succeed");

        let default_sender = factory.environment().sender.clone();
        let mut users = vec![default_sender.clone()];

        let chain = factory.environment();
        let mut scratch_funds = vec![];
        for i in 1..num_users {
            let user_addr = chain.addr_make(format!("fuzz_user_{i}"));
            for token_type in [token_a.token_type.clone(), token_b.token_type.clone()] {
                scratch_funds.clear();
                faucet(
                    chain,
                    user_addr.as_str(),
                    10_000_000,
                    token_type,
                    &mut scratch_funds,
                );
            }
            users.push(user_addr);
        }

        factory.set_sender(&default_sender);

        let mut pool = Self {
            interchain,
            factory,
            router,
            pool_key,
            token_a,
            token_b,
            positions: Vec::new(),
            users,
            config: config.clone(),
            last_remove_ticks: None,
        };

        pool.sync_positions();
        pool
    }

    /// Generate a random operation informed by live pool state and config weights.
    fn random_op(&self, rng: &mut StdRng) -> ConcentratedOp {
        let state = self.pool_state();
        if self.users.len() <= 1 {
            ConcentratedOp::random(rng, self.positions.len(), &self.config, &state)
        } else {
            let user_idx = rng.gen_range(0..self.users.len());
            let user_position_count = self.positions_for_user(user_idx).len();
            ConcentratedOp::random(rng, user_position_count, &self.config, &state)
                .with_user_idx(user_idx)
        }
    }

    /// Dispatch an operation to the appropriate handler method.
    fn execute_op(&mut self, op: &ConcentratedOp) -> Result<(), String> {
        let op = op.clone();
        self.exec_as_user(op.user_idx(), |pool| match &op {
            ConcentratedOp::Swap {
                amount,
                zero_for_one,
                ..
            } => pool.swap(*amount, *zero_for_one),
            ConcentratedOp::AddLiquidity {
                lower_tick,
                upper_tick,
                amount_0,
                amount_1,
                ..
            } => pool.add_liquidity(*lower_tick, *upper_tick, *amount_0, *amount_1),
            ConcentratedOp::RemoveLiquidity {
                position_idx,
                fraction_bps,
                user_idx: uid,
            } => match pool.resolve_user_position(*uid, *position_idx) {
                Some(idx) => pool.remove_liquidity(idx, *fraction_bps),
                None => Err("no positions for this user".to_string()),
            },
            ConcentratedOp::CollectFees {
                position_idx,
                user_idx: uid,
            } => match pool.resolve_user_position(*uid, *position_idx) {
                Some(idx) => pool.collect_fees(idx),
                None => Err("no positions for this user".to_string()),
            },
        })
    }

    fn snapshot(&self) -> PoolSnapshot {
        self.full_snapshot()
    }

    fn light_snapshot(&self) -> PoolSnapshot {
        ConcentratedPool::light_snapshot(self)
    }

    fn check_snapshot_invariants(&self, snapshot: &PoolSnapshot) -> InvariantResult {
        let mut result = check_shared_snapshot(snapshot);
        result.merge(check_all_snapshot_invariants(
            snapshot,
            self.config.tick_spacing,
        ));
        result
    }

    fn check_light_snapshot_invariants(&self, snapshot: &PoolSnapshot) -> InvariantResult {
        let mut result = check_shared_snapshot(snapshot);
        result.merge(check_light_snapshot(snapshot));
        result
    }

    fn check_transition_invariants(
        &self,
        before: &PoolSnapshot,
        after: &PoolSnapshot,
        op: &ConcentratedOp,
    ) -> InvariantResult {
        let mut result = check_shared_transition(before, after);
        let op_invariants_result = op.check_transition(before, after, self.last_remove_ticks);
        result.merge(op_invariants_result);
        result
    }

    fn check_post_test_invariants(&self, snapshot: &PoolSnapshot) -> InvariantResult {
        check_post_test(snapshot)
    }

    fn seed_liquidity(&mut self, rng: &mut StdRng, num_positions: usize) {
        self.seed_positions(rng, num_positions);
    }

    fn drain_all_liquidity(&mut self) {
        ConcentratedPool::drain_all_liquidity(self);
    }

    fn op_name(op: &ConcentratedOp) -> &'static str {
        op.name()
    }

    fn pool_name(&self) -> String {
        format!(
            "Concentrated(fee={}, spacing={})",
            self.config.fee_tier_bps, self.config.tick_spacing
        )
    }
}
