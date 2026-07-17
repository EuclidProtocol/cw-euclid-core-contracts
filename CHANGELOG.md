# Changelog

All notable changes to Euclid core contracts are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Only contract and package changes are tracked (not test or CI changes). Each release is named after a star and carries a status: **in progress**, **freezed**, or **released**.

## Sirius (in progress)

### Added

#### Contracts

- [router] `RegisterFactory` accepts the new `RegisterFactoryChainType::Tvm` variant, storing registered TRON chains as `ChainType::Tvm` (first-class TRON chain type; previously TRON reused `Evm`). Same lowercase 0x-hex factory-address rule as EVM
- [meta_transaction] TVM meta-transactions verify via the EVM path (secp256k1 + keccak256, 20-byte hex address derivation); TRON Base58Check is a client-side display encoding and never enters the contracts
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
- [all] `GetBuildInfo {}` query on every contract, returning the embedded build provenance (`contract_version`, `build_commit`, `build_time`) so a deployed artifact can be traced back to its source revision

#### Packages

- [euclid] `cross-vm` feature: `CwExecuteFns` / `CwQueryFns` derives on all `msgs` Execute/Query enums, generating per-contract typed handle traits (`FactoryExecuteFns`, `RouterQueryFns`, etc.) for the cross-VM testing harness
- [euclid] `cross-vm` feature: `#[payable]` markers on `factory::ExecuteMsg::DepositToken` and `ExecuteSwapRequest` so the generated cross-VM typed handles take a trailing `funds` argument (mirrors the existing `cw_orch(payable)` markers; wasm/non-cross-vm builds are unaffected)
- [euclid] First-class TRON chain type: `chain::ChainType::Tvm(TvmChain)` (stored) and `msgs::router::execute::RegisterFactoryChainType::Tvm(RegisterFactoryChainTvm)` (message), plus `Chain::is_tvm()` / `Chain::tvm_info()` and `get_chain_type_str()` → `"tvm"`. TVM mirrors EVM address/signature mechanics; the distinction is the type tag, the `"tvm"` event string, and the `.chain_type.tvm.*` register key path
- [euclid] `factory::ManageFactoryState::ResetRegistration {}` variant — migration_admin entry to clear the factory `REGISTERED` flag for re-registration recovery (SC-32 review B2)
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
- [euclid-ibc] SC-4 reply-data amendment PR A: `is_pool_variant()` method centralising the pool-variant matcher so main factory's inbound ack dispatcher and the new outbound reply handler share a single source of truth. Introduced on `RouterCrossChainExecuteMsg`; carried over onto `RouterReceiveMsg` when the legacy enum was deleted
- [factory] SC-4 reply-data amendment PR A: `POOL_FACTORY_DELEGATE_REPLY_ID` reply id and `on_pool_factory_delegate_reply` handler — decodes `PoolFactoryReply::SendPacket` from a successful submsg's data, validates the inner `RouterCrossChainExecuteMsg` is a pool variant (defence in depth), and dispatches via the existing `to_msg` flow. Additive: `ProxySendPacket` remains in place until PR E retires it
- [euclid] New `build_info` module (plus a `build.rs`) embedding `BUILD_COMMIT` (short git sha, suffixed `(dirty)`) and `BUILD_TIME` (commit timestamp, ISO-8601) into every artifact at compile time, exposed via `BuildInfoResponse` and `build_info()`. Values are taken from the `BUILD_COMMIT` / `BUILD_TIME` env vars when set (the Docker optimizer path, where `.git` is not visible) and otherwise read from git. Commit timestamp (not wall-clock) is used so wasm checksums stay reproducible
- [euclid-utils][relayer] Each package's `QueryMsg` gains a `GetBuildInfo {}` variant returning `euclid::build_info::BuildInfoResponse`
- [euclid-encoding] New package at `packages/euclid_encoding`: a pure codec crate with no euclid dependencies, exposing only the generic JSON and ABI encode/decode primitives (`JsonEncode`/`JsonDecode`, `AbiEncode`/`AbiDecode`, `AbiMap`, and the `encode`/`decode` dispatch fns) plus `Encoding` and wire protocol version `0.0.1` (`PROTOCOL_VERSION`). The domain-facing wire message types crossing the hub/factory boundary (`RouterCrossChainExecuteMsg`, `FactoryCrossChainExecuteMsg`, `AcknowledgementMsg<S>`, and shared domain types) live in `euclid_ibc::wire`, which depends on this crate for the codec. Also ships the `SendPacketEncoded` event spec (`docs/send-packet-encoded.md`); no contract wiring yet, codec and documentation only
- [euclid-ibc] New `wire` module owning the cross chain wire messages. Per-domain `*SendMsg` and `*AckMsg` types, plus `RouterSendMsg` and `FactorySendMsg` envelopes, with `From` conversions from the existing domain structs and an `IntoAck` trait for building acknowledgements from domain responses. Wire bytes, ABI tags, and `PROTOCOL_VERSION` are unchanged. The envelopes were later renamed to `RouterReceiveMsg` and `FactoryReceiveMsg` (receiver perspective; a receiving contract decodes its own inbound wire type, it does not decode what it sends), and then became the sole cross-chain message representation: `RouterCrossChainExecuteMsg` and `FactoryCrossChainExecuteMsg` (`packages/euclid_ibc/src/router_ibc.rs` and `factory_ibc.rs`) are deleted, with `to_msg()` and the other domain helpers moved directly onto `RouterReceiveMsg`/`FactoryReceiveMsg`. JSON wire bytes are unchanged by both the rename and the deletion
- [euclid-ibc] `PendingPacket` gains a `#[serde(default)] encoding: u8` field (`Encoding::as_u8` of the leg) so the pending-send record can recall which wire encoding produced `original_msg`. Every constructor writes `encoding: 0` (Json) for now; in-flight packets stored before this change deserialize as 0 via the serde default. No behavior change yet, groundwork for threading real per-leg encoding through router/factory pending-send state
- [euclid-ibc] (Amendment B) `PendingPacket` gains a `#[serde(default)] wire_msg: Binary` field holding the exact wire bytes emitted in the send event, committed by both `execute_send_packet` handlers so the acknowledgement path can byte-check the relayer-supplied msg against what was sent. The send handlers now encode before the pending-packet write, and `create_pending_packet_and_update_sequence` takes a trailing `wire_msg: Binary` on both router and factory. On Json legs `wire_msg` duplicates `original_msg`. The native reply-queue constructors write `Binary::default()` (the native path never reaches the relayer ack byte check). Packets stored before this change deserialize with an empty `wire_msg`, which then fails the byte check; drain in-flight packets before upgrading
- [euclid] Encoded packet event helpers `send_packet_encoded_event` and `write_acknowledgement_encoded_event`, emitting `euclid-send-packet-encoded` and `euclid-write-acknowledgement-encoded`. Both carry a `version` (`0.0.1`) and an `encoding` attribute. Attributes are emitted in the canonical cross-VM order, matching the Solidity event field order so one indexer schema reads both VMs: the send event emits eight attributes as `msg`, `sequence`, `source_port`, `destination_port`, `timeout`, `destination_chain_type`, `version`, `encoding`. The write-acknowledgement is a single complete event emitted once from the cross-chain receive reply, with nine attributes as `msg`, `sequence`, `source_port`, `destination_port`, `ack`, `destination_chain_type`, `ack_type`, `version`, `encoding` (ports swapped versus the incoming packet, so the acknowledging contract is `source_port`). These replace the legacy `send_packet_event` / `write_acknowledgement_event`
- [euclid-ibc] New `wire::transcode` module bridging the internal JSON domain messages and the leg wire encoding at the four cross-chain boundaries (send emit, receive decode, ack emit, ack decode). The Json arm is a byte passthrough; the Abi arm routes through `euclid-encoding`. `wire_attr_string` renders the `msg`/`ack` event attributes as raw JSON text on Json legs and as base64 on Abi legs. `PendingPacket.encoding` is now populated with the real per-leg encoding rather than always 0. Following the envelope rename, `decode_router_send`/`decode_factory_send` are renamed to `decode_router_receive`/`decode_factory_receive`, returning `RouterReceiveMsg`/`FactoryReceiveMsg` directly; the `encode_router_send`/`encode_factory_send` and `router_send_tag`/`factory_send_tag` helpers are removed now that a typed value encodes via `euclid_encoding::encode` directly and each envelope exposes its own `wire_tag()`
- [euclid-ibc] Added `SingleSidedAddLiquidityAckMsg`, a dedicated ack wire type for the single-sided add-liquidity path
- [euclid-ibc] Breaking package change: `RouterCrossChainExecuteMsg` and `FactoryCrossChainExecuteMsg`, and the `router_ibc`/`factory_ibc` modules that defined them, are deleted. `RouterReceiveMsg`/`FactoryReceiveMsg` (the former `RouterSendMsg`/`FactorySendMsg`) are now the only cross-chain message enums; every caller that matched on the legacy enums or imported `router_ibc`/`factory_ibc` must move to the wire envelopes. JSON serialization is unchanged, so on-wire bytes and stored `PendingPacket.original_msg` are unaffected
- [euclid-encoding] (Amendment B) New `repr` module with `to_transport_string`/`from_transport_string`, converting wire bytes to and from the relayer facing representation: raw JSON text for a Json (`encoding: 0`) leg, `0x` prefixed lowercase hex for an Abi (`encoding: 1`) leg. Decode is strict, requiring the `0x` prefix (`hex::decode` tolerates mixed case on input; emit is always lowercase). Both directions fail into a single new `EncodingError::InvalidRepresentation { expected, reason }` variant, covering the hex parse and UTF-8 decode failure modes. Pulls in the `hex` 0.4 dependency via a targeted lockfile addition; the `ruint 1.16.0` and `time 0.3.41` pins are reasserted, not disturbed
- [euclid] (Amendment B) New `ContractError::PacketMsgMismatch { sequence: u128 }` variant, raised by the router and factory acknowledgement handlers when the relayer supplied `AcknowledgePacket.msg` bytes do not byte match the wire bytes committed in `PendingPacket.wire_msg` at send time. Same error name as the Solidity mirror

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
- [factory] SC-4 reply-data amendment PR E: `request_concentrated_pool_creation` delegate SubMsg in `execute_request_concentrated_pool_creation` switched from fire-and-forget `SubMsg::new` to `SubMsg::reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`. The `method=request_concentrated_pool_creation_delegated` attribute is unchanged
- [pool_factory] SC-4 reply-data amendment PR E: `on_request_concentrated_pool_creation` no longer emits a `FactoryExecuteMsg::ProxySendPacket` submsg; instead it returns `Response::data` typed as `PoolFactoryReply::SendPacket` carrying the outbound `RouterCrossChainExecuteMsg::RequestConcentratedPoolCreation` packet. The CLP module no longer imports `FactoryExecuteMsg`; all four landed pool factory handlers now use the reply-data pattern exclusively
- [solidity/factory] SC-34 alignment: every pool entrypoint that takes a token pair (`request_pool_creation`, `request_concentrated_pool_creation`, `add_liquidity`, `add_concentrated_liquidity`, `add_single_sided_liquidity`, `remove_liquidity`) now canonicalizes the pair via the new `PoolLib.toCanonical` — it sorts the two tokens into lexicographically ascending order (mirroring the CosmWasm `Pair::new` invariant) instead of rejecting a non-canonical pair with `TokenLib.InvalidPairOrder()`. Behavioral change to the Solidity factory surface: a reversed pair is now accepted and resolves to the same pool/VLP the canonical order would; an *equal* pair (token paired with itself) still reverts `InvalidPairOrder()`. Token-id and token-type validation is unchanged
- [solidity/factory] SC-34 alignment: the concentrated pool-creation path (`ConcentratedPoolCreationLib`) now reverts the shared `IPoolErrors.PoolAlreadyExists()` custom error on a duplicate pool, matching the classic creation path and the CosmWasm `PoolAlreadyExists` surface — previously it reverted a `ContractError("Pool already exists")` string. `PoolAlreadyExists` is now declared once in `IPoolErrors` and inherited by both paths
- [factory] `AddConcentratedLiquidity` now accepts a zero amount on one token leg: a concentrated position entirely on one side of the current price needs only that side's token, and callers previously had to send 1 micro unit of the unused token to satisfy the per-token `ZeroAssetAmount` guard. The guard is relaxed to "at least one leg nonzero" on this path only (pool creation and classic `AddLiquidity` still reject any zero leg); zero legs skip the fund transfer on request, the escrow forward on the success ack, and the refund on the error ack. The Solidity factory relaxes its mirror guard in the same change
- [router] `ibc_execute_add_concentrated_liquidity` skips the virtual-balance `Mint` and `Approve` messages for a zero token leg (the virtual balance contract rejects zero-amount operations), so a one-sided add with a true-zero unused leg settles instead of error-acking
- [concentrated_vlp] `AddLiquidity` skips the voucher transfer for a zero token leg; the existing at-least-one-leg-nonzero guard, zero-provided slippage pass, and zero-guarded refunds are unchanged
- [factory][pool_factory] LP cw20 tokens are now always instantiated with `LP_TOKEN_DECIMAL` (18) decimals, pinned by the factory rather than the LP token contract. `execute_request_pool_creation` and `on_request_pool_creation` no longer thread a caller-supplied decimal; both build the intermediate cw20 `InstantiateMsg` with `decimals: LP_TOKEN_DECIMAL as u8`, and the factory ack handler copies that stored `18` back into the LP `InstantiateMsg`. The `lp_token` contract stays a flexible cw20 whose `InstantiateMsg` keeps its `decimals: u8` field (mirroring Solidity, where the factory passes `TokenLib.LP_TOKEN_DECIMALS` into `LPToken.InitializeParams.decimals`). Because 18 is within cw20-base's `decimals <= 18` limit, `execute_request_pool_creation` keeps validating the LP `InstantiateMsg` with stock `cw20_base::msg::InstantiateMsg::validate`, and `lp_token::instantiate` keeps delegating to stock `cw20_base::contract::instantiate` (no state-writing bypass)
- [router] Cross-chain packet events replaced with versioned encoded successors, no dual-emit window. `euclid-send-packet` becomes `euclid-send-packet-encoded` and `euclid-write-acknowledgement` becomes `euclid-write-acknowledgement-encoded`, each gaining `version` (`0.0.1`) and `encoding` attributes. The write-acknowledgement is emitted exactly once per receive, complete and self describing, from the cross-chain receive reply (`InFlightReceive` carries the ports, wire msg, sequence, and destination chain type across the submessage boundary); the old two-part header/payload pattern is gone and it carries an `ack_type` (`success` or `error`) so bridges classify acks without decoding. Attribute order is canonical across both VMs and matches the Solidity event field order, payload first: the send event emits `msg`, `sequence`, `source_port`, `destination_port`, `timeout`, `destination_chain_type`, `version`, `encoding`, and the write-acknowledgement emits `msg`, `sequence`, `source_port`, `destination_port`, `ack`, `destination_chain_type`, `ack_type`, `version`, `encoding`. The `msg` and `ack` attributes now render as raw JSON text on Json legs (`encoding` 0) and as base64 only on Abi legs (`encoding` 1), a change from the previous always-base64 rendering. Backend indexers keying on the old event names, the old attribute order, the two-part pattern, or the base64 rendering must update
- [router] `ExecuteMsg::ReceivePacket` gains an `encoding` field; the relayer copies it from the send event so the hub decodes the leg's declared encoding (both Json and Abi accepted inbound)
- [router] Outbound sends and inbound acks transcode at the boundary through `euclid_ibc::wire::transcode`; internal `RouterCrossChainExecuteMsg` plumbing stays JSON, the wire encoding is applied only at send-event emission, receive decode, ack emission, and ack decode. Packet payloads ride the leg encoding chosen by the factory chain type
- [factory] Same cross-chain event replacement as [router]: `euclid-send-packet-encoded` and the single complete `euclid-write-acknowledgement-encoded` emitted once from the receive reply (with `ack_type`, `version`, `encoding`), the same canonical cross-VM attribute order, raw-JSON-text `msg`/`ack` on Json legs and base64 on Abi legs, the new `ReceivePacket.encoding` field, and boundary transcode via `euclid_ibc::wire::transcode`. No dual-emit window; backend indexers must update
- [router] Operator note: `PendingPacket.encoding` defaults to 0 (Json) via serde for packets stored before this upgrade, so an in-flight packet on an Abi leg decodes wrong once the router expects the real per-leg encoding. Drain in-flight packets on EVM and TVM chains before upgrading a CosmWasm hub or factory, matching the equivalent Solidity note that in-flight packets sent before an upgrade cannot settle after it
- [router] (Amendment B) `ExecuteMsg::ReceivePacket.msg` and `ExecuteMsg::AcknowledgePacket.msg`/`.ack` change from `Binary` to `String`; the JSON schema field stays a string, only the content rule changes: raw JSON text on a Json leg, `0x` prefixed lowercase hex on an Abi leg. The `msg`/`ack` attributes on `euclid-send-packet-encoded` and `euclid-write-acknowledgement-encoded` follow the same rule, so an Abi leg attribute changes from base64 to `0x` lowercase hex (Json legs already rendered raw JSON text and are unchanged). Backend indexers reading Abi leg attributes as base64 must update. `ReceivePacket` parses the incoming `String` with `from_transport_string` before decode; `AcknowledgePacket` carries no encoding field, so the handler loads the pending packet by sequence first and resolves the representation from the stored `PendingPacket.encoding`, parsing only after that lookup. `ReceivePacketInternalCallback`, the reply queue, the native path, `PendingPacket` storage, and `InFlightReceive.msg` are unaffected and stay `Binary`. Ships in the same lockstep cutover as the earlier event rename; the production relayer must send the new `String` forms
- [factory] (Amendment B) Same transport representation change as [router]: `ExecuteMsg::ReceivePacket.msg` and `ExecuteMsg::AcknowledgePacket.msg`/`.ack` are `String`, the `msg`/`ack` event attributes render as `0x` lowercase hex on Abi legs instead of base64, and `AcknowledgePacket` resolves its leg encoding from the loaded `PendingPacket` before parsing. Same lockstep relayer cutover as [router]
- [router] Internal `InFlightReceive.destination_chain_type` renamed to `source_chain_type` for clarity: the field records the incoming packet's source chain type, not a destination. The emitted `destination_chain_type` acknowledgement event attribute is unchanged. The factory's `InFlightReceive` never carried the field (its counterparty is always the hub), only its doc comment was clarified

#### Packages

- [euclid_pool] SC-4 pool-function split: `pool_functions.rs` replaced by per-pool-type modules. `euclid_pool::cp` owns constant-product math and operations (`calculate_cp_swap`, `calculate_lp_allocation`, `add_liquidity`, `pre_swap`, `execute_swap`, `simulate_swap`); `euclid_pool::stable` owns the StableSwap equivalents with a required `amp_factor: Uint64`; `euclid_pool::common` keeps the pool-type-agnostic surface (`register_pool`, `remove_liquidity`, `assert_slippage_tolerance`, `update_fee`, `update_amp_factor`, `update_admin`, `calculate_amount_from_shares`, `PreSwapResponse`, `SwapResult`, `MINIMUM_LIQUIDITY`). Contracts now call dedicated per-type functions instead of one reusable function that branched internally
- [euclid_pool] `register_pool` no longer takes a `PoolConfig` argument; pool-type event attributes (`pool_type`, `amp_factor`, `fee_tier_bps`, `tick_spacing`) are emitted by the calling VLP after `register_pool` returns. Emitted attributes are unchanged from before
- [cp_vlp] swap/liquidity/registration call sites moved to `euclid_pool::cp::*` / `euclid_pool::common::*`; `SwapCalculationMethod::Regular` discriminant removed from call sites
- [stable_vlp] swap/liquidity/registration call sites moved to `euclid_pool::stable::*` / `euclid_pool::common::*`; `SwapCalculationMethod::Stable(amp)` and the `Some(amp_factor)` add-liquidity discriminant removed — `amp_factor` is now a plain required argument
- [concentrated_vlp] registration and CP-style simulate-swap call sites moved to `euclid_pool::common::*` / `euclid_pool::cp::*`
- [euclid] build provenance is now persisted to contract state. `set_build_info` writes a `StoredBuildInfo { build_commit, build_time }` item (`euclid_build_info`) into state; every contract calls it from `instantiate` and `migrate` alongside `set_contract_version`. `build_info(storage, contract_version)` now reads the persisted values (falling back to the compile-time constants when unset), so `GetBuildInfo {}` reports the build a contract was deployed/migrated from rather than the constants of whatever code answers the query
- chore(deps): bump cross-vm-macros / cross-vm-cosmwasm to 4ece484 (dyn ops are the only operation style upstream; derives unchanged)
- chore(deps): bump cross-vm-macros / cross-vm-cosmwasm to 491fe3c (introspection-driven CLI describe + opt-in op help; framework chain presets now gated behind a `presets` feature)
- [euclid-encoding] ABI decode is now fully canonical. Every decode routes through the new `decode_params_canonical` entry: alloy's validating decoder (rejecting dirty fixed-width integer words, invalid UTF-8, and out-of-range offsets) followed by a re-encode byte-equality check that requires the decoded value's canonical encoding to reproduce the input bytes exactly, rejecting anything else as `NonCanonicalEncoding` (non-canonical bool words, gapped or overlapping tail offsets, and any other laundered layout). `None` composite options carrying a non-empty payload are rejected (`NonEmptyPayload`), and `None` primitive options carrying a non-default value are rejected via the new `opt_prim_from_sol` helper (`NonCanonicalOption`). All `euclid_ibc::wire` nested tagged-payload, option, and envelope ack decodes route through the same canonical entry
- [euclid] `lp_token::InstantiateMsg` keeps its `decimals: u8` field and `From<InstantiateMsg> for Cw20InstantiateMsg` forwards `msg.decimals`, so the LP token contract stays a flexible cw20. The factory is the only caller that pins the value to `LP_TOKEN_DECIMAL` (18)
- [euclid] new `LP_TOKEN_DECIMAL` (18) constant for LP cw20 decimals, set to 18 because cw20-base's `InstantiateMsg::validate` caps decimals at 18 (rather than the 24-decimal voucher precision)
- [euclid] (Amendment B) Breaking package change: `router::ExecuteMsg::ReceivePacket`/`AcknowledgePacket` and `factory::ExecuteMsg::ReceivePacket`/`AcknowledgePacket` change their `msg` field (and `AcknowledgePacket.ack`) from `Binary` to `String`. The JSON schema field stays a string; only the content rule changes, per the [router]/[factory] Changed entries above. Schemas are regenerated
- [euclid_ibc] The `ack` module moved into `wire::envelope::ack`; `AcknowledgementMsg` and `make_ack_fail` now import from `euclid_ibc::wire::envelope` (or `euclid_ibc::wire::envelope::ack` directly) instead of `euclid_ibc::ack`. The unused `make_ack_success`, `AcknowledgementMsg::unwrap`, and `AcknowledgementMsg::unwrap_err` helpers were removed; none had production call sites

### Fixed

#### Contracts

- [euclid-ibc] Wire transcode mapped router ack tags 4 and 5 (`RequestPoolCreation` and `RequestConcentratedPoolCreation`) to `RequestPoolCreationAckMsg` and `RequestConcentratedPoolCreationAckMsg`, but the hub router never acks a pool-creation packet with a pool-creation response. Creation chains into the initial liquidity add, so `on_add_liquidity_reply` sets the final reply data to `AcknowledgementMsg<AddLiquidityResponse>` (classic) or `<ConcentratedAddLiquidityResponse>` (concentrated), which the factory decodes. On an ABI leg the old table failed to parse the router's add-liquidity JSON. Tags 4 and 5 now map to `AddLiquidityAckMsg` and `AddConcentratedLiquidityAckMsg`. The two unused pool-creation wire ack mirrors (structs, `From` conversions, `AbiMap` impls, `IntoAck` entries, and the Solidity `AckWire.sol` / `AckCodec.sol` counterparts) were deleted; the send-side halves stay
- [euclid] `Token::KEY_ELEMS` was 42 and `Pair::KEY_ELEMS` was 42, though `Token::key()` emits one key segment and `Pair::key()` emits three. `cw-storage-plus` uses `KEY_ELEMS` to split a composite key into its halves, so any range query over a map keyed by `(Token, ..)` read length prefixes past the end of the key and panicked. The only such map today is the router's `RELEASE_FEES: Map<(Token, ChainUid), Uint256>`, which made `GetReleaseFees` panic as soon as one per-token release fee existed. Corrected to 1 and 3
- [euclid] Removed leftover `println!` debug statements from `Pair::from_vec`
- [euclid] `build.rs` embedded a stale `BUILD_COMMIT`/`BUILD_TIME` after any commit, amend, or rebase on the current branch. It declared `cargo:rerun-if-changed` on `$GIT_DIR/HEAD`, which on a branch holds the text `ref: refs/heads/<name>` and only changes when the branch itself is switched. The build script now also watches the ref that `HEAD` points at (loose file or `packed-refs`), resolved via `git rev-parse --git-path` so linked worktrees, whose refs live in the main `.git`, are handled. Paths are emitted only when they exist, since cargo reruns a build script unconditionally when a declared path is missing
- [factory] `validate_pool_creation_tokens` (shared by `RequestPoolCreation` and `RequestConcentratedPoolCreation`) now rejects any token leg with a zero amount (`ContractError::ZeroAssetAmount`). Voucher tokens skip the FundManager funding path, which was the only thing implicitly rejecting zero for native and smart tokens, so a voucher/voucher request with both amounts set to 0 previously passed source-side validation and was dispatched to the VSL, failing only there. The Solidity factory gains the mirror guard in the same change.
- [factory] Corrected the "two new tokens" pool-creation guard message (raised by `validate_pool_creation_tokens`, shared by classic `RequestPoolCreation` and `RequestConcentratedPoolCreation`) from "Cannot create pool two new tokens. Atleast one token must already be registered." to "Cannot create pool with two new tokens. At least one token must already be registered." — fixes a typo and makes the text identical to the Solidity factory's error
- [factory] `TransferVoucher` now rejects an empty `recipients` list synchronously (`ContractError::new("Recipients cannot be empty")`), matching the Solidity factory's `if (recipients.length == 0) revert`. Previously the empty-recipients packet was accepted and reached the hub, where the recipient loop never ran and the transfer settled as a no-op success ack — a cross-VM divergence in rejection surface (the EVM factory rejects locally). Both VMs now reject at the factory before any cross-chain round trip
- [factory] In-flight double factory registration: the factory now tracks a `REGISTERED` flag and rejects a second `RegisterFactory` ("Factory already registered"), which propagates to the router as an error ack. A chain is saved on the router only on the factory's success ack, and the router's submit-time guard checks only the settled `CHAIN_UID_TO_CHAIN` registry — so two admin `RegisterFactory` submits for one `chain_uid` issued before either ack settled both cleared the guard and both emitted packets, and the second ack could blindly overwrite the first. Making the factory the registration authority closes that window: the second packet is rejected at the factory, so the router never re-registers or overwrites (SC-32 review B2). The Solidity (EVM) factory now carries the equivalent `registered` flag for cross-VM alignment
- [factory] `ManageFactoryState::ResetRegistration {}` admin entry (migration_admin) clears the `REGISTERED` flag so the router can register a factory again. Recovery path for a registration that marked the factory registered on receive but never settled on the router (e.g. a lost success ack), which would otherwise leave the factory rejecting every retry forever. The duplicate guard itself is unchanged. The Solidity factory exposes the equivalent `reset_factory_registration()` (upgrade admin) plus an `is_registered()` view (SC-32 review B2)

### Security

- [factory] SC-4 reply-data amendment PR E: removed `factory::ExecuteMsg::ProxySendPacket` variant, the `execute_proxy_send_packet` handler, and `handle_proxy_send_packet` wrapper. Pool factory now communicates outbound IBC packets exclusively via `Response::data` typed as `PoolFactoryReply::SendPacket`, consumed by main factory's `on_pool_factory_delegate_reply`. The reply handler decodes the inner `RouterCrossChainExecuteMsg` and rejects any non-pool variant before dispatching (`is_pool_variant` check), removing the previously addressable `ProxySendPacket` execute surface as defence in depth
- [router] (Amendment B) `execute_receive_acknowledgement` now byte checks the relayer supplied `AcknowledgePacket.msg` against the wire bytes committed in `PendingPacket.wire_msg` at send time, rejecting a mismatch with `ContractError::PacketMsgMismatch { sequence }`. The check sits immediately after `remove_pending_packet_and_decrement_count` and the representation decode of the supplied `String`s, before the source port validation and the ack transcode, mirroring Solidity's keccak `PacketMsgMismatch` commitment. The obsolete comment excusing EVM intermediaries from re-encoding is removed; wire bytes are now canonical, though the handler still dispatches off the stored `original_msg` as defence in depth once the check passes. Packets stored before this change deserialize with an empty `wire_msg` and always fail the check; drain in-flight packets before upgrading
- [factory] (Amendment B) Same acknowledgement byte check as [router]: `execute_receive_acknowledgement` rejects a relayer supplied `AcknowledgePacket.msg` that does not byte match the stored `PendingPacket.wire_msg` with `ContractError::PacketMsgMismatch { sequence }`. Same in-flight drain requirement before upgrading

### Removed

- [factory] SC-4 reply-data amendment PR E: `ExecuteMsg::ProxySendPacket` variant (breaking change to the factory execute surface); `execute_proxy_send_packet` / `handle_proxy_send_packet` handlers in `factory/src/execute/proxy.rs`; the three associated unit tests (`test_proxy_send_packet_unauthorised_caller_rejected`, `test_proxy_send_packet_with_no_pool_factory_set_unauthorised`, `test_proxy_send_packet_authorised_caller_emits_submsg`)
- [euclid] SC-4 reply-data amendment PR E: `factory::ExecuteMsg::ProxySendPacket` variant removed from the shared message enum
- [euclid] Legacy packet event surface deleted from `events.rs`: the `EUCLID_SEND_PACKET_EVENT` and `EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT` consts and the `send_packet_event` / `write_acknowledgement_event` helpers, replaced by the encoded successors. `receive_packet_event`, `receive_acknowledgement_event`, and their consts are unchanged
- [euclid_pool] SC-4 pool-function split: `SwapCalculationMethod` enum removed (the per-type modules encode the swap curve directly); `pool_functions` module removed in favour of `cp` / `stable` / `common`; the `amp_factor: Option<Uint64>` parameter on `add_liquidity` removed (CP has no amp factor; stable takes it as a required `Uint64`)
- [euclid-encoding] Removed the dead `EncodingError::AbiEncode` and `EncodingError::OutOfRange` variants (constructed nowhere in the workspace; alloy ABI encoding is infallible) and dropped the unused `cosmwasm-schema` dependency
- [euclid] `lp_token_decimal: u8` removed from `factory::ExecuteMsg::RequestPoolCreation` and `pool_factory::ExecuteMsg::OnRequestPoolCreation`. Breaking change to the factory execute surface: callers can no longer supply LP token decimals, which the factory now fixes to `LP_TOKEN_DECIMAL` (18). `lp_token::InstantiateMsg` keeps its `decimals: u8` field (the contract remains a flexible cw20; only the factory pins 18)
- [euclid-ibc] (Amendment B) `wire::transcode::wire_attr_string` removed, superseded by `euclid_encoding::repr::to_transport_string`/`from_transport_string`, which now render the `msg`/`ack` event attributes and the new `String` entry point fields: raw JSON text on Json legs, `0x` lowercase hex on Abi legs (previously base64 on Abi legs)

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
- [router] `EUCLID_FEE_OVERRIDES: Map<(ChainUid, String), u64>` storing per-wallet Euclid-fee overrides keyed on the swapping `CrossChainUser` components
- [router] `ManageRouterState::SetEuclidFeeOverride { user, euclid_fee_bps }` fee-admin-gated handler: `Some(bps)` upserts (bounded by `MAX_FEE_BPS`), `None` removes
- [router] `GetEuclidFeeOverride { user } -> EuclidFeeOverrideResponse { euclid_fee_bps: Option<u64> }` query for backend/admin auditing
- [router] `helpers::euclid_fee_override::get_euclid_fee_override` shared resolver — single resolution point reused by execute and simulate paths so quote and execution cannot drift
- [orderbook_deposits] `NULLIFIERS` map tracking withdrawn amounts by hashed key
- [factory] `AddSingleSidedLiquidity` execute entry point: user deposits a single token, the hub atomically swaps a backend-computed portion and adds liquidity on the target VLP in one IBC roundtrip
- [factory] `AddSingleSidedLiquidity` supports `TokenType::Smart` (CW20) `asset_in` via the `IncreaseAllowance` + `TransferFrom` pattern (mirrors `add_liquidity_request`); `TokenType::Voucher` rejected as `UnreachableCode`
- [factory] `AddSingleSidedLiquidity` accepts an optional `partner_fee` (bounded by `MAX_PARTNER_FEE_BPS`); fee retained at the factory and routed to the recipient on ack success, refunded with `amount_in` on ack failure
- [factory] `PENDING_SINGLE_SIDED_LIQUIDITY` map carries `partner_fee_amount` and `partner_fee_recipient` for the ack handler
- [factory] `PendingSingleSidedLiquidity { user, pagination }` query returning in-flight single-sided requests for a user
- [router] `PENDING_SINGLE_SIDED_LIQUIDITY` map storing in-flight single-sided requests
- [router] `on_single_sided_swap_reply` and `on_single_sided_add_liquidity_reply` reply handlers chaining the internal swap -> add-liquidity flow with `min_lp_out` slippage enforcement
- [router] `ibc_execute_single_sided_add_liquidity` IBC receive handler validating the request and kicking off the swap submsg
- [euclid] `SingleSidedLiquidityRequest` pending-state struct (in `liquidity.rs`) carrying `partner_fee_amount` and `partner_fee_recipient` for the factory ack handler
- [euclid] `GetPendingSingleSidedLiquidityResponse { pending_single_sided_liquidity }` for the new query
- [euclid_ibc] `RouterCrossChainSingleSidedAddLiquidityMsg` IBC packet payload carrying `asset_in`, `amount_in`, `swap_amount`, target `pair`, `swaps` route, `min_lp_out`, and partner-fee fields
- [euclid] `ManageRouterState::SetEuclidFeeOverride` execute variant and `QueryMsg::GetEuclidFeeOverride` query variant plus `EuclidFeeOverrideResponse` response type for the per-wallet Euclid-fee override (SC-23)
- [euclid] `VlpSwapMsg.euclid_fee_override: Option<u64>` field (serde-defaulted) carrying the resolved per-wallet Euclid-fee override to the VLP (SC-23)
- [euclid] `VlpSimulateSwapMsg.euclid_fee_override: Option<u64>` field (serde-defaulted) carrying the resolved per-wallet Euclid-fee override through the simulation path so quotes match execution (SC-23)
- [euclid] `QuerySimulateSwap.sender: Option<CrossChainUser>` field (serde-defaulted) — optional swapping wallet so the router can resolve and apply its Euclid-fee override to the quote (SC-23)

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
- [euclid] `TxType::SingleSidedAddLiquidity` variant (display: `single_sided_add_liquidity`) emitted by the router on single-sided add-liquidity entry
- [euclid] `euclid_fee_override_change_event(action, user, euclid_fee_bps)` emitting `euclid-fee-override-change` with attributes: action (`set`/`remove`), chain_uid, address, euclid_fee_bps (omitted on remove)
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
- [router] Swap chokepoint (`ibc_execute_swap`) resolves the swapping wallet's Euclid-fee override and stamps it onto the outgoing `VlpSwapMsg`; native and IBC swaps both inherit it identically (SC-23)
- [euclid_pool] `pre_swap`/`execute_swap`/`simulate_swap` accept an `euclid_fee_override: Option<u64>`; when `Some(bps)` it replaces the pool's Euclid-fee rate (`Some(0)` = full exemption), LP fee always charged at the pool rate; `execute_swap` forwards the override to the next hop (SC-23)
- [router] Swap simulation (`query_simulate_swap`) resolves the optional `QuerySimulateSwap.sender`'s Euclid-fee override and threads it through the simulation, so a quote matches what execution charges; the execute-path slippage pre-check now simulates with the same resolved override (SC-23)
- [cp_vlp] / [stable_vlp] `query_simulate_swap` accepts and applies `euclid_fee_override`, forwarding it onto each next-hop `VlpSimulateSwapMsg` so the override applies on every simulated hop (SC-23)
- [concentrated_vlp] CLP swap execution and `SimulateSwap` apply `euclid_fee_override` by lowering the structural fee tier for the swap (new `resolve_effective_fee` helper), so a reduced/zero override actually improves the trader's quote instead of only shifting the protocol's cut to LPs; the LP keeps its absolute pip share of the tier (`lp_pips = tier_pips - tier_pips*d/10_000`) and only the protocol's slice (`tier_pips*X/10_000`) is waived; an override at/above the pool default is an exact no-op. The override is forwarded to downstream legs in both execution and simulation, so it applies on every hop of a multi-hop route that passes through a CLP (SC-23 Issue 8)
- [euclid_pool] Clarified the `pre_swap` deferred-CLP note: CLP overrides are applied in `concentrated_vlp::resolve_effective_fee`, not `pre_swap`, which stays cp/stable-only (SC-23 Issue 8)
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
- [euclid] `generate_tx` no longer embeds `block.height` or `transaction.index` in the `tx_id`. New format: `{sender}:{chain_id}:{nonce}`. Reorg replay now reproduces the same `tx_id`, so the ack-direction lookup in `PENDING_SWAPS` / `PENDING_REMOVE_LIQUIDITY` / `PENDING_RELEASE_VOUCHER` cannot miss its entry after a source reorg. Old in-flight entries written under the previous format remain valid (segment-count differs, no collision); no migration required.
- [euclid] `TX_NONCE: Item<u128>` replaced by `TX_NONCES: Map<String, u128>` keyed by `sender.to_sender_string()`. Each sender's nonce stream is now independent of all others, so cross-sender reordering during a reorg replay does not shift any individual sender's `tx_id`. Cosmos SDK's per-account sequence ordering guarantees that a single sender's own txs cannot reorder, making the determinism property hold under any realistic replay. Old `Item<u128>` at key `"tx_nonce"` is orphaned (zero reads/writes); no migration required.
- [escrow] cw20 `Send`-hook deposits no longer fail with `UnsupportedDenomination`: the `TokenAllowed` query now matches denoms decimals-agnostically via the new `TokenType::shallow_eq` instead of full `PartialEq`. Smart denoms are always registered with explicit decimals while callers (e.g. the cw20 `Send` hook) send `decimals: None`, and a `(chain, token)` denom is only ever registered with one decimals value — so comparing on type + denom is unambiguous. The hub still derives the canonical decimals from the registered token metadata. Supersedes the earlier `receive_cw20` decimals-hydration fix, which is reverted.
- [euclid] Added `TokenType::shallow_eq` (decimals-agnostic equality) and `TokenType::deep_eq` (full equality including decimals) to make the decimals distinction explicit at call sites.

### Deprecated

- [router] `ESCROW_BALANCES` storage definition kept for backward compatibility, all reads and writes removed
- [router] `TOKEN_DENOMS` storage deprecated (moved to virtual_balance)
- [router] `GetAllEscrows` query (exists only for migration, not for production use)
- [virtual_balance] `NormalizeBalanceKeys` execute message removed (migration function no longer needed)
