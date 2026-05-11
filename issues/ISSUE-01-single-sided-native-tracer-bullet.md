# Issue 1 — Single-sided liquidity: native tracer bullet (single-hop, no partner fee, no Smart tokens)

**Type:** AFK
**Label:** ready-for-agent

## Parent

`PRD-single-sided-liquidity.md` at the repo root.

## What to build

End-to-end single-sided liquidity for the dominant case: a user on a Cosmos remote chain submits a single transaction depositing a native token, and the hub atomically swaps a backend-computed portion of that token through the target VLP and adds the resulting balanced pair as liquidity to the same VLP — all in one IBC roundtrip.

The user provides: `asset_in` (Native token), `amount_in`, `asset_out`, `swap_amount`, `swap_route: Vec<NextSwapPair>` (must be length 1), `min_lp_out` (sole slippage guard), and standard `cross_chain_config`. The factory validates inputs, escrows the funds, sends the IBC packet, and (on success ack) mints LP CW20 tokens to the user. On any failure, the full deposit is refunded and no residual state persists on either chain.

Concretely, this slice lands:

- New `RouterCrossChainExecuteMsg::SingleSidedAddLiquidity(RouterCrossChainSingleSidedAddLiquidityMsg)` IBC variant in `packages/euclid_ibc`. The struct carries `sender`, `asset_in`, `amount_in`, `swap_amount`, `asset_out`, `swap_route`, `min_lp_out`, `tx_id`. `get_tx_id()` and `get_sender()` arms updated.
- New `SingleSidedLiquidityRequest { sender, tx_id, asset_in, amount_in }` struct in `packages/euclid`.
- New `TxType::SingleSidedAddLiquidity` variant in the shared events package, used in factory entry events, router IBC events, and ack-success events.
- Factory `ExecuteMsg::AddSingleSidedLiquidity { asset_in, amount_in, asset_out, swap_amount, swap_route, min_lp_out, cross_chain_config }` variant + dispatch.
- Factory state map `PENDING_SINGLE_SIDED_LIQUIDITY: Map<(Addr, String), SingleSidedLiquidityRequest>`.
- Factory `execute_single_sided_add_liquidity_request`: validates `amount_in > 0`, `swap_amount > 0`, `swap_amount < amount_in`, `asset_in.token != asset_out`, `PAIR_TO_VLP.has((asset_in.token, asset_out))`, escrow exists for `asset_in`, escrow's `TokenAllowed` query passes, `tx_id` not pending. Native funds via `fund_manager.use_fund`. Voucher rejected as `UnreachableCode`. Smart token rejected (out of scope for this slice — covered in Issue 3).
- Factory `ack_single_sided_add_liquidity` dispatched by `reusable_internal_ack_call`. Success: increments `VLP_TO_LP_SHARES`, escrows `amount_in` of `asset_in` via `create_escrow_msg`, mints LP CW20 to the user. Failure: returns `Err` if `is_native`; otherwise refunds `amount_in` via `create_transfer_msg`.
- Router state map `PENDING_SINGLE_SIDED_LIQUIDITY: Map<String, RouterCrossChainSingleSidedAddLiquidityMsg>` keyed by `tx_id`.
- Router reply ID constants: `SINGLE_SIDED_SWAP_REPLY_ID = 9`, `SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID = 10`. Both wired into `contract.rs::reply` dispatch.
- Router `ibc_execute_single_sided_add_liquidity`: validates `swap_route.len() == 1`, `swap_route[0].token_in == asset_in.token`, `swap_route[0].token_out == asset_out`, `swap_amount > 0 && < amount_in`, `tx_id` not pending; loads target VLP via `VLPS.may_load((asset_in.token, asset_out))?.ok_or(PoolDoesNotExist)?`; saves pending; queries `asset_in` metadata on sender chain; normalizes `swap_amount`; mints raw `amount_in` to user's virtual_balance (which normalizes internally); approves VLP for `normalized_swap_amount`; emits `SubMsg::reply_always(VlpSwap, SINGLE_SIDED_SWAP_REPLY_ID)`.
- Router `on_single_sided_swap_reply`: on `SubMsgResult::Err` returns `ContractError::Reply { ... }`. On `Ok`: parses `VlpSwapResponse`, loads pending state, computes `remaining_normalized = normalize(amount_in - swap_amount, decimals)`. **Does NOT mint `asset_out`** — the VLP swap's terminal-hop transfer already deposited it into the user's virtual balance. Approves target VLP for `remaining_normalized` of `asset_in` and `amount_out` of `asset_out`. Emits `SubMsg::reply_always(VlpAddLiquidity, SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID)` with `slippage_tolerance_bps = BPS_50_PERCENT`.
- Router `on_single_sided_add_liquidity_reply`: on `SubMsgResult::Err` returns `ContractError::Reply { ... }`. On `Ok`: parses `AddLiquidityResponse`, loads pending state, validates `mint_lp_tokens >= min_lp_out` via `ensure!` returning `Err(ContractError::SlippageExceeded)` on failure (never via manually-constructed ack_fail), removes pending, sets `data` to `AcknowledgementMsg::Ok(AddLiquidityResponse)`.

**Inner `VlpSwapMsg` construction (from prototype during grilling):**

```rust
VlpSwapMsg {
    sender,
    tx_id,
    asset_in: asset_in.token,
    amount_in: normalized_swap_amount,
    min_token_out: Uint256::zero(),       // min_lp_out is the real guard
    next_swaps: vec![],                   // single-hop
    test_fail: swap_route[0].test_fail,
}
```

**Atomicity invariant:** any `Err` returned from any reply handler propagates to the existing outer `on_cross_chain_receive_reply`, which uses `reply_always` and emits `make_ack_fail`. All hub mutations — pending-state save, virtual-balance mint, approves, swap effects, add-liquidity effects — roll back. No reply handler may call `set_data(make_ack_fail(...))` directly, because that would commit the transaction and leave residual state.

## Acceptance criteria

- [ ] New IBC variant added with `get_tx_id` / `get_sender` arms; `RouterCrossChainSingleSidedAddLiquidityMsg` struct defined.
- [ ] `SingleSidedLiquidityRequest` struct added in `packages/euclid`.
- [ ] `TxType::SingleSidedAddLiquidity` variant added and used in factory entry event, router IBC event, and ack-success event.
- [ ] Factory `ExecuteMsg::AddSingleSidedLiquidity` accepts native deposits, validates all inputs from the PRD validation surface, saves pending, sends IBC submsg. Smart asset_in returns an explicit "not supported in this version" error; Voucher returns `UnreachableCode`.
- [ ] Factory ack success: escrows `amount_in`, increments `VLP_TO_LP_SHARES`, mints LP CW20 to user, clears pending.
- [ ] Factory ack failure: refunds `amount_in` to user (Cosmos chain) or returns `Err` (native chain), clears pending.
- [ ] Router IBC receive handler enforces single-hop (`swap_route.len() == 1`), endpoint match, `swap_amount` bounds, pool existence, duplicate `tx_id` rejection.
- [ ] Router swap-reply does NOT emit an `asset_out` mint message.
- [ ] Router add-liquidity reply enforces `mint_lp_tokens >= min_lp_out` and returns `Err` on slippage failure.
- [ ] All four error edges (initial validation, swap submsg fail, swap reply post-processing, add-liquidity reply / slippage) propagate as `Err` — verified by absence of any `set_data(make_ack_fail(...))` call inside the new reply handlers.
- [ ] Unit tests on the router orchestrator cover: happy path, route length != 1, route endpoint mismatch, `swap_amount` bounds violations, missing VLP, duplicate `tx_id`, slippage exceeded (returns `Err`), swap submsg `Err` propagates, add-liquidity submsg `Err` propagates, and the negative assertion that no `asset_out` mint is emitted.
- [ ] Unit tests on the factory cover: happy path with native funds, `PAIR_TO_VLP` miss, `asset_in.token == asset_out`, `swap_amount == 0`, `swap_amount >= amount_in`, escrow not allowed, duplicate `tx_id`, Native funds mismatch, ack success state transitions, ack failure refund, ack `is_native == true` returns `Err`.
- [ ] An integration test in `tests-integration/` exercises the full IBC roundtrip against a constant-product VLP: deposit native USDC, receive LP CW20 on the remote chain, escrow holds the deposit.
- [ ] `cargo clippy --all-targets --all-features` and `cargo fmt --check` pass.
- [ ] `cargo test -p factory` and `cargo test -p router` pass, including all new tests.

## Blocked by

None — can start immediately.
