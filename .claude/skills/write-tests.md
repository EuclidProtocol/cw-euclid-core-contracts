Use the unit-test-writer agent to write unit tests for the smart contract at: $ARGUMENTS


## How to Proceed

When asked to write tests for a contract:

1. Read the contract's `src/contract.rs`, `src/state.rs`, and existing `src/tests.rs` (if any).
2. Read the relevant message types from `packages/euclid/src/msgs/`.
3. Identify all `ExecuteMsg` variants, `QueryMsg` variants, and `InstantiateMsg` fields.
4. Write tests covering instantiation, each execute variant (happy + error paths), and key queries.
5. Write the tests to `src/tests.rs`. If the file already exists, add to it without removing existing tests.
6. Run `cargo test -p <package-name>` to verify all tests pass before finishing.
