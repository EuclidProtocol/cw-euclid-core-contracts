# Changelog

All notable changes to Euclid core contracts are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Only contract and package changes are tracked (not test or CI changes). Each release is named after a star and carries a status: **in progress**, **freezed**, or **released**.

## Sirius (in progress)

### Added

#### Contracts

- [pool_factory] New contract at `contracts/liquidity/pool_factory/` introduced by SC-4 Slice 1; owns CP/Stable pool registry (`PAIR_TO_VLP`, `VLP_TO_LP_TOKEN`), pending pool requests, `MAIN_FACTORY_ADDRESS`, and one-shot `MIGRATION_ACCEPTED` flag
- [pool_factory] Execute entries `OnRequestPoolCreation` (delegated by main factory; builds outbound `RouterCrossChainExecuteMsg::RequestPoolCreation` and calls `ProxySendPacket`), `OnPoolAck` (ack dispatcher), and `MigrateAcceptPoolState` (one-shot drain-and-cut accept)
- [pool_factory] Queries `GetVlp { pair }`, `GetLpToken { vlp }`, `GetMainFactoryAddress {}`
- [pool_factory] Reply IDs in disjoint namespace from main factory (`LP_INSTANTIATE_REPLY_ID = 1001`, etc.) so reply collisions across the two contracts are structurally impossible
- [pool_factory] Deep `outbound` module with table-driven tests for outbound packet builders
- [pool_factory] SC-4 Slice 2: `OnAddLiquidity` execute entry (auth: caller is main factory) records `PENDING_ADD_LIQUIDITY`, builds outbound `RouterCrossChainExecuteMsg::AddLiquidity` via `outbound::add_liquidity`, and dispatches through main factory's `ProxySendPacket`
- [pool_factory] SC-4 Slice 2: `OnPoolAck` extended to handle the `AddLiquidity` variant — on success issues `ProxyMintLpToken` to main factory; on failure issues `ProxyReleaseEscrow` per non-voucher token to refund the user
- [pool_factory] SC-4 Slice 2: `PENDING_ADD_LIQUIDITY: Map<(Addr, String), AddLiquidityRequest>` state map mirrors main factory's pre-refactor pending-queue shape
- [pool_factory] SC-4 Slice 3: `OnRemoveLiquidity` execute entry (auth: caller is main factory) records `PENDING_REMOVE_LIQUIDITY`, builds outbound `RouterCrossChainExecuteMsg::RemoveLiquidity` via `outbound::remove_liquidity`, and dispatches through main factory's `ProxySendPacket`
- [pool_factory] SC-4 Slice 3: `OnPoolAck` extended to handle the `RemoveLiquidity` variant — on success decrements `VLP_TO_LP_SHARES` and issues `ProxyBurnLpToken`; on failure issues `ProxyTransferLpToken` to return the held LP cw20 tokens to the user
- [pool_factory] SC-4 Slice 3: `PENDING_REMOVE_LIQUIDITY: Map<(Addr, String), RemoveLiquidityRequest>` and `VLP_TO_LP_SHARES: Map<String, Int256>` state maps mirror main factory's pre-refactor shape
- [pool_factory] SC-4 Slice 4: new `execute::clp` module with `OnRequestConcentratedPoolCreation` execute entry (auth: caller is main factory) — validates pair/pool_key/fee/spacing, records `PENDING_CONCENTRATED_POOL_REQUESTS`, builds outbound `RouterCrossChainExecuteMsg::RequestConcentratedPoolCreation` via `outbound::request_concentrated_pool_creation`, and dispatches through main factory's `ProxySendPacket`
- [pool_factory] SC-4 Slice 4: `OnPoolAck` extended to handle the `RequestConcentratedPoolCreation` variant — on success writes `CONCENTRATED_VLPS`; on failure logs and (for native chains) propagates the hub error
- [pool_factory] SC-4 Slice 4: `CONCENTRATED_VLPS: Map<String, String>` (keyed by `PoolKey::to_map_key()`) and `POSITION_TOKEN_CONTRACT: Item<Addr>` state items mirror main factory's pre-refactor shape; `ConcentratedPoolCreateRequest` + `PENDING_CONCENTRATED_POOL_REQUESTS` map the pending-queue
- [pool_factory] SC-4 Slice 4: new queries `GetConcentratedVlp { pool_key }` and `GetPositionTokenContract {}` exposing the new CLP/position-token surface
- [factory] SC-4 Slice 4: `ProxyMintPosition { token_id, owner, vlp_address, liquidity }` execute entry (auth: pool factory) mints into the singleton position-token NFT contract held by main factory — added now so the auth boundary is in place for Slice 5 CLP add-liquidity
- [factory] `ProxySendPacket` execute entry (auth: `info.sender == POOL_FACTORY_ADDRESS`) routing pool packets through main factory's existing IBC/native transport
- [factory] SC-4 Slice 2: `ProxyMintLpToken { lp_token, recipient, amount }` execute entry (auth: pool factory) mints LP cw20 tokens — main factory remains the cw20 minter
- [factory] SC-4 Slice 2: `ProxyReleaseEscrow { token, denom, recipient, amount }` execute entry (auth: pool factory) drives an escrow `Withdraw` via the existing `RELEASE_ESCROW_REPLY_ID` reply path
- [factory] SC-4 Slice 3: `ProxyBurnLpToken { lp_token, amount }` execute entry (auth: pool factory) burns LP cw20 tokens held by main factory after a successful remove-liquidity ack
- [factory] SC-4 Slice 3: `ProxyTransferLpToken { lp_token, recipient, amount }` execute entry (auth: pool factory) returns LP cw20 tokens held by main factory back to the original sender after a failed remove-liquidity ack
- [factory] `SetPoolFactory` admin entry (migration admin, one-shot) for fresh-chain bootstrap
- [factory] State items `POOL_FACTORY_ADDRESS: Item<Addr>` and `POOL_FACTORY_INITIALISED: Item<bool>`
- [factory] Queries `QueryAdminRole { addr, role }` and `QueryPoolFactoryAddress {}`

#### Packages

- [euclid] New `msgs::pool_factory` module with `InstantiateMsg`, `ExecuteMsg`, `QueryMsg`, `MigrateMsg`, and response types
- [euclid] New factory response types `QueryAdminRoleResponse` and `QueryPoolFactoryAddressResponse`
- [euclid] SC-4 Slice 2: `pool_factory::ExecuteMsg::OnAddLiquidity` variant for delegated add-liquidity
- [euclid] SC-4 Slice 2: `factory::ExecuteMsg::ProxyMintLpToken` and `factory::ExecuteMsg::ProxyReleaseEscrow` proxy variants
- [euclid] SC-4 Slice 3: `pool_factory::ExecuteMsg::OnRemoveLiquidity` variant for delegated remove-liquidity
- [euclid] SC-4 Slice 3: `factory::ExecuteMsg::ProxyBurnLpToken` and `factory::ExecuteMsg::ProxyTransferLpToken` proxy variants for the remove-liquidity burn/refund paths
- [euclid] SC-4 Slice 4: `pool_factory::ExecuteMsg::OnRequestConcentratedPoolCreation` variant for delegated CLP pool creation; `MigrateAcceptPoolState` extended with optional `concentrated_vlps` and `position_token_contract` fields
- [euclid] SC-4 Slice 4: `pool_factory::QueryMsg::GetConcentratedVlp` / `GetPositionTokenContract` + matching response types
- [euclid] SC-4 Slice 4: `factory::ExecuteMsg::ProxyMintPosition` proxy variant for position-NFT minting from pool factory
- [euclid] SC-4 reply-data amendment PR A: `msgs::pool_factory::PoolFactoryReply` enum (single variant `SendPacket { msg, timeout, ack_response, sender }`) — the typed `Response::data` shape pool_factory will return on delegated `On*` handlers in PRs B–E so main factory can run `execute_send_packet` from its reply handler instead of routing through `ProxySendPacket`
- [euclid-ibc] SC-4 reply-data amendment PR A: `RouterCrossChainExecuteMsg::is_pool_variant()` method centralising the pool-variant matcher so main factory's inbound ack dispatcher and the new outbound reply handler share a single source of truth
- [factory] SC-4 reply-data amendment PR A: `POOL_FACTORY_DELEGATE_REPLY_ID` reply id and `on_pool_factory_delegate_reply` handler — decodes `PoolFactoryReply::SendPacket` from a successful submsg's data, validates the inner `RouterCrossChainExecuteMsg` is a pool variant (defence in depth), and dispatches via the existing `to_msg` flow. Additive: `ProxySendPacket` remains in place until PR E retires it

### Changed

#### Contracts

- [factory] `RequestPoolCreation` user-facing handler shrinks to a thin stub when `POOL_FACTORY_INITIALISED == true`: validates inputs, deposits funds, then delegates to `pool_factory::OnRequestPoolCreation` via `WasmMsg::Execute`. Pre-initialisation chains continue to use the in-Factory code path
- [factory] SC-4 Slice 2: `AddLiquidity` user-facing handler shrinks to a thin stub when `POOL_FACTORY_INITIALISED == true`: validates inputs, deposits each non-voucher token to escrow up-front, then delegates to `pool_factory::OnAddLiquidity`. Pre-initialisation chains continue to use the in-Factory code path. Funds now land in escrow before any pool-state mutation; ack-failure refunds release them back through `ProxyReleaseEscrow`
- [factory] SC-4 Slice 3: `RemoveLiquidity` (cw20 hook) shrinks to a thin stub when `POOL_FACTORY_INITIALISED == true`: validates inputs and holds the LP cw20 tokens (they arrived via the `cw20::Send` hook), then delegates to `pool_factory::OnRemoveLiquidity`. Pre-initialisation chains continue to use the in-Factory code path. The delegated path emits `method=remove_liquidity_request_delegated` to distinguish it from the legacy `method=remove_liquidity_request` for indexer telemetry
- [factory] SC-4 Slice 4: `RequestConcentratedPoolCreation` shrinks to a thin stub when `POOL_FACTORY_INITIALISED == true`: validates fee/spacing, slippage, pair, and fund custody, then delegates to `pool_factory::OnRequestConcentratedPoolCreation`. Pre-initialisation chains continue to use the in-Factory code path. The delegated path emits `method=request_concentrated_pool_creation_delegated` to distinguish it from the legacy `method=request_concentrated_pool_creation` for indexer telemetry. Per-token escrow funding and the position-NFT mint remain main-factory-side carry-overs while the bridge pattern lands in Slice 5+
- [factory] `reusable_internal_ack_call` forwards pool-related ack variants to `pool_factory::OnPoolAck` when pool factory is initialised; non-pool variants unchanged. Slice 2 adds `AddLiquidity` to the forwarded set; Slice 3 adds `RemoveLiquidity`; Slice 4 adds `RequestConcentratedPoolCreation`
- [factory] SC-4 reply-data amendment PR B: `RequestPoolCreation` delegate SubMsg in `execute_request_pool_creation` switched from fire-and-forget `SubMsg::new` to `SubMsg::reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`. Pool factory now returns the outbound packet via `Response::data`; main factory's reply handler unwraps the `MsgExecuteContractResponse` envelope and dispatches the packet. The `method=request_pool_creation_delegated` attribute is unchanged
- [pool_factory] SC-4 reply-data amendment PR B: `on_request_pool_creation` no longer emits a `FactoryExecuteMsg::ProxySendPacket` submsg; instead it returns `Response::data` typed as `PoolFactoryReply::SendPacket { msg, timeout, ack_response, sender }`. Attributes (`method`, `tx_id`) are preserved
- [factory] SC-4 reply-data amendment PR C: `add_liquidity_request_delegated` switches its pool_factory delegate SubMsg from fire-and-forget `SubMsg::new` to `SubMsg::reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`. Escrow-deposit submessages and the `method=add_liquidity_request_delegated` attribute are unchanged; funds still land in escrow before the IBC packet is sent
- [pool_factory] SC-4 reply-data amendment PR C: `on_add_liquidity` no longer emits a `FactoryExecuteMsg::ProxySendPacket` submsg; instead it returns `Response::data` typed as `PoolFactoryReply::SendPacket` carrying the outbound `RouterCrossChainExecuteMsg::AddLiquidity` packet. Attributes (`method`, `tx_id`) are preserved
- [factory] SC-4 reply-data amendment PR D: `remove_liquidity_request` (cw20 hook) switches its pool_factory delegate SubMsg from fire-and-forget `SubMsg::new` to `SubMsg::reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`. LP cw20 custody and the `method=remove_liquidity_request_delegated` attribute are unchanged
- [pool_factory] SC-4 reply-data amendment PR D: `on_remove_liquidity` no longer emits a `FactoryExecuteMsg::ProxySendPacket` submsg; instead it returns `Response::data` typed as `PoolFactoryReply::SendPacket` carrying the outbound `RouterCrossChainExecuteMsg::RemoveLiquidity` packet. CP module no longer imports `FactoryExecuteMsg` — all three CP handlers (`on_request_pool_creation`, `on_add_liquidity`, `on_remove_liquidity`) now use the reply-data pattern

### Added (existing items continue below)

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
- [router] CLP add liquidity now normalizes raw token amounts to voucher decimals before approve and VLP dispatch
- [router] Removed deprecated `ESCROW_BALANCES` writes from CLP add liquidity path (escrow managed by virtual_balance)

### Deprecated

- [router] `ESCROW_BALANCES` storage definition kept for backward compatibility, all reads and writes removed
- [router] `TOKEN_DENOMS` storage deprecated (moved to virtual_balance)
- [router] `GetAllEscrows` query (exists only for migration, not for production use)
- [virtual_balance] `NormalizeBalanceKeys` execute message removed (migration function no longer needed)
