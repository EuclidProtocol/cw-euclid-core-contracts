# Issue 2 — Single-sided liquidity: partner fee support

**Type:** AFK
**Label:** ready-for-agent

## Parent

`PRD-single-sided-liquidity.md` at the repo root.

## What to build

Extend the single-sided liquidity entry point with an optional partner fee, mirroring the existing swap path's partner-fee model exactly. Integrators who build single-sided deposit UX can charge a basis-points fee on the deposit, deducted from the user's input before the swap leg, and routed to the integrator on success or fully refunded on failure.

The model — identical to `execute_swap_request` / `ack_swap_request`:

- The factory entry message gains `partner_fee: Option<PartnerFee> { partner_fee_bps, recipient }`.
- `partner_fee_amount = amount_in.checked_mul_ceil(Decimal::bps(partner_fee_bps))`.
- `amount_in -= partner_fee_amount` before the IBC packet is built. The fee portion is retained at the factory contract's address until the ack arrives.
- The IBC message gains `partner_fee_amount: Uint256` and `partner_fee_recipient: CrossChainUser`. These fields are accounting-only on the hub — the hub does not consume them, but they round-trip back into the ack so the factory can route on the failure path even after a long delay.
- `partner_fee_bps` is bounded by the existing `MAX_PARTNER_FEE_BPS`.
- `SingleSidedLiquidityRequest` in pending state gains `partner_fee_amount` and `partner_fee_recipient` so the factory's ack handler can route correctly.
- Factory ack success: in addition to escrowing the (now reduced) `amount_in`, transfers `partner_fee_amount` to `partner_fee_recipient` via `create_transfer_msg` when non-zero.
- Factory ack failure: refunds `amount_in + partner_fee_amount` to the user (full reversal).
- Native-chain ack failure path still returns `Err` as before — the transaction-level revert handles the refund.

## Acceptance criteria

- [ ] Factory `ExecuteMsg::AddSingleSidedLiquidity` accepts `partner_fee: Option<PartnerFee>`. Default `None` is equivalent to `partner_fee_amount = 0` and uses the user as the recipient (mirror the swap path's defaulting logic).
- [ ] Factory validates `partner_fee_bps <= MAX_PARTNER_FEE_BPS`; rejects with `InvalidPartnerFee {}` otherwise.
- [ ] Factory computes `partner_fee_amount`, subtracts it from `amount_in` before the IBC packet, and retains the fee at the factory.
- [ ] IBC message carries `partner_fee_amount` and `partner_fee_recipient`. `get_tx_id` / `get_sender` still work; struct schema additions are backwards-compatible.
- [ ] `SingleSidedLiquidityRequest` pending-state struct gains `partner_fee_amount` and `partner_fee_recipient`.
- [ ] Factory ack success path: when `partner_fee_amount > 0`, emits a transfer message to `partner_fee_recipient` for `partner_fee_amount`, in addition to the escrow message for the reduced `amount_in`. When `partner_fee_amount == 0`, only the escrow + LP mint messages are emitted.
- [ ] Factory ack failure path: refunds `amount_in + partner_fee_amount` to the user (Cosmos chain) or returns `Err` (native chain).
- [ ] Unit tests cover: zero partner fee (no partner transfer message emitted), non-zero partner fee on success (both escrow and transfer messages emitted with correct amounts), refund on failure includes the partner fee portion, `partner_fee_bps` cap rejection, refund correctness when `partner_fee_bps == 0` is supplied explicitly.

## Blocked by

- Issue 1 (single-sided native tracer bullet)
