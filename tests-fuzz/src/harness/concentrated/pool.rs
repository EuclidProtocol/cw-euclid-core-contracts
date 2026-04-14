use cosmwasm_std::Addr;
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use cw_orch_interchain::mock::MockInterchainEnv;
use euclid::msgs::vlp::base::PoolKey;
use euclid::token::TokenWithDenom;

use factory::FactoryContract;
use router::RouterContract;

use super::config::{ConcentratedConfig, TrackedPosition};

/// The main fuzz harness for concentrated liquidity testing.
///
/// Supports multiple users: `users[0]` is the default sender (created during
/// environment setup). Additional users are created with `addr_make` and funded
/// with both tokens. Operations specify which user acts via `user_idx`.
pub struct ConcentratedPool {
    /// Ownership anchor — keeps the mock interchain environment alive
    #[allow(dead_code)]
    pub(crate) interchain: MockInterchainEnv,
    pub(crate) factory: FactoryContract<MockBase>,
    pub(crate) router: RouterContract<MockBase>,
    pub(crate) pool_key: PoolKey,
    pub(crate) token_a: TokenWithDenom,
    pub(crate) token_b: TokenWithDenom,
    pub(crate) positions: Vec<TrackedPosition>,
    pub(crate) users: Vec<Addr>,
    pub(crate) config: ConcentratedConfig,
    /// Cached tick bounds from the most recent `remove_liquidity` call.
    /// Needed by T5 (burn liquidity invariant) since the position may be
    /// removed from `self.positions` before transition checks run.
    pub(crate) last_remove_ticks: Option<(i64, i64)>,
}

impl ConcentratedPool {
    /// Get the address for user at `idx`.
    pub fn user_addr(&self, idx: usize) -> &Addr {
        &self.users[idx % self.users.len()]
    }

    /// Execute a closure as a specific user, restoring the previous sender afterward.
    pub fn exec_as_user<T>(&mut self, user_idx: usize, f: impl FnOnce(&mut Self) -> T) -> T {
        let user = self.user_addr(user_idx).clone();
        self.exec_as_sender(&user, f)
    }

    /// Execute a closure as a specific address, restoring the previous sender afterward.
    pub fn exec_as_sender<T>(&mut self, sender: &Addr, f: impl FnOnce(&mut Self) -> T) -> T {
        let prev = self.factory.environment().sender.clone();
        self.factory.set_sender(sender);
        let result = f(self);
        self.factory.set_sender(&prev);
        result
    }

    /// Clone the pool key (convenience for passing to APIs that take ownership).
    pub(crate) fn pool_key(&self) -> PoolKey {
        self.pool_key.clone()
    }

    /// Get the address of the owner of a tracked position.
    pub(super) fn position_owner(&self, idx: usize) -> Addr {
        Addr::unchecked(&self.positions[idx].owner)
    }

    /// Get global indices of positions owned by a specific user.
    pub fn positions_for_user(&self, user_idx: usize) -> Vec<usize> {
        let user_addr = self.user_addr(user_idx).to_string();
        self.positions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.owner == user_addr)
            .map(|(i, _)| i)
            .collect()
    }

    /// Resolve a user-relative position index to a global index.
    pub(super) fn resolve_user_position(
        &self,
        user_idx: usize,
        user_relative_idx: usize,
    ) -> Option<usize> {
        let user_addr = self.user_addr(user_idx).to_string();
        self.positions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.owner == user_addr)
            .map(|(i, _)| i)
            .nth(user_relative_idx)
    }
}
