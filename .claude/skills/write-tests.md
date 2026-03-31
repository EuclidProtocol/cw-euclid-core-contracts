Use the unit-test-writer agent to write unit tests for the smart contract at: $ARGUMENTS


## How to Proceed

When asked to write tests for a contract:

1. Read the contract's `src/contract.rs`, `src/state.rs`, and `src/tests.rs` (if it exists). **Read `src/tests.rs` first** to catalogue which execute variants, query variants, and error paths are already covered — do not write duplicate tests for coverage that already exists.
2. Read the relevant message types from `packages/euclid/src/msgs/`.
3. Identify all `ExecuteMsg` variants, `QueryMsg` variants, and `InstantiateMsg` fields that lack coverage.
4. Write tests covering instantiation, each uncovered execute variant (happy + error paths), and key queries.
5. Write the tests to `src/tests.rs`. If the file already exists, append to it without removing or modifying existing tests.
6. Run `cargo clippy -p <package-name> -- -D warnings` and fix every warning before proceeding.
7. Run `cargo test -p <package-name>` and verify all tests pass. Fix any failures before finishing.
