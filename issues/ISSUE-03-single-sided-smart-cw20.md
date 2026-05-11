# Issue 3 — Single-sided liquidity: Smart (CW20) asset_in support

**Type:** AFK
**Label:** ready-for-agent

## Parent

`PRD-single-sided-liquidity.md` at the repo root.

## What to build

Extend the single-sided liquidity factory entry point to accept `TokenType::Smart` (CW20) as `asset_in`, using the **TransferFrom pattern** — the same model `add_liquidity_request` uses, where the user pre-approves the factory contract for the CW20 token and then calls the factory directly.

This slice is purely a factory-side concern. The hub-side router only ever sees the resolved `amount_in` (a `Uint256`) after the factory has pulled the tokens in, so no router changes are needed.

The model:

- User flow: (1) user calls `Cw20::IncreaseAllowance` to approve the factory for `amount_in + partner_fee_amount` of the CW20; (2) user calls `factory::ExecuteMsg::AddSingleSidedLiquidity`. Two transactions, matching the existing `add_liquidity_request` UX.
- Factory `execute_single_sided_add_liquidity_request` adds a Smart-token branch: emit a `create_transfer_msg(amount_in, env.contract.address, Some(sender.address), None)` submessage that pulls the full `amount_in` (including partner fee portion, before the partner-fee deduction) from the user into the factory. This is identical to `add_liquidity_request:297-304`.
- The Smart-token amount validation is interleaved with the Native branch: validation continues to enforce `amount_in > 0`, escrow exists, escrow `TokenAllowed` query passes, `tx_id` not pending.
- Factory ack success: the existing `create_escrow_msg` already handles Smart tokens by producing a CW20 `Transfer` to the escrow. No new branching needed in the ack — `asset_in.token_type.create_escrow_msg(amount_in, escrow_address)` handles both Native and Smart.
- Factory ack failure refund: `asset_in.create_transfer_msg(amount_in + partner_fee_amount, sender, ...)` handles both Native and Smart through `create_transfer_msg`.
- Voucher input remains `UnreachableCode`.

## Acceptance criteria

- [ ] Factory `execute_single_sided_add_liquidity_request` accepts `TokenType::Smart` and emits a `TransferFrom` submessage pulling the full `amount_in` from the user into the factory contract.
- [ ] Voucher input still rejects with `UnreachableCode`.
- [ ] All existing validations (`PAIR_TO_VLP` existence, escrow allowed, `swap_amount` bounds, etc.) apply identically to Smart input.
- [ ] Factory ack success with Smart input: tokens are escrowed correctly via the existing `create_escrow_msg`, partner fee routed correctly via `create_transfer_msg`, LP CW20 minted to user.
- [ ] Factory ack failure with Smart input: refund transfer (`amount_in + partner_fee_amount` if Issue 2 is merged, otherwise just `amount_in`) lands back in the user's CW20 balance.
- [ ] Unit tests cover: Smart token happy path with TransferFrom submessage shape verified, Smart token escrow on success, Smart token refund on failure, Voucher input rejected.

## Blocked by

- Issue 1 (single-sided native tracer bullet)
