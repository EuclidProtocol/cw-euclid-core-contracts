# Voucher Normalization Architecture

## Core Principle

All token amounts in the Euclid hub are stored and processed in **voucher units** (24 decimal precision) via `Uint256`. Raw token amounts (with their native decimals, e.g. 6 for USDC, 18 for ETH) are normalized to 24 decimals at the system boundary (the Router's IBC receive handlers) and denormalized back to raw units only when releasing tokens to destination chains.

## Architecture Layers

```
External Chain (raw decimals)
    │
    ▼
┌─────────────────────────────────────────┐
│  Router (IBC Receive Handlers)          │
│  - Normalizes raw → voucher on entry    │
│  - Denormalizes voucher → raw on exit   │
│  - ALL values passed inward use voucher │
└─────────┬───────────────────────────────┘
          │ voucher amounts only
          ▼
┌─────────────────────────────────────────┐
│  VLP (Virtual Liquidity Pool)           │
│  - Operates ONLY in voucher amounts     │
│  - Swap, add/remove liquidity           │
│  - Never sees raw token decimals        │
└─────────────────────────────────────────┘
          │ voucher amounts only
          ▼
┌─────────────────────────────────────────┐
│  Pool Math (packages/pool)              │
│  - All calculations in voucher amounts  │
│  - stable_math, cp_swap, LP allocation  │
└─────────────────────────────────────────┘
          │ voucher amounts only
          ▼
┌─────────────────────────────────────────┐
│  Virtual Balance                        │
│  - VOUCHER_BALANCES: 24-dec voucher     │
│  - ESCROW_BALANCES: raw token amounts   │
│  - Mint normalizes raw → voucher        │
│  - Burn denormalizes voucher → raw      │
└─────────────────────────────────────────┘
```

## Normalization Points (raw → voucher)

All normalization happens in the Router layer and Virtual Balance mint:


| Location                          | Function                    | What's Normalized            |
| --------------------------------- | --------------------------- | ---------------------------- |
| `router/ibc/receive/swap.rs:88`   | `ibc_execute_swap`          | `amount_in` for swap         |
| `router/ibc/receive/swap.rs:211`  | `ibc_execute_swap`          | `partner_fee_amount`         |
| `router/ibc/receive/pool.rs:243`  | `ibc_execute_add_liquidity` | token amounts for liquidity  |
| `router/ibc/receive/token.rs:163` | `ibc_execute_deposit_token` | deposit `amount_in`          |
| `virtual_balance/execute.rs:44`   | `execute_mint`              | raw amount → voucher balance |


## Denormalization Points (voucher → raw)

Denormalization happens when releasing tokens back to chains:


| Location                         | Function           | What's Denormalized                   |
| -------------------------------- | ------------------ | ------------------------------------- |
| `router/execute/token.rs:278`    | `_release_voucher` | voucher → raw for escrow comparison   |
| `router/execute/token.rs:343`    | `_release_voucher` | raw release → voucher for burn amount |
| `virtual_balance/execute.rs:127` | `execute_burn`     | voucher → raw for escrow decrement    |


## Key Rules

1. **VLPs never normalize/denormalize.** They receive and return voucher amounts only.
2. **Pool math never normalizes/denormalize.** All inputs and outputs are voucher amounts.
3. `**min_amount_out` is always in voucher units.** It is compared directly against VLP output (which is in voucher units). No normalization needed.
4. `**QuerySimulateSwap` expects voucher amounts.** Both `amount_in` and `min_amount_out` should be in voucher units since the query is forwarded directly to VLPs.
5. **Escrow balances are in raw token amounts.** They track actual tokens held per chain/denom.
6. **Voucher balances are in 24-decimal units.** They represent the user's normalized balance.

## Known Issues / Gaps

