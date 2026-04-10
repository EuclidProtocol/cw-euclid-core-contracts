# Solidity Changes for CLP Integration (Factory + Liquidity Only)

## Scope

This document specifies what must be implemented on the **EVM side** to match the current CLP architecture in this repo.

- **In scope**: EVM `Factory` and EVM liquidity token contracts.
- **Out of scope**: Hub/VSL logic (`router`, `concentrated_vlp`) on CosmWasm.
- Assumption: VSL (hub) remains CosmWasm and already supports CLP lifecycle.

---

## 1) Data Model Parity Required on EVM

Your EVM factory must mirror the same logical types used by CosmWasm messages:

1. `PoolType`
   - `ConstantProduct`
   - `Stable`
   - `Concentrated { fee_tier_bps, tick_spacing }`
2. `PoolKey`
   - `pair` + `pool_type`
3. `NextSwapPair`
   - `token_in`
   - `token_out`
   - `pool_key: optional` (required for CLP hop selection)
4. `CrossChainUser`
   - `chain_uid`
   - `address` (must be lowercase for compatibility with current validation behavior)

Notes:

- Pair ordering must be canonical (same logic as `Pair::new` in CosmWasm).
- `position_id` and `liquidity_delta` are `uint128` semantics end-to-end.

---

## 2) Factory Contract Changes (EVM)

## 2.1 New/Updated Storage

Add the equivalent of:

1. `pairToVlp` (legacy CP/Stable routing)
2. `poolKeyToVlp` (CLP routing by fee tier + spacing)
3. `positionIdToMetadata`
   - owner
   - poolKey
   - liquidity (uint128)
   - vlpAddress
4. `ownerToPositions` (owner => list of `position_id`)
5. Pending request maps keyed by `(sender, tx_id)` for:
   - concentrated pool create
   - concentrated add liquidity
   - concentrated remove liquidity
   - concentrated collect fees
   - concentrated collect protocol fees

Keep existing CP/Stable storage unchanged.

## 2.2 New Factory Endpoints

Implement these CLP endpoints (same logical behavior as Cosmos factory):

1. `requestConcentratedPoolCreation`
2. `addConcentratedLiquidity`
3. `removeConcentratedLiquidity`
4. `collectConcentratedFees`
5. `collectConcentratedProtocolFees` (admin-only)

Also update swap endpoint path:

6. existing `executeSwapRequest` must support CLP hops via `NextSwapPair.pool_key`.

## 2.3 Validation Rules (Must Match Current Architecture)

1. Fee tier + tick spacing whitelist:
   - `(100,1)`, `(500,10)`, `(3000,60)`, `(10000,200)`.
2. Tick validation on add:
   - `lower < upper`
   - both aligned to `tick_spacing`.
3. PoolKey validation:
   - hop pair must match `pool_key.pair`.
   - `pool_key.pool_type` must be concentrated when used for CLP.
4. Ownership checks:
   - remove/collect position fees must be position owner.
5. Protocol fee collect:
   - caller must be factory admin.
   - at least one requested amount > 0.
6. Collect pass-through fallback:
   - if local position metadata is missing, allow request pass-through (VLP remains source of truth).

## 2.4 Swap Routing Update (Critical)

`NextSwapPair.pool_key` behavior must match current semantics:

1. If `pool_key` is set:
   - resolve hop using `poolKeyToVlp` only.
   - reject if missing/unregistered/mismatched pair.
2. If `pool_key` is not set:
   - resolve using classic pair map only (`pairToVlp`).
   - do not auto-select CLP.

This preserves backward compatibility while enabling explicit CLP tier selection.

---

## 3) Cross-Chain Message Compatibility (Factory -> VSL Router)

EVM factory must emit payloads equivalent to these router cross-chain execute messages:

1. `RequestConcentratedPoolCreation`
2. `AddConcentratedLiquidity`
3. `RemoveConcentratedLiquidity`
4. `CollectConcentratedFees`
5. `CollectConcentratedProtocolFees`
6. `Swap` (with `NextSwapPair[]` carrying optional `pool_key`)

Field names and semantics must match current definitions in:

- `/Users/georgeschouchani/Documents/coding/cw-contracts/cw-euclid-core-contracts/packages/euclid_ibc/src/router_ibc.rs`
- `/Users/georgeschouchani/Documents/coding/cw-contracts/cw-euclid-core-contracts/packages/euclid/src/msgs/factory/msg.rs`
- `/Users/georgeschouchani/Documents/coding/cw-contracts/cw-euclid-core-contracts/packages/euclid/src/swap.rs`

Important compatibility detail:

- remove concentrated still uses `lp_allocation` naming on factory-side messages (alias semantics for liquidity delta).

---

## 4) Ack Handling Required on EVM Factory

Ack envelope semantics must match:

- `AcknowledgementMsg::Ok(payload)` / `AcknowledgementMsg::Error(error)`.

For CLP paths, parse these payloads:

1. `ConcentratedAddLiquidityResponse`
2. `ConcentratedRemoveLiquidityResponse`
3. `ConcentratedCollectFeesResponse`
4. `ConcentratedCollectProtocolFeesResponse`

Compatibility note:

- Concentrated pool creation ack currently returns `ConcentratedAddLiquidityResponse` shape (initial position + liquidity delta), so your create-pool ack parser must accept that payload.

Expected state transitions:

1. **Pool create ack success**
   - register `poolKeyToVlp`.
   - create initial `positionId` metadata.
   - add owner index.
   - mint position NFT.
2. **Add ack success**
   - if position exists: increase liquidity.
   - else: create metadata + owner index + mint NFT.
3. **Remove ack success**
   - subtract liquidity.
   - if position goes to zero: remove metadata/index and burn NFT.
4. **Collect fee/protocol ack success**
   - verify pool/position consistency with pending request.
5. **Idempotency**
   - if pending request missing on ack replay, return success no-op (do not mutate state).
6. **Error ack**
   - clear pending entry.
   - apply refund behavior only where funds were escrowed by request type.

---

## 5) Liquidity Token Contract Changes on EVM

## 5.1 CLP Position Token (Required)

Create a non-fungible position token contract (ERC721-style) for CLP positions.

Minimum required functions:

1. `mint(tokenId, owner, tokenURI)` (factory-authorized)
2. `burn(tokenId)` (factory-authorized or policy-defined)
3. `ownerOf(tokenId)`
4. optional metadata query/accessors.

Required integration:

1. Factory stores `positionTokenContract` address.
2. Mint on first position creation (pool create/add).
3. Burn when full remove reaches zero liquidity.

Ownership sync note:

- Current factory authorization is based on factory position metadata (`positionId -> owner`), not NFT `ownerOf` lookups. If your NFT is transferable, you must also update factory metadata on transfer; otherwise make transfers restricted/disabled.

## 5.2 Existing Fungible LP Token (No CLP Change)

Do not replace CP/Stable LP ERC20 behavior.

- CP/Stable keep fungible LP token flow.
- CLP uses position NFT flow.

---

## 6) Practical Encoding Notes for EVM

1. Use canonical pair ordering before building `PoolKey` identifiers.
2. Use deterministic `poolKey` hashing/encoding for local map keys.
3. Ensure addresses in `CrossChainUser` are lowercase before forwarding.
4. Keep `uint128` bounds where CosmWasm expects `Uint128`.
5. Preserve exact field names in cross-chain payloads for relayer compatibility.

---

## 7) Minimum EVM Test Matrix (Must Pass)

1. Create same pair with two CLP fee tiers -> different `PoolKey` and VLP mapping.
2. Add liquidity mints position NFT and metadata.
3. Increase liquidity on same position updates liquidity only.
4. Partial remove keeps NFT; full remove burns NFT.
5. Unauthorized remove/collect rejected.
6. Protocol fee collect non-admin rejected; admin path succeeds.
7. Mixed multihop route (`Stable -> CLP -> CP`) executes with explicit CLP `pool_key`.
8. Missing `pool_key` does not route through CLP.
9. Quote/simulation parity with execution on CLP and mixed routes.
10. Ack replay is idempotent and does not double-apply state.

---

## 8) What You Do NOT Need to Change

1. CosmWasm hub router and VSL CLP contracts.
2. Hub-side tick/swap engine behavior.
3. Existing CP/Stable swap/liquidity semantics.

This keeps the EVM work focused on factory-side request construction, ack handling, and position token lifecycle.
