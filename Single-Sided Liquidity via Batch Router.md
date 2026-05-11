---
title: Single-Sided Liquidity via Batch Router

---

# Single-Sided Liquidity via Batch Router

## Context

Users currently must provide both tokens of a pair to add liquidity. This plan enables single-sided deposits: the user sends one token, the Router internally swaps half of it against the target VLP, then adds the resulting balanced pair as liquidity — all in a single IBC roundtrip. A zapper contract was considered and rejected because both the swap and add_liquidity are purely hub-side VLP operations; chaining them as SubMsg replies within the Router is atomic, requires no extra contract, and needs only one IBC packet.

The swap amount is computed off-chain by the backend (using pool reserve queries) and passed as a parameter. On-chain slippage guards (`min_amount_out` and `min_lp_out`) protect the user against stale computation.

---

## Files to Modify

| File | Change |
|---|---|
| `packages/euclid_ibc/src/router_ibc.rs` | Add `SingleSidedAddLiquidity` variant + struct |
| `packages/euclid/src/liquidity.rs` | Add `SingleSidedLiquidityRequest` struct |
| `packages/euclid/src/msgs/factory/msg.rs` | Add `AddSingleSidedLiquidity` to factory `ExecuteMsg` |
| `contracts/liquidity/factory/src/state.rs` | Add `PENDING_SINGLE_SIDED_LIQUIDITY` map |
| `contracts/liquidity/factory/src/execute/pool.rs` | Add `execute_single_sided_add_liquidity_request` |
| `contracts/liquidity/factory/src/contract.rs` | Wire new `ExecuteMsg` variant |
| `contracts/liquidity/factory/src/ibc/ack_and_timeout.rs` | Add `ack_single_sided_add_liquidity`, wire into dispatch |
| `contracts/hub/router/src/state.rs` | Add `PENDING_SINGLE_SIDED_LIQUIDITY` map |
| `contracts/hub/router/src/reply.rs` | Add 2 reply ID constants + 2 reply handlers |
| `contracts/hub/router/src/ibc/receive/pool.rs` | Add `ibc_execute_single_sided_add_liquidity` |
| `contracts/hub/router/src/ibc/receive/mod.rs` | Wire new `RouterCrossChainExecuteMsg` variant |
| `contracts/hub/router/src/contract.rs` | Add new reply IDs to `reply` dispatch |

---

## Step 1 — New IBC Message Type (`packages/euclid_ibc/src/router_ibc.rs`)

Add to `RouterCrossChainExecuteMsg` enum:
```rust
SingleSidedAddLiquidity(RouterCrossChainSingleSidedAddLiquidityMsg),
```

Add struct:
```rust
#[cw_serde]
pub struct RouterCrossChainSingleSidedAddLiquidityMsg {
    pub sender: CrossChainUser,
    pub asset_in: TokenWithDenom,      // token the user is depositing
    pub amount_in: Uint256,            // total raw amount of asset_in
    pub swap_amount: Uint256,          // raw amount of asset_in to swap (backend-computed)
    pub asset_out: Token,              // the other token in the target pool
    pub min_amount_out: Uint256,       // min swap output, in 24-dec voucher units
    pub swap_route: Vec<NextSwapPair>, // routing path (can be multi-hop)
    pub min_lp_out: Uint256,           // min LP tokens to receive (final slippage guard)
    pub slippage_tolerance_bps: u64,   // ratio tolerance for the add_liquidity step
    pub tx_id: String,
}
```

Update `get_tx_id()` and `get_sender()` match arms to include the new variant.

---

## Step 2 — Pending State Structs (`packages/euclid/src/liquidity.rs`)

```rust
#[cw_serde]
pub struct SingleSidedLiquidityRequest {
    pub sender: String,
    pub tx_id: String,
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint256,  // original full amount — used for escrow on success, refund on failure
}
```

---

## Step 3 — Factory User-Facing Message (`packages/euclid/src/msgs/factory/msg.rs`)

Add to `ExecuteMsg`:
```rust
#[cfg_attr(not(target_arch = "wasm32"), cw_orch(payable))]
AddSingleSidedLiquidity {
    asset_in: TokenWithDenom,
    amount_in: Uint256,
    asset_out: Token,
    swap_amount: Uint256,
    swap_route: Vec<NextSwapPair>,
    min_amount_out: Uint256,
    min_lp_out: Uint256,
    slippage_tolerance_bps: u64,
    cross_chain_config: CrossChainConfig,
},
```

---

## Step 4 — Factory State (`contracts/liquidity/factory/src/state.rs`)

```rust
pub const PENDING_SINGLE_SIDED_LIQUIDITY: Map<(Addr, String), SingleSidedLiquidityRequest> =
    Map::new("pending_single_sided_liquidity");
```

---

## Step 5 — Factory Execution (`contracts/liquidity/factory/src/execute/pool.rs`)

Add `execute_single_sided_add_liquidity_request`. Follow the same pattern as `execute_swap_request`:

1. Validate `swap_amount < amount_in`, both > 0
2. Validate `asset_in` escrow exists and denom is allowed (same as swap)
3. Validate native funds via `fund_manager.use_fund(amount_in, denom)`
4. Validate no duplicate `tx_id` in `PENDING_SINGLE_SIDED_LIQUIDITY`
5. Save `SingleSidedLiquidityRequest { sender, tx_id, asset_in, amount_in }` to `PENDING_SINGLE_SIDED_LIQUIDITY`
6. Construct and send `RouterCrossChainExecuteMsg::SingleSidedAddLiquidity(msg).to_msg(...)`

---

## Step 6 — Factory Ack Handler (`contracts/liquidity/factory/src/ibc/ack_and_timeout.rs`)

Add `ack_single_sided_add_liquidity` and wire it in `reusable_internal_ack_call` under `RouterCrossChainExecuteMsg::SingleSidedAddLiquidity`.

**Success path** — mirrors `ack_add_liquidity`:
1. Load and remove `PENDING_SINGLE_SIDED_LIQUIDITY[(sender, tx_id)]`
2. Update `VLP_TO_LP_SHARES` (increment by `mint_lp_tokens`)
3. Send `amount_in` of `asset_in` to its Escrow via `create_escrow_msg` — only asset_in is ever escrowed; asset_out never leaves the hub
4. Mint LP CW20 tokens to sender via `VLP_TO_LP_TOKEN`

**Error path** — mirrors `ack_swap_request`:
1. If `is_native`: return `Err`
2. Otherwise: refund full `amount_in` to sender via `create_transfer_msg`

---

## Step 7 — Router State (`contracts/hub/router/src/state.rs`)

```rust
pub const PENDING_SINGLE_SIDED_LIQUIDITY: Map<String, RouterCrossChainSingleSidedAddLiquidityMsg> =
    Map::new("pending_single_sided_liquidity");
```

Keyed by `tx_id` (same pattern as router's `PENDING_SWAPS`).

---

## Step 8 — New Reply IDs (`contracts/hub/router/src/reply.rs`)

```rust
pub const SINGLE_SIDED_SWAP_REPLY_ID: u64 = 9;
pub const SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID: u64 = 10;
```

Existing IDs 1–8 are taken. Wire both into the `reply` match in `contract.rs`.

---

## Step 9 — Router Receive Handler (`contracts/hub/router/src/ibc/receive/pool.rs`)

Add `ibc_execute_single_sided_add_liquidity(deps, env, msg)`:

1. Duplicate-check tx_id against `PENDING_SINGLE_SIDED_LIQUIDITY`
2. Save full msg to `PENDING_SINGLE_SIDED_LIQUIDITY[tx_id]`
3. Validate swap route (first token_in == asset_in.token, last token_out == asset_out)
4. Normalize `swap_amount` to voucher units using `query_token_metadata_by_denom` + `normalize_token_to_voucher`
5. Mint virtual balance for full `amount_in` (raw) — same as `ibc_execute_add_liquidity`
6. Simulate swap via `QueryMsg::SimulateSwap` to validate `min_amount_out`
7. Approve first VLP in `swap_route` for `normalized_swap_amount`
8. `SubMsg::reply_always(VlpSwapMsg { asset_in, amount_in: normalized_swap_amount, min_token_out: min_amount_out, next_swaps, tx_id }, SINGLE_SIDED_SWAP_REPLY_ID)`

---

## Step 10 — `on_single_sided_swap_reply` (`contracts/hub/router/src/reply.rs`)

Called after the internal swap completes. Chains directly into add_liquidity.

1. Parse `VlpSwapResponse { amount_out, asset_out, tx_id }`
2. Load `PENDING_SINGLE_SIDED_LIQUIDITY[tx_id]`
3. Compute `remaining_normalized = normalize(amount_in - swap_amount, decimals)`
   - `amount_in - swap_amount` is still in user's virtual_balance from the initial mint in step 9
4. **Mint `amount_out` of `asset_out` as voucher** to user's virtual_balance:
   ```rust
   ExecuteMint { amount: amount_out, token_type: TokenType::Voucher {}, token_source_chain_uid: vsl_chain_uid }
   ```
   This is safe because asset_out is consumed immediately by add_liquidity and never escrow-backed
5. Look up pool VLP: `VLPS[(asset_in.token, asset_out)]`
6. Approve VLP for `remaining_normalized` of asset_in
7. Approve VLP for `amount_out` of asset_out
8. `SubMsg::reply_always(VlpAddLiquidityMsg { liquidity: (remaining, amount_out), slippage_tolerance_bps, sender, tx_id }, SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID)`

---

## Step 11 — `on_single_sided_add_liquidity_reply` (`contracts/hub/router/src/reply.rs`)

1. Parse `AddLiquidityResponse { mint_lp_tokens, vlp_address, tx_id }`
2. Load `PENDING_SINGLE_SIDED_LIQUIDITY[tx_id]` to retrieve `min_lp_out`
3. **Validate** `mint_lp_tokens >= min_lp_out` — error here becomes an error ack, triggering factory refund
4. Remove from `PENDING_SINGLE_SIDED_LIQUIDITY`
5. Build `AcknowledgementMsg::Ok(AddLiquidityResponse { ... })` and `set_data`

---

## Execution Flow Summary

```
User sends 100 USDC to Factory
  │
  ▼
Factory: execute_single_sided_add_liquidity_request
  - saves PENDING_SINGLE_SIDED_LIQUIDITY[(sender, tx_id)] = { asset_in: USDC, amount_in: 100 }
  - sends IBC → RouterCrossChainExecuteMsg::SingleSidedAddLiquidity
  │
  ▼
Router: ibc_execute_single_sided_add_liquidity
  - saves PENDING_SINGLE_SIDED_LIQUIDITY[tx_id] = full msg
  - mints virtual_balance(100 USDC)
  - approves VLP for swap_amount (e.g. 47 USDC normalized)
  - SubMsg VlpSwapMsg → SINGLE_SIDED_SWAP_REPLY_ID
  │
  ▼ (VLP executes swap, returns amount_out ETH)
on_single_sided_swap_reply
  - mints virtual_balance(amount_out ETH, voucher type)
  - approves pool VLP for remaining 53 USDC + amount_out ETH
  - SubMsg VlpAddLiquidityMsg → SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID
  │
  ▼ (VLP adds liquidity, returns mint_lp_tokens)
on_single_sided_add_liquidity_reply
  - validates mint_lp_tokens >= min_lp_out
  - sets ack = Ok(AddLiquidityResponse)
  │ IBC ack
  ▼
Factory: ack_single_sided_add_liquidity
  - sends 100 USDC → USDC Escrow  (only asset_in ever touches escrow)
  - mints LP CW20 tokens → User
```

---

## Verification

```bash
# Build
cargo build -p factory
cargo build -p router

# Type-check
cargo clippy -p factory -p router --all-targets

# Run new unit tests (to be added alongside implementation)
cargo test -p factory execute_single_sided
cargo test -p factory ack_single_sided
cargo test -p router single_sided_swap_reply
cargo test -p router single_sided_add_liquidity_reply

# Run full test suites to check for regressions
cargo test -p factory
cargo test -p router
```

Key test cases:
- Happy path: 100 USDC in, correct LP tokens out, USDC escrowed
- `swap_amount >= amount_in` is rejected by factory
- `min_amount_out` exceeded → error ack → factory refunds USDC
- `min_lp_out` not met → error ack → factory refunds USDC
- Duplicate `tx_id` rejected
- Multi-hop `swap_route` (USDC→ATOM→ETH) executes correctly
