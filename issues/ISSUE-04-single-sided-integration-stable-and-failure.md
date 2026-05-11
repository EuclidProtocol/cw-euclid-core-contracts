# Issue 4 — Single-sided liquidity: integration test coverage for stable pool and ack-failure refund

**Type:** AFK
**Label:** ready-for-agent

## Parent

`PRD-single-sided-liquidity.md` at the repo root.

## What to build

Extend `tests-integration/` with two end-to-end interchain scenarios that go beyond what unit tests can demonstrate, both using `cw-orch` and `cw-orch-interchain` against a real multi-contract, multi-chain setup.

**Scenario A — Stable pool happy path.** Same flow as the constant-product happy path covered by Issue 1's integration test, but against a stable VLP (e.g. USDC/USDT with an amp factor). Demonstrates that the orchestrator is genuinely pool-type agnostic — no branching on pool type, no Stable-only edge cases. The user submits native USDC, the hub atomically swaps a portion via the stable curve and adds liquidity on the same VLP, the LP CW20 lands on the remote chain, and the escrow holds the deposit.

**Scenario B — Ack-failure refund.** A native deposit against a constant-product VLP, but with `min_lp_out` deliberately set above the simulated LP output (e.g. simulated output is N, user sets `min_lp_out = 2 * N`). The full flow should execute through both reply handlers on the hub, the add-liquidity reply should return `Err(ContractError::SlippageExceeded)`, the outer cross-chain-receive reply should convert that to an error ack via `make_ack_fail`, and the factory's ack handler should refund the user. Verify that:

- The error ack is emitted and received by the factory.
- Funds land back in the user's account on the remote chain.
- No residual state remains on either chain: `PENDING_SINGLE_SIDED_LIQUIDITY` is empty on both, `VLP_TO_LP_SHARES` is unchanged (no shares minted), the VLP's reserves are unchanged from before the deposit attempt.

If Issues 2 (partner fee) and 3 (Smart asset_in) are merged before this issue lands, the failure-refund scenario should also verify partner-fee refund (`amount_in + partner_fee_amount` returned to user, partner address balance unchanged) and Smart-token refund.

## Acceptance criteria

- [ ] Stable pool integration scenario: deposits native into a USDC/USDT (or equivalent) stable VLP, LP CW20 minted on remote chain, escrow holds the deposit, no test-only branching in production code.
- [ ] Failure-refund integration scenario: contrived `min_lp_out` triggers the error-ack path; refund lands at the user; no residual state on either chain (asserted explicitly via state queries after the test).
- [ ] If Issue 2 has merged: failure scenario also asserts the partner fee portion is refunded to the user (not retained at the factory).
- [ ] If Issue 3 has merged: at least one scenario uses a Smart (CW20) asset_in, demonstrating TransferFrom + escrow on success and TransferFrom-reversal on failure.
- [ ] `cargo test -p tests-integration` passes for both scenarios.

## Blocked by

- Issue 1 (single-sided native tracer bullet)
- (Coverage strengthened if Issues 2 and 3 are merged first, but not blocked on them)
