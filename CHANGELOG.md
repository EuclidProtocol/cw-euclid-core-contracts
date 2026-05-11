# Changelog

All notable changes to Euclid core contracts are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Only contract and package changes are tracked (not test or CI changes). Each release is named after a star and carries a status: **in progress**, **freezed**, or **released**.

## Sirius (in progress)

### Added

#### Contracts

- [virtual_balance] Token metadata registry (`TOKEN_METADATA`) storing per token decimals, chain, and type
- [virtual_balance] Centralized escrow balance tracking (`ESCROW_BALANCES`), migrated from router contract
- [virtual_balance] `RegisterTokenMetadata` and `DeregisterTokenMetadata` execute messages
- [virtual_balance] `GetEscrowBalance`, `GetTokenEscrows`, `GetAllEscrowBalances` queries for escrow lookups
- [virtual_balance] `GetTokenMetadataByDenom`, `GetTokenMetadata`, `GetAllTokenMetadata`, `GetTokenStatus` queries
- [virtual_balance] Migration entry point: normalizes `BALANCES` to `VOUCHER_BALANCES` at 24 decimal precision, seeds `TOKEN_METADATA` from `MigrateMsg`, queries router `GetAllEscrows` to populate `ESCROW_BALANCES`
- [cp_vlp] Migration entry point that normalizes pool reserves by querying virtual_balance for token decimals
- [stable_vlp] Migration entry point that normalizes pool reserves by querying virtual_balance for token decimals
- [stable_vlp] `MIN_AMP = 100` constant with validation in `compute_stable_swap`, `update_amp_factor`, and `instantiate`
- [router] `GetAllEscrows` query (deprecated on arrival, exists only to support virtual_balance migration)
- [orderbook_deposits] `NULLIFIERS` map tracking withdrawn amounts by hashed key
- [factory] `AddSingleSidedLiquidity` execute entry point: user deposits a single token, the hub atomically swaps a backend-computed portion and adds liquidity on the target VLP in one IBC roundtrip
- [factory] `AddSingleSidedLiquidity` supports `TokenType::Smart` (CW20) `asset_in` via the `IncreaseAllowance` + `TransferFrom` pattern (mirrors `add_liquidity_request`); `TokenType::Voucher` rejected as `UnreachableCode`
- [factory] `AddSingleSidedLiquidity` accepts an optional `partner_fee` (bounded by `MAX_PARTNER_FEE_BPS`); fee retained at the factory and routed to the recipient on ack success, refunded with `amount_in` on ack failure
- [factory] `PENDING_SINGLE_SIDED_LIQUIDITY` map and `SingleSidedLiquidityRequest` pending-state struct carry `partner_fee_amount` and `partner_fee_recipient` for the ack handler

#### Packages

- [euclid] `normalize.rs` module with `normalize_token_to_voucher`, `normalize_voucher_to_token`, and generic `normalize` functions
- [euclid] `utils/migration.rs` module with `query_token_decimals` and `query_voucher_balance` helpers
- [euclid] `TokenMetadata` struct (token, chain_uid, token_type, allowed)
- [euclid] `VOUCHER_DECIMAL = 24` constant defining unified voucher precision
- [euclid] `TokenType` methods: `get_decimals()`, `query_decimals()`, `get_type()`, `is_same_type()`, `from_key()`

#### Events (important for backend indexing)

- [euclid] `token_metadata_update_event(token_metadata, action)` emitting `euclid-token-metadata-update` with attributes: action, token, chain_uid, token_type, decimals, allowed
- [euclid] `virtual_balance_change_event(action, amount, user, token_id)` emitting `euclid-virtual-balance-change` with attributes: action, amount, user, token_id
- [euclid] `escrow_balance_change_event(action, amount, token_id, chain_uid, token_type)` emitting `euclid-escrow-balance-change` with attributes: action, amount, token_id, chain_uid, token_type
- [virtual_balance] `normalized_amount` attribute added to `execute_mint` response

#### Documentation

- `VOUCHER.md` documenting voucher normalization architecture
- `MIGRATION.md` documenting migration deployment guide and contract ordering

### Changed

#### State (Uint128 to Uint256 widening)

- [virtual_balance] `BALANCES` replaced by `VOUCHER_BALANCES` (Uint256, 24 decimal precision)
- [virtual_balance] `ALLOWANCES` replaced by `VOUCHER_ALLOWANCES` with new `VoucherAllowance` struct (Uint256 amount, optional expiry timestamp)
- [router] `PendingReleaseVoucher.total_amount` and `.release_fee_amount` widened to Uint256
- [router] `RELEASE_FEES` and `DEFAULT_RELEASE_FEE` widened to Uint256
- [cp_vlp] `CHAIN_LP_TOKENS`, `BALANCES`, `COLLATERAL_LP_TOKENS` widened to Uint256
- [stable_vlp] `CHAIN_LP_TOKENS`, `BALANCES`, `COLLATERAL_LP_TOKENS` widened to Uint256
- [escrow] `State.total_amount`, `DENOM_TO_AMOUNT` widened to Uint256
- [relayer] `NONCES` widened to Uint256
- [meta_transaction] `NONCES` widened to Uint256
- [orderbook_deposits] `ASSET_DEPOSITS`, `USER_DEPOSITS` widened to Uint256
- [astroport_forwarding] `ForwardingState.from_amount`, `.previous_balance` widened to Uint256
- [osmosis_forwarding] `ForwardingState.from_amount`, `.previous_balance` widened to Uint256

#### Messages (breaking API changes)

- [virtual_balance] `ExecuteMint` now requires `token_type` and `token_source_chain_uid` fields
- [virtual_balance] `ExecuteBurn` restructured: old `(amount, balance_key)` replaced with `(voucher_amount, from_user, token_id, release_denom, release_chain_uid)`
- [virtual_balance] `ExecuteTransfer`, `ExecuteApprove` amounts widened to Uint256
- [virtual_balance] `VoucherReceive.amount` changed from Uint128 to Uint256
- [virtual_balance] `MigrateMsg` now requires `token_metadata: Vec<TokenMetadata>`
- [virtual_balance] Query response amounts widened to Uint256 (`GetBalanceResponse`, `GetUserBalancesResponseItem`, `GetAllBalancesResponseItem`, `GetTokenBalancesResponseItem`)
- [virtual_balance] `GetAllowanceResponse.allowance` type changed from `Allowance` to `VoucherAllowance`
- [virtual_balance] Pagination types widened from `Pagination<Uint128>` to `Pagination<Uint256>` across balance queries
- [factory] `DepositToken.amount_in`, `TransferVoucher.amount`, `ExecuteSwapRequest.amount_in`, `.min_amount_out` widened to Uint256
- [factory] `PartnerFeesCollectedPerDenomResponse.total`, `FeeBracket.fee` widened to Uint256
- [factory] `ReleaseEscrowDenomsResponse`, `ReleaseEscrowResponse` amounts widened to Uint256
- [factory] Pagination types widened to `Pagination<Uint256>` for swap and liquidity queries
- [escrow] `Withdraw.amount`, `StateResponse.total_amount`, `DenomBalanceResponse.amount` widened to Uint256
- [claimer] `Claim.amount` widened to Uint256
- [meta_transaction] `NonceRelayedResponse.height` widened to Uint256
- [lp_token] `Transfer`, `Burn`, `Send`, and all allowance amounts widened to Uint256
- [cp_vlp] `GetStateResponse.total_lp_tokens`, `TotalFeesPerDenomResponse` fees, `PoolResponse` reserves widened to Uint256
- [stable_vlp] Same query response widenings as cp_vlp

#### Types

- [euclid] `TokenType::Native` and `TokenType::Smart` gained optional `decimals: Option<u32>` field
- [euclid] `create_voucher_transfer_msg` amount parameter widened to Uint256

#### Behavior

- [virtual_balance] Allowance deduction now validates optional expiry timestamp
- [virtual_balance] `_deduct_allowance` signature gained `env` parameter for expiry checks
- [virtual_balance] `execute_remove_zero_state_values` operates on `VOUCHER_BALANCES`/`VOUCHER_ALLOWANCES`
- [router] IBC receive handlers pass `token_type` and `token_source_chain_uid` to `ExecuteMint`
- [router] `_release_voucher` queries escrow from virtual_balance instead of local state
- [router] IBC ack failure path delegates re-mint to virtual_balance (no local escrow restore)
- [factory] Replaced `.unwrap()` with error propagation in LP minting and burning (4 sites)

#### Events (changes affecting backend indexing)

- [euclid] `register_denom_event` and `deregister_denom_event` removed, replaced by `token_metadata_update_event`
- [router] Denom registration in IBC token receive now uses `simple_event()` with action, token, chain_uid, token_type attributes (instead of dedicated event functions)
- [virtual_balance] `execute_burn` now emits `virtual_balance_change_event` and `escrow_balance_change_event`
- [virtual_balance] `execute_mint` now emits `virtual_balance_change_event` and `escrow_balance_change_event` (matching `execute_burn`)
- [router] Migration event attribute changed from `admins_migrated` to `reserves_normalized`

### Fixed

- [euclid] Decimals greater than 24 rejected in normalization functions (prevents silent overflow)
- [virtual_balance] Dust amounts that truncate to zero gracefully skipped during voucher release (prevents multi-recipient transaction aborts)
- [euclid] `query_token_decimals` pagination was capped at default 10 entries, now passes explicit unlimited limit
- [virtual_balance] Phase 2 idempotency guard (`may_load` skip) prevents double execution of migration
- [virtual_balance] Strict single metadata assertion in migration prevents duplicate seeds
- [factory] `ZeroAssetAmount` error on `Limit::Dynamic` validation was misleading, now corrected
- [cp_vlp, stable_vlp] Migration uses `.may_load()` instead of `.load()` for proper optional semantics

### Deprecated

- [router] `ESCROW_BALANCES` storage definition kept for backward compatibility, all reads and writes removed
- [router] `TOKEN_DENOMS` storage deprecated (moved to virtual_balance)
- [router] `GetAllEscrows` query (exists only for migration, not for production use)
- [virtual_balance] `NormalizeBalanceKeys` execute message removed (migration function no longer needed)
