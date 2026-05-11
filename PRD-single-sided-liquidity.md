# PRD: Single-Sided Liquidity via Batch Router

**Status:** ready-for-agent
**Area:** factory, router, euclid, euclid_ibc

---

## Problem Statement

Today, a user who wants to provide liquidity to a Euclid VLP must bring both tokens of the pair in roughly the right ratio. Users who hold only one of the two tokens have to perform two separate cross-chain operations: first an IBC swap from token A to token B (paying gas + partner fees + waiting for IBC ack), then a second IBC add-liquidity submission with both tokens (paying gas again + waiting for a second IBC ack). This is slow, expensive, error-prone, and pushes ratio-management work onto users or onto frontends that have to chain two distinct flows.

The net effect is that single-asset holders are under-represented as LPs, frontends ship complex two-step deposit UX, and partner integrators lose fee revenue compared to a clean single-action deposit.

## Solution

Add a new factory entry point — `AddSingleSidedLiquidity` — that accepts one token from the user and, in a single IBC roundtrip, performs the swap-then-deposit sequence atomically on the hub. The user provides the deposit token, the target pool's other token, a backend-computed split telling the router how much to swap, and a minimum-LP slippage guard. The router mints virtual balance, executes the swap on the target VLP, chains directly into add-liquidity on the same VLP via a submessage reply, and validates the LP-token output against the user's minimum. On success, the factory escrows the deposit token, routes any partner fee, and mints LP tokens to the user. On any failure along the chain, all hub state rolls back and the factory refunds the user fully.

No new contract is introduced — all logic lives in the existing factory and router. The flow uses CosmWasm's SubMsg + reply pattern to chain swap → add-liquidity atomically, which is only possible because both operations are hub-side VLP calls.

## User Stories

1. As a single-asset holder on a remote chain, I want to deposit only one token of a pair and receive LP tokens, so that I can become an LP without having to perform a separate swap first.

2. As a user who only holds USDC, I want to deposit USDC into the USDC/ETH VLP and receive LP tokens, so that I can earn fees on the pair without sourcing ETH separately.

3. As a user, I want the single-sided deposit to complete in one IBC roundtrip rather than two, so that I pay less gas and wait less time.

4. As a user, I want to provide a minimum LP-token output, so that I'm protected from receiving fewer LP tokens than expected if pool reserves shift between off-chain computation and on-chain execution.

5. As a user, I want my deposit to fully refund if any step of the swap-then-add sequence fails, so that I am never partially in a position I didn't intend.

6. As a user, I want my deposit's swap leg to be priced against the same pool I'm depositing into, so that the resulting ratio matches the pool's post-swap ratio exactly and I don't unintentionally donate value via ratio imbalance.

7. As a user, I want the single-sided deposit to support both CosmWasm-native and CW20 (Smart) input tokens, so that I'm not forced to convert between token types before depositing.

8. As a user on a Cosmos IBC remote chain, I want the operation to follow the standard timeout/refund flow, so that a stuck packet returns my funds via the same mechanism as other operations.

9. As a user on a native chain (NativeReceiveCallback path), I want a failure to revert the entire transaction so my funds are not held in flight, so that the failure mode matches my chain's atomicity guarantees.

10. As an LP, I want the LP CW20 tokens to be minted to my originating-chain address, so that they appear in my wallet on the chain I initiated from.

11. As an LP, I want the swap fee (LP fee + Euclid fee) to be charged transparently on the internal swap leg, so that the fee accounting is consistent with a manual swap-then-deposit.

12. As a partner integrator (frontend or aggregator), I want to charge a basis-points fee on single-sided deposits, so that I can monetize the UX I build around this feature.

13. As a partner integrator, I want the partner fee to be deducted from the user's input amount before the swap-then-add calculation, so that the fee model matches the existing swap path and is easy to reason about.

14. As a partner integrator, I want the partner fee to fully refund to the user if the deposit fails, so that the user is not penalized for an outcome they didn't choose.

15. As a backend operator, I want to compute the optimal swap amount off-chain using pool reserve queries, so that the on-chain path remains simple and the rebalancing math can iterate without contract migrations.

16. As a backend operator, I want the optimal-swap-amount computation to account for the protocol swap fees (LP + Euclid), so that the resulting deposit ratio matches the pool exactly.

17. As a frontend developer, I want a single ExecuteMsg variant on the factory rather than a two-step orchestration, so that I can ship a single-transaction deposit UX.

18. As an analytics/indexer consumer, I want single-sided deposits to emit a distinct event type, so that I can bucket them separately from two-sided deposits in TVL, volume, and fee reports.

19. As a protocol operator, I want stable VLPs and constant-product VLPs to be supported through the same code path, so that I don't have to maintain pool-type-specific branching.

20. As a protocol operator, I want no new contracts deployed for this feature, so that there's nothing extra to migrate or audit.

21. As a protocol operator, I want failed single-sided deposits to leave no residual hub state, so that pending-state cleanup, fee accounting, and virtual-balance balances stay correct.

22. As a security reviewer, I want all validation to be re-checked on the router side of the IBC boundary, so that a compromised or buggy factory cannot corrupt hub state.

23. As a security reviewer, I want the slippage guard to be enforced via `Err` propagation rather than manual ack-fail construction, so that hub state is guaranteed to roll back atomically on slippage failure.

24. As a security reviewer, I want the partner fee to be capped at the existing `MAX_PARTNER_FEE_BPS`, so that there is no fee-escalation surface specific to this feature.

25. As a user, I want the factory to reject the deposit early if the target pool doesn't exist on the hub, so that I don't pay for an IBC roundtrip that will only return a failure ack.

26. As a user, I want the factory to reject the deposit if the swap amount is zero or equal to/greater than my deposit amount, so that obviously-malformed inputs fail fast with a clear error.

27. As a user, I want the same `tx_id` to never be reusable, so that replay attempts are rejected at both the factory and router boundaries.

28. As a user, I want my deposit's `asset_in` to be validated against the escrow's allowed denominations on my chain, so that I cannot accidentally deposit a denom the pool can't accept.

29. As a user submitting a CW20 (Smart) token, I want the factory to pull tokens from me using the TransferFrom pattern, so that the auth model matches the existing add-liquidity request and `info.sender` is unambiguously my address.

30. As a user, I want the same `min_lp_out` slippage guard to cover both the swap leg and the add-liquidity leg, so that I don't have to reason about two separate slippage parameters.

31. As a developer maintaining the feature, I want the IBC message to carry `swap_route: Vec<NextSwapPair>` even though v1 only accepts single-hop, so that multi-hop support can be added later without an IBC schema migration.

## Implementation Decisions

### Architecture & control flow

- **No new contract is introduced.** The swap-then-add sequence is implemented atomically on the hub via CosmWasm `SubMsg::reply_always` chaining inside the router. A zapper contract was considered and rejected because both legs are hub-side VLP calls — chaining them via reply IDs is atomic, requires no extra contract, and needs only one IBC packet.
- **Single-hop only in v1.** The swap route must have length 1, with `token_in == asset_in.token` and `token_out == asset_out`. The VLP used for the swap is the same VLP used for add-liquidity (`VLPS[(asset_in.token, asset_out)]`). This guarantees the post-swap ratio of (remaining_in : amount_out) lands on the pool's new ratio exactly, eliminating donation risk. The `swap_route: Vec<NextSwapPair>` field is kept in the IBC message type for forward compatibility with future multi-hop support.
- **Stable and CP VLPs are supported through the same code path.** Both VLP types implement the same `Swap` and `AddLiquidity` execute message contract; the internal math differs but the on-chain orchestration does not branch on pool type.

### API contract — factory

The factory exposes a new `ExecuteMsg::AddSingleSidedLiquidity` variant. From a prototype during design grilling, the field shape is:

```rust
AddSingleSidedLiquidity {
    asset_in: TokenWithDenom,           // Native or Smart only
    amount_in: Uint256,                 // total deposit BEFORE partner-fee deduction
    asset_out: Token,
    swap_amount: Uint256,               // backend-computed; must be < amount_in (post-fee) and > 0
    swap_route: Vec<NextSwapPair>,      // v1: length-1; kept Vec for forward-compat
    min_lp_out: Uint256,                // sole user-facing slippage guard
    partner_fee: Option<PartnerFee>,    // mirrors swap path: bps + recipient
    cross_chain_config: CrossChainConfig,
}
```

- **Single slippage parameter for users: `min_lp_out`.** The inner swap leg is configured with `min_token_out = Uint256::zero()` and the inner add-liquidity leg with `slippage_tolerance_bps = BPS_50_PERCENT` (the protocol-enforced maximum); both are hardcoded internally. Exposing a separate swap-leg minimum would be redundant since `min_lp_out` is the end-to-end check, and the same-pool swap-then-add guarantees the ratio check passes by construction.
- **Voucher input is rejected** as `UnreachableCode` — vouchers never originate from a remote-chain factory.
- **Smart-token input uses the TransferFrom pattern**, mirroring `add_liquidity_request` rather than the cw20 `Send` pattern used by `execute_swap_request`. Single-sided is structurally an add-liquidity variant: LP tokens mint to `info.sender`, so `info.sender` must be the user's address directly.

### Partner-fee model

Exact mirror of the existing swap path:

- API takes `Option<PartnerFee> { partner_fee_bps, recipient }` (bps form, not raw `Uint256`).
- `partner_fee_amount = amount_in.checked_mul_ceil(Decimal::bps(partner_fee_bps))`.
- `amount_in -= partner_fee_amount` before the IBC packet is constructed; the fee portion is retained at the factory.
- On success ack: partner fee is transferred from the factory to `partner_fee_recipient` and the reduced `amount_in` is sent to escrow.
- On failure ack: `amount_in + partner_fee_amount` is refunded to the user (full reversal).
- `partner_fee_bps` is bounded by the existing `MAX_PARTNER_FEE_BPS`.

### IBC message

New `RouterCrossChainExecuteMsg::SingleSidedAddLiquidity(RouterCrossChainSingleSidedAddLiquidityMsg)` variant. The struct carries:

```rust
pub struct RouterCrossChainSingleSidedAddLiquidityMsg {
    pub sender: CrossChainUser,
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint256,            // post-partner-fee
    pub swap_amount: Uint256,
    pub asset_out: Token,
    pub swap_route: Vec<NextSwapPair>,
    pub min_lp_out: Uint256,
    pub partner_fee_amount: Uint256,   // ack-accounting only; hub does not consume
    pub partner_fee_recipient: CrossChainUser,
    pub tx_id: String,
}
```

`get_tx_id()` and `get_sender()` arms must be extended to cover the new variant.

### Router orchestrator (deep module)

A single IBC entry point and two reply handlers form the orchestrator:

- **`ibc_execute_single_sided_add_liquidity`**: validates the message (single-hop route, route endpoints match `asset_in`/`asset_out`, `swap_amount` invariants, target VLP exists, no duplicate `tx_id`); saves the full message to `PENDING_SINGLE_SIDED_LIQUIDITY[tx_id]`; mints raw `amount_in` to the user's virtual_balance (which internally normalizes to 24-decimal voucher units); approves the target VLP to spend `normalized_swap_amount` of `asset_in`; issues a `SubMsg::reply_always(VlpSwap, SINGLE_SIDED_SWAP_REPLY_ID)`. No upfront `SimulateSwap` query — the swap-leg minimum is zero, so there is nothing to pre-check.

- **`on_single_sided_swap_reply`** (reply ID 9): on `SubMsgResult::Err`, returns `ContractError::Reply { ... }` and lets the outer cross-chain-receive reply convert it to an ack_fail. On `Ok`: parses `VlpSwapResponse`, loads pending state, computes `remaining_normalized = normalize(amount_in - swap_amount, decimals)`, **does NOT mint `asset_out`** (the VLP swap's terminal-hop transfer already deposited it into the user's virtual balance), issues approvals for both `asset_in` (remaining) and `asset_out` (swap output) from the user to the target VLP, then `SubMsg::reply_always(VlpAddLiquidity, SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID)`.

- **`on_single_sided_add_liquidity_reply`** (reply ID 10): on `SubMsgResult::Err`, returns `ContractError::Reply { ... }`. On `Ok`: parses `AddLiquidityResponse`, loads pending state to retrieve `min_lp_out`, validates `mint_lp_tokens >= min_lp_out` via `ensure!` (returning `Err` on failure — never via manually-constructed ack data), removes the pending entry, and sets the ack data to `AcknowledgementMsg::Ok(AddLiquidityResponse { ... })`.

**Error-propagation invariant:** all failures inside the orchestrator propagate as `Err` and are caught by the existing outer `on_cross_chain_receive_reply` (which uses `reply_always` and emits `make_ack_fail`). State changes — pending-state save, virtual-balance mint, approves, swap output — all roll back atomically. No reply handler may call `set_data(make_ack_fail(...))` directly, because that would commit the transaction successfully and leave residual hub state.

### Factory handler (deep module)

- **`execute_single_sided_add_liquidity_request`**: validates `partner_fee_bps ≤ MAX_PARTNER_FEE_BPS`, deducts the partner fee from `amount_in`, validates `amount_in > 0` / `swap_amount > 0` / `swap_amount < amount_in` / `asset_in.token != asset_out` / target pool exists in `PAIR_TO_VLP` / escrow exists for `asset_in` / escrow's `TokenAllowed` query passes / `tx_id` not pending. Handles funds: Native via `fund_manager.use_fund`, Smart via `create_transfer_msg` (TransferFrom), Voucher rejected as `UnreachableCode`. Saves `PENDING_SINGLE_SIDED_LIQUIDITY[(sender_addr, tx_id)] = SingleSidedLiquidityRequest { sender, tx_id, asset_in, amount_in (post-fee), partner_fee_amount, partner_fee_recipient }`. Emits a tx event using a new `TxType::SingleSidedAddLiquidity` discriminator. Sends the IBC submessage.

- **`ack_single_sided_add_liquidity`**: dispatched by `reusable_internal_ack_call` on the new variant. On success: increments `VLP_TO_LP_SHARES` by `mint_lp_tokens`, sends post-fee `amount_in` of `asset_in` to its escrow via `create_escrow_msg`, transfers `partner_fee_amount` to `partner_fee_recipient` (when non-zero) via `create_transfer_msg`, mints LP CW20 to the user. On failure: returns `Err` if `is_native`; otherwise refunds `amount_in + partner_fee_amount` to the user.

### State additions

- Factory: `PENDING_SINGLE_SIDED_LIQUIDITY: Map<(Addr, String), SingleSidedLiquidityRequest>` keyed by `(sender, tx_id)`.
- Router: `PENDING_SINGLE_SIDED_LIQUIDITY: Map<String, RouterCrossChainSingleSidedAddLiquidityMsg>` keyed by `tx_id`.
- Both maps are written on entry and removed on either success ack (router: success reply; factory: success ack) or implicit rollback (failure causes the router save to revert; the factory ack handler removes its entry).

### Reply IDs

Two new constants on the router: `SINGLE_SIDED_SWAP_REPLY_ID = 9` and `SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID = 10`. Existing IDs 1–8 are taken. Wire both into the router's `reply` match.

### Event taxonomy

A new `TxType::SingleSidedAddLiquidity` variant is added to the shared events package. The factory entry event, router IBC entry event, and the final ack-success path all tag events with this discriminator so the backend indexer can distinguish single-sided deposits from regular two-sided deposits for analytics, partner-fee reporting, and TVL bucketing.

### Decimal handling

The router normalizes `swap_amount` from raw to 24-decimal voucher units using the metadata's decimals (looked up via `query_token_metadata_by_denom` for `asset_in` on the sender's chain). No explicit decimals-mismatch check between submitted `token_type` and metadata — mirrors `ibc_execute_add_liquidity`'s behavior. The user's virtual-balance balance after the mint is in normalized units; `remaining_normalized = normalize(amount_in - swap_amount, decimals)` matches the user's residual balance exactly by linearity of normalization. The `amount_out` returned by the VLP swap is already in 24-decimal units.

### Off-chain backend responsibilities (documented for backend team, not implemented on-chain)

- The backend computes the optimal `swap_amount` using pool reserve queries.
- The computation must account for `euclid_fee_bps + lp_fee_bps` carved out of the swap leg, since the pool sees `swap_amount - total_fees` going into reserve.
- The `min_lp_out` value should be set with reasonable slippage tolerance against the simulated optimal output, since pool reserves can shift between off-chain simulation and on-chain execution.

## Testing Decisions

Tests must validate **observable external behavior** — pending-state writes/clears, virtual-balance mint/approve/transfer messages, escrow vs refund routing, ack contents — never the internal sequencing of helper calls or attribute-ordering. The two deep modules (the router orchestrator and the factory request/ack pair) are the testing focus; the shallow wiring modules are covered transitively.

### Modules under test

- **Router single-sided orchestrator**: `ibc_execute_single_sided_add_liquidity`, `on_single_sided_swap_reply`, `on_single_sided_add_liquidity_reply`. Prior art: existing unit tests for `ibc_execute_swap` and `on_swap_reply` in `contracts/hub/router/src/ibc/receive/swap.rs` and `contracts/hub/router/src/reply.rs` — same patterns of `MockDeps`, mocked virtual-balance/VLP queries, encoded protobuf reply data, and `rstest` parameterization.

- **Factory single-sided request + ack**: `execute_single_sided_add_liquidity_request` and `ack_single_sided_add_liquidity`. Prior art: existing unit tests for `execute_swap_request` and `ack_swap_request` in `contracts/liquidity/factory/src/` — same helpers (`init`, `seed_vlp`, `seed_escrow`, `set_escrow_token_allowed`, `default_cross_chain_config`).

- **Integration test (multi-contract, multi-chain)**: a happy-path end-to-end run through `tests-integration/src/` using `cw-orch` and `cw-orch-interchain`, exercising the IBC roundtrip from factory through router and back. This compensates for the things unit tests can't catch (real IBC ack data shape, real virtual-balance state evolution across the swap and add legs, escrow message delivery).

### Test cases (router orchestrator)

- Happy path: msg saved to pending; mint message issued for raw `amount_in`; approve message issued for `normalized_swap_amount`; final reply data is `AcknowledgementMsg::Ok(AddLiquidityResponse)`.
- Reject: `swap_route.len() != 1`.
- Reject: `swap_route[0].token_in != asset_in.token` or `token_out != asset_out`.
- Reject: `swap_amount == 0`, `swap_amount >= amount_in`.
- Reject: target VLP doesn't exist (`VLPS` miss).
- Reject: duplicate `tx_id`.
- `min_lp_out` exceeded in the add-liquidity reply → `Err(ContractError::SlippageExceeded)`, no `set_data` call.
- Swap submsg fails (`SubMsgResult::Err`) → reply returns `ContractError::Reply`.
- Add-liquidity submsg fails (`SubMsgResult::Err`) → reply returns `ContractError::Reply`.
- Verify no `asset_out` mint message is emitted in the swap-reply path.

### Test cases (factory request + ack)

- Happy path: deposit 100 USDC with 30bps partner fee → IBC msg carries 99.7 USDC; pending state stores fee accounting; success ack escrows 99.7 USDC, transfers 0.3 USDC to partner, mints LP to user; pending state cleared.
- Reject: pool does not exist (`PAIR_TO_VLP` miss) — fail before sending IBC.
- Reject: `asset_in.token == asset_out`.
- Reject: `partner_fee_bps > MAX_PARTNER_FEE_BPS`.
- Reject: `swap_amount == 0` or `swap_amount >= amount_in` (post-fee).
- Reject: duplicate `tx_id`.
- Reject: `asset_in` escrow doesn't allow the submitted denom.
- Reject: Native funds mismatch (`fund_manager` failure).
- Smart asset_in: TransferFrom submessage emitted with correct amount and addresses.
- Voucher asset_in: `UnreachableCode`.
- Ack success with `partner_fee_amount == 0`: no partner transfer message emitted; only escrow + LP mint.
- Ack success with `partner_fee_amount > 0`: both escrow and partner transfer messages emitted.
- Ack failure (Cosmos chain, `is_native == false`): refund of `amount_in + partner_fee_amount` to user.
- Ack failure (`is_native == true`): returns `Err`.

### Test cases (integration)

- End-to-end: a remote chain's factory submits `AddSingleSidedLiquidity` with native USDC against the USDC/ETH VLP; the IBC packet is relayed; the hub executes swap-then-add atomically; the ack returns `Ok`; LP CW20 minted on the remote chain; escrow holds the post-fee USDC. Repeat with a stable pool (e.g. USDC/USDT) to confirm pool-type agnosticism.
- End-to-end failure: same setup but `min_lp_out` set above the simulated LP output → ack returns Error → factory refunds full deposit to the user → no residual state on either chain.

## Out of Scope

- **Multi-hop swap routes.** The IBC message keeps `swap_route: Vec<NextSwapPair>` for forward compatibility, but v1 enforces `len() == 1`. Multi-hop adds donation risk (when the swap route does not use the target pool, the resulting deposit ratio diverges from the target pool's ratio and the over-supplied side is silently absorbed). Defer to v2 once `min_lp_out` is established as a strong-enough guard in practice.
- **Single-sided deposit from a voucher balance on the hub.** Vouchers do not originate from a remote-chain factory, so this would require a different entry point on the hub-side and a different state model. Defer.
- **Backend off-chain swap-amount calculator.** The math (accounting for protocol fees, pool curve shape, stable amp factor) is the backend team's responsibility and is not part of this contract change.
- **Single-sided remove-liquidity** (LP → one token). Symmetric feature in concept but a separate design with its own atomicity questions. Defer.
- **Dynamic re-balancing if pool reserves shift mid-tx.** `min_lp_out` is the user's protection; we do not attempt to re-compute `swap_amount` on-chain.
- **Partner fee on the add-liquidity leg.** The partner fee applies only to the swap leg, mirroring the existing swap-path fee semantics.

## Further Notes

- **Atomicity guarantee.** Because all hub operations live inside a single IBC packet execution context, any `Err` propagated from any reply handler causes the entire chain of mutations (pending-state save, virtual-balance mint, approves, VLP swap effects, VLP add-liquidity effects) to roll back. The factory then sees an error ack and refunds. This is the same model as the existing two-step add-liquidity path; the only new wrinkle is that *intermediate* mutations (the swap's virtual-balance changes) also need to roll back, which they do because they're nested submessages within the same packet.

- **Why a new IBC variant rather than reusing `Swap` + ack-chain.** Chaining a `Swap` then an `AddLiquidity` via factory acks would require *two* IBC roundtrips (the user submits Swap, waits for ack, then submits AddLiquidity). The point of this feature is single-roundtrip. The new variant is the minimum schema addition needed to express "do both on the hub in one go."

- **Why no zapper contract.** A zapper contract was considered and rejected. Both legs are hub-side VLP calls. Chaining them via `SubMsg::reply_always` within the existing router is atomic, requires no extra contract deployment, no extra audit surface, and no extra IBC packet. The router's reply dispatch already has the necessary infrastructure (reply IDs, pending-state pattern).

- **Backend dependency.** This contract change is shipped together with backend support for computing the optimal `swap_amount` from pool reserve queries. Without that, integrators would need to compute the split themselves — viable for sophisticated integrators (the math is well-known for constant-product) but a friction point for general use.
