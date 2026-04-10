#[cfg(test)]
pub mod fixtures {
    use crate::testing::helpers::{
        deps_with_state, remote_user, seed_allowance, seed_balance, vsl_user, MockDeps,
    };

    /// A `MockDeps` with three user balances seeded:
    /// - vsl "alice"  → token "eucl" : 1000
    /// - vsl "bob"    → token "eucl" : 500
    /// - chain1 "eve" → token "usdc" : 200
    #[allow(dead_code)]
    pub fn deps_with_balances() -> MockDeps {
        let mut deps = deps_with_state();
        seed_balance(&mut deps, vsl_user("alice"), "eucl", 1000);
        seed_balance(&mut deps, vsl_user("bob"), "eucl", 500);
        seed_balance(&mut deps, remote_user("1", "eve"), "usdc", 200);
        deps
    }

    /// A `MockDeps` with balances AND a pending allowance:
    /// alice → allows bob to spend 300 eucl.
    #[allow(dead_code)]
    pub fn deps_with_allowance() -> MockDeps {
        let mut deps = deps_with_balances();
        seed_allowance(
            &mut deps,
            vsl_user("alice"),
            "eucl",
            vsl_user("bob"),
            300,
        );
        deps
    }
}
