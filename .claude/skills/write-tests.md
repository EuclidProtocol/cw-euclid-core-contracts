Use the unit-test-writer agent to write unit tests for the smart contract at: $ARGUMENTS


## How to Proceed

When asked to write tests for a contract:

1. Read the contract's `src/contract.rs`, `src/state.rs`, and `src/tests.rs` (if it exists). **Read `src/tests.rs` first** to catalogue which execute variants, query variants, and error paths are already covered — do not write duplicate tests for coverage that already exists.
2. Read the relevant message types from `packages/euclid/src/msgs/`.
3. Identify all `ExecuteMsg` variants, `QueryMsg` variants, and `InstantiateMsg` fields that lack coverage.
4. Write tests covering instantiation, each uncovered execute variant (happy + error paths), and key queries.
5. Write each test in the same file where the function under test is defined. For example, tests for a query function in `src/query.rs` go at the bottom of `src/query.rs` inside a `#[cfg(test)]` module; tests for execute handlers in `src/contract.rs` go at the bottom of `src/contract.rs`, etc. If a `#[cfg(test)]` module already exists in that file, append to it without removing or modifying existing tests.
   - **Fixtures and shared helpers** (type aliases, constants, `#[fixture]` functions, seed helpers) must live in dedicated files under `src/testing/`, not inline in a test module:
     - `src/testing/fixtures.rs` — rstest `#[fixture]` functions that return pre-seeded `MockDeps`
     - `src/testing/helpers.rs` — type aliases (`MockDeps`), address constants, `init()`, and seed/builder helpers
     - `src/testing/mod.rs` — declares both modules:
       ```rust
       pub mod fixtures;
       #[cfg(test)]
       pub mod helpers;
       ```
   - If `src/testing/` does not exist yet, create all three files. Add `#[cfg(test)] mod testing;` to `src/lib.rs`.
   - `helpers.rs` must **not** wrap its contents in an inner `mod` block — items live at file-top-level so the import path stays flat: `use crate::testing::helpers::{MockDeps, init, ...}` and `use crate::testing::fixtures::initialized`.
6. Run `cargo clippy -p <package-name> -- -D warnings` and fix every warning before proceeding.
7. Run `cargo test -p <package-name>` and verify all tests pass. Fix any failures before finishing.
