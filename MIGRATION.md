# Migration Guide: Normalized Voucher System

## Overview

This migration introduces a normalized voucher system in `virtual_balance` where all voucher balances are stored at 24 decimal precision using `Uint256`. Additionally, `ESCROW_BALANCES` has been moved from the `router` contract to `virtual_balance`, centralizing all balance management in one contract.

## Contracts Changed

### virtual_balance (breaking)

**State changes:**
- `BALANCES` (Map, Uint256) replaced by `VOUCHER_BALANCES` (Map, Uint256, normalized to 24 decimals)
- `ALLOWANCES` (Map, Allowance) replaced by `VOUCHER_ALLOWANCES` (Map, VoucherAllowance with optional expiry)
- New: `ESCROW_BALANCES` (Map, Uint256) tracks raw token amounts held in escrow (previously in router)
- New: `TOKEN_METADATA` (Map) stores per token per chain decimal info and token type

**Message changes:**
- `ExecuteMint`: added `token_type: TokenType`, `token_source_chain_uid: ChainUid`
- `ExecuteBurn`: added `token_type: TokenType`, `token_source_chain_uid: ChainUid`
- All `amount` fields in execute messages: `Uint256` to `Uint256`
- New: `ExecuteMsg::RegisterTokenMetadata { token_metadata }`
- New: `ExecuteMsg::UpdateTokenMetadata { token_metadata }`
- New: `QueryMsg::GetEscrowBalance { token, chain_uid, token_type }` and `GetEscrowBalanceResponse`
- `VoucherReceive.amount`: `Uint256` to `Uint256`

### router (breaking)

**State changes:**
- Removed: `ESCROW_BALANCES` reads/writes (escrow management now delegated to `virtual_balance`)

**Code changes:**
- IBC receive handlers (`token.rs`, `swap.rs`, `pool.rs`): removed local escrow balance tracking, added `token_type` and `token_source_chain_uid` to `ExecuteMint` calls
- `ack_and_timeout.rs`: removed escrow restore on failed ack (virtual_balance handles this via re-mint which re-increments escrow)
- `execute/token.rs`: `_release_voucher` now queries escrow from virtual_balance instead of local state, `ExecuteBurn` calls include `token_type` and `token_source_chain_uid`

### euclid package (breaking)

- `VoucherReceive.amount`: `Uint256` to `Uint256`
- `create_voucher_transfer_msg`: amount parameter `Uint256` to `Uint256`
- `ExecuteMint`, `ExecuteBurn` message structs updated with new fields
- New types: `GetEscrowBalanceResponse`

### pool package (non-breaking internal)

- Added `.into()` conversions where `Uint256` values are passed to functions now expecting `Uint256`

### claimer, orderbook_deposits (non-breaking internal)

- Added `try_into()` conversions for `VoucherReceive.amount` (Uint256 to Uint256) at contract boundaries

## Migration Steps

### 1. Register Token Metadata

Before any mints can work, all existing tokens must have their metadata registered. For each token active in the system:

```
ExecuteMsg::RegisterTokenMetadata {
    token_metadata: TokenMetadata {
        token_id: "<token_id>",
        chain_uid: "<source_chain>",
        decimals: <token_decimals>,  // e.g. 6 for ATOM, 18 for ETH
        token_type: <Native|Smart|Cw20>,
        allowed: true,
    }
}
```

### 2. Deploy Updated virtual_balance

Deploy the new virtual_balance contract. The new storage keys (`voucher_balances`, `voucher_allowances`, `escrow_balances`, `token_metadata`) do not collide with the old keys (`balances`, `allowances`), so both can coexist during migration.

### 3. Migrate Existing Balances

Run a migration entry point or admin script to:

1. Read all entries from `BALANCES` (old, Uint256)
2. For each entry, look up the token's decimals from `TOKEN_METADATA`
3. Normalize the balance: `amount * 10^(24 - token_decimals)`
4. Write to `VOUCHER_BALANCES` (new, Uint256)
5. Read all entries from `ALLOWANCES` (old) and write to `VOUCHER_ALLOWANCES` (new)

### 4. Migrate Escrow Balances from Router

Transfer escrow balance state from router to virtual_balance:

1. Query all escrow balances from the router's deprecated `ESCROW_BALANCES`
2. Write each entry to virtual_balance's `ESCROW_BALANCES`
3. Verify totals match

### 5. Deploy Updated Router

Deploy the updated router that no longer writes to local `ESCROW_BALANCES`. The router now delegates all escrow management to virtual_balance.

### 6. Deploy Updated Dependent Contracts

Redeploy or migrate any contracts that interact with `VoucherReceive` (claimer, orderbook_deposits, pool) to handle the `Uint256` amount field.

### 7. Cleanup (Optional)

After verifying the migration is complete:
- Remove deprecated `BALANCES` and `ALLOWANCES` entries from virtual_balance storage
- Remove deprecated `ESCROW_BALANCES` entries from router storage

## Verification Checklist

- [ ] All tokens have metadata registered with correct decimals
- [ ] `VOUCHER_BALANCES` contain normalized (24 decimal) values for all users
- [ ] `ESCROW_BALANCES` in virtual_balance match previous router escrow values
- [ ] Router no longer writes to local escrow state
- [ ] Mint/burn/transfer operations work end to end with normalized amounts
- [ ] IBC ack/timeout flows correctly re-mint on failure
