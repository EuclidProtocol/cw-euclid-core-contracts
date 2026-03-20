# Orderbook Deposits Withdraw (Exact Behavior)

This document describes the exact `Withdraw` flow implemented in
`contracts/hub/orderbook_deposits/src/execute.rs`. It covers validation steps,
permit signature requirements, Merkle proof verification, state updates, and
all data structures referenced by the withdraw logic.

## Entry Point

Withdraw is handled by `ExecuteMsg::Withdraw` in
`contracts/hub/orderbook_deposits/src/msg.rs` and dispatched by
`execute()` in `contracts/hub/orderbook_deposits/src/execute.rs`.

```
ExecuteMsg::Withdraw {
  root_id: String,
  amount: Uint256,
  nonce: u64,
  leaf: WithdrawalLeaf,
  proof: Vec<MerkleProofStep>,
  permit: Permit,
  destination_chain_uid: String,
  destination: String,
}
```

## Withdraw Validation (Exact Order)

`execute_withdraw()` performs the following checks and actions in order:

1. `amount` must be non-zero.
2. `destination_chain_uid` must be non-empty.
3. `destination` must be non-empty.
4. Contract status must be `OrderbookDepositsStatus::Active`.
5. Current root must exist in `CURRENT_ROOT`.
6. `root_id` must match the current root.
7. Current root hash length must be exactly 32 bytes.
8. `leaf.user` and `leaf.token_id` must be non-empty.
9. `leaf.token_id` must be whitelisted in `WHITELISTED_ASSETS`.
10. `Token::create(leaf.token_id)` must succeed (token id format validation).
11. Permit must be valid (see Permit Verification).
12. Permit must not be replayed (see Permit Replay Protection).
13. Merkle proof must match the current root (see Merkle Verification).
14. Remaining withdrawable balance must be sufficient:
    `amount <= leaf.balance - already_withdrawn`.
15. Escrow totals must be sufficient for the token:
    `ASSET_DEPOSITS[token_id] >= amount`.

If any check fails, `execute_withdraw()` returns a `ContractError` and no state
is updated.

## Permit Verification (Exact)

Permit verification is performed by `verify_permit()`:

- `RootConfig.permit_signer_pubkey` and `RootConfig.permit_signer_address`
  must be configured. If missing, `PermitSignerNotConfigured` is returned.
- `permit.data` is parsed as `relayer::verify::MsgSignData`.
- The first message is extracted (`signed_data.msgs[0]`), and its `signer`
  must equal `permit_signer_address`.
- The first message `data` is parsed as `PermitData` and must match all
  fields exactly:
  - `root_id`
  - `user` (must match `leaf.user`)
  - `token_id` (must match `leaf.token_id`)
  - `amount`
  - `nonce`
  - `destination_chain_uid`
  - `destination`
- `env.block.time` must be `<= PermitData.expiry` (seconds).
- `verify_signature(deps, &permit.data, &permit.signature, pubkey)`
  must return `true`.

If any check fails, `InvalidPermit` or `PermitExpired` is returned.

### Permit Replay Protection

After a permit is verified, a replay key is computed and stored:

- `permit_id(permit)` computes `sha256(permit.data)` and hex-encodes it.
- `USED_PERMITS[permit_id]` must be `false` or missing.
- On success, `USED_PERMITS[permit_id] = true`.

This prevents the same signed `permit.data` from being used again.

## Merkle Verification (Exact)

The proof is verified by `apply_merkle_proof()`:

1. The leaf hash is computed as:
   - `leaf_hash = sha256(JSON(WithdrawalLeaf))` via `to_json_binary(leaf)`.
2. For each `MerkleProofStep` in order:
   - `step.hash` length must be exactly 32 bytes.
   - If `position == Left`: `computed = sha256(step.hash || computed)`
   - If `position == Right`: `computed = sha256(computed || step.hash)`
3. The final `computed` hash must equal `current_root.root_hash`.

If any step hash length is invalid or the final hash does not match, the
withdraw fails with `InvalidMerkleProof`.

## State Updates (Exact)

After all checks pass:

1. Nullifier tracking:
   - Key: `nullifier_key(root_id, user, token_id, nonce)`
   - `already_withdrawn = NULLIFIERS[key]` (default 0)
   - `new_withdrawn = already_withdrawn + amount`
   - Store `NULLIFIERS[key] = new_withdrawn`

2. Asset totals (`ASSET_DEPOSITS`):
   - `new_asset_total = ASSET_DEPOSITS[token_id] - amount`
   - If `new_asset_total == 0`, remove the key.
   - Else save the new value.

3. User totals (`USER_DEPOSITS`):
   - `user_key = (user, token_id)`
   - `new_user_total = USER_DEPOSITS[user_key] - amount`
   - If `new_user_total == 0`, remove the key.
   - Else save the new value.

## Transfer (Exact)

A transfer is sent to the Virtual Balance contract:

- `destination_chain_uid` is validated via `ChainUid::create()`.
- `destination_user = CrossChainUser::new(destination_chain_uid, destination)`.
- Message:
  - `VirtualBalanceExecuteMsg::Transfer(ExecuteTransfer { ... })`
  - `amount`, `token_id`, `to = destination_user`
  - `sender`, `from`, and `msg` are `None`.
- The transfer is dispatched via `WasmMsg::Execute` to
  `state.virtual_balance` with empty funds.

## Events (Attributes)

`withdrawal_completed` is emitted with the following attributes:

- `action = withdrawal_completed`
- `root_id`
- `user`
- `token_id`
- `amount`
- `nonce`
- `destination_chain_uid`
- `destination`

## Structures Referenced in execute.rs

All of the following structures are referenced directly in
`contracts/hub/orderbook_deposits/src/execute.rs`:

### ExecuteMsg (withdraw-related variant)
```
Withdraw {
  root_id: String,
  amount: Uint256,
  nonce: u64,
  leaf: WithdrawalLeaf,
  proof: Vec<MerkleProofStep>,
  permit: Permit,
  destination_chain_uid: String,
  destination: String,
}
```

### WithdrawalLeaf
```
WithdrawalLeaf {
  user: String,
  token_id: String,
  balance: Uint256,
}
```

### MerkleProofStep
```
MerkleProofStep {
  hash: Binary,          // must be 32 bytes
  position: ProofPosition,
}
```

### ProofPosition
```
ProofPosition::Left
ProofPosition::Right
```

### Permit
```
Permit {
  data: String,       // JSON-encoded MsgSignData
  signature: Binary,  // signature over permit.data
}
```

### PermitData
```
PermitData {
  root_id: String,
  user: String,
  token_id: String,
  amount: Uint256,
  nonce: u64,
  destination_chain_uid: String,
  destination: String,
  expiry: u64,         // seconds since epoch
}
```

### RootInfo (current root)
```
RootInfo {
  root_id: String,
  root_hash: Binary,        // must be 32 bytes
  per_asset_totals: Vec<AssetTotal>,
  da_hash: Option<Binary>,
  da_url: Option<String>,
  proposed_at: u64,
}
```

### RootConfig (permit verification)
```
RootConfig {
  root_challenge_period: u64,
  permit_signer_pubkey: Option<Binary>,
  permit_signer_address: Option<String>,
  authorized_posters: Vec<Addr>,
}
```

### AssetTotal (used for root validation)
```
AssetTotal {
  token_id: String,
  amount: Uint256,
}
```

### OrderbookDepositsStatus
```
OrderbookDepositsStatus::Active
OrderbookDepositsStatus::Paused
```

### ChainUid (destination chain validation)

`ChainUid::create(uid)` is used to validate `destination_chain_uid`.
Validation rules (from `packages/euclid/src/chain.rs`):

- Must be non-empty.
- Characters must be lowercase ASCII letters, digits, or '.'.

### CrossChainUser
```
CrossChainUser { chain_uid: ChainUid, address: String }
```

### ExecuteTransfer (virtual balance transfer)
```
ExecuteTransfer {
  amount: Uint256,
  token_id: String,
  sender: Option<CrossChainUser>,
  to: CrossChainUser,
  from: Option<CrossChainUser>,
  msg: Option<Binary>,
}
```

## Hashing Details

- `permit_id(permit)` uses `sha256(permit.data)` and hex encoding.
- `hash_leaf(leaf)` uses `sha256(JSON(WithdrawalLeaf))`.
- `nullifier_key` uses `sha256(root_id || 0x00 || user || 0x00 || token_id || 0x00 || nonce_be)`
  and hex encoding.
