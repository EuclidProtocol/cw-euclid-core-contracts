# Orderbook Deposits Contract

## Overview

This contract tracks virtual balance deposits for whitelisted assets, lets
authorized posters publish Merkle roots for off-chain balance snapshots, and
allows users to withdraw based on a Merkle proof plus an operator permit.

Key behaviors:
- Deposits come from the virtual balance contract via a receive hook.
- Roots can be proposed and optionally activated after a challenge period.
- Withdrawals require a valid Merkle proof and a signed permit.
- Per-asset and per-user totals are tracked and decremented on withdrawal.

## InstantiateMsg

```
InstantiateMsg {
  virtual_balance: String,
  admin: Option<String>,
  root_challenge_period: Option<u64>,
  permit_signer_pubkey: Option<Binary>,
  permit_signer_address: Option<String>,
  authorized_posters: Option<Vec<String>>,
}
```

Behavior:
- `admin` defaults to the sender if not provided.
- `root_challenge_period` defaults to 0 (roots become current immediately).
- `authorized_posters` defaults to `[admin]` if not provided.
- `permit_signer_*` must be configured to enable withdrawals.

## ExecuteMsg

### SetWhitelist
```
SetWhitelist { token_id: String, whitelisted: bool }
```
- Admin-only.
- Controls which assets can be deposited and included in root totals.

### VirtualBalanceReceive
```
VirtualBalanceReceive(VirtualBalanceReceive)
```
- Only callable by the configured `virtual_balance` contract.
- The hook message must be `VirtualBalanceReceiveHookMsg::Deposit`.
- Updates:
  - `ASSET_DEPOSITS[token_id] += amount`
  - `USER_DEPOSITS[(user, token_id)] += amount`

### UpdateConfig
```
UpdateConfig {
  admin: Option<String>,
  status: Option<OrderbookDepositsStatus>,
  root_challenge_period: Option<u64>,
  permit_signer_pubkey: Option<Binary>,
  permit_signer_address: Option<String>,
  authorized_posters: Option<Vec<String>>,
}
```
- Admin-only.
- `authorized_posters` replaces the current list; if empty, it falls back to
  `[admin]`.

### ProposeRoot
```
ProposeRoot {
  root_id: String,
  root_hash: Binary,
  per_asset_totals: Vec<AssetTotal>,
  da_hash: Option<Binary>,
  da_url: Option<String>,
}
```
- Authorized posters only (admin or in `authorized_posters`).
- Contract status must be `Active`.
- `root_hash` must be 32 bytes.
- Each `per_asset_totals` entry must be whitelisted and <= on-chain escrow
  (`ASSET_DEPOSITS`).
- If `root_challenge_period > 0`, the root is stored as pending.
- Otherwise it becomes the current root immediately.

### ActivateRoot
```
ActivateRoot { root_id: String }
```
- Authorized posters only.
- Contract status must be `Active`.
- Pending root must exist, match `root_id`, and have passed the challenge
  period.
- The pending root becomes the current root.

### Withdraw
```
Withdraw {
  root_id: String,
  amount: Uint128,
  nonce: u64,
  leaf: WithdrawalLeaf,
  proof: Vec<MerkleProofStep>,
  permit: Permit,
  destination: String,
}
```
Requirements:
- Contract status is `Active`.
- `root_id` matches the current root.
- `leaf` is well-formed and matches the request (`leaf.nonce == nonce`).
- `leaf.token_id` is whitelisted.
- Permit is valid and not expired, and the signature verifies against the
  configured `permit_signer_pubkey`.
- Permit payload must match `root_id`, `user`, `token_id`, `amount`, `nonce`,
  and `destination`.
- Permit data must not be replayed.
- Merkle proof must compute the current root hash.
- `amount <= leaf.balance - already_withdrawn`.
- Escrow totals must be sufficient.

Effects:
- Updates nullifier tracking for `(root_id, user, token_id, nonce)`.
- Decrements `ASSET_DEPOSITS` and `USER_DEPOSITS`.
- Transfers virtual balance to `destination` on the VSL chain.
- Emits `action=withdrawal_completed` with relevant attributes.

## QueryMsg

### State
```
State {}
```
Returns:
```
StateResponse { admin, status, virtual_balance }
```

### AssetDeposit
```
AssetDeposit { token_id: String }
```
Returns:
```
AssetDepositResponse { token_id, amount }
```

### UserDeposit
```
UserDeposit { user: String, token_id: String }
```
Returns:
```
UserDepositResponse { user, token_id, amount }
```

### Whitelist
```
Whitelist { token_id: String }
```
Returns:
```
WhitelistResponse { token_id, whitelisted }
```

### WhitelistedAssets
```
WhitelistedAssets { start_after: Option<String>, limit: Option<u32> }
```
Returns:
```
WhitelistListResponse { assets: Vec<WhitelistResponse> }
```

### CurrentRoot
```
CurrentRoot {}
```
Returns:
```
RootResponse {
  root_id,
  root_hash,
  per_asset_totals,
  da_hash,
  da_url,
  proposed_at,
}
```

## Message Types

```
AssetTotal { token_id: String, amount: Uint128 }

WithdrawalLeaf {
  user: String,
  token_id: String,
  balance: Uint128,
  nonce: u64,
}

MerkleProofStep {
  hash: Binary,
  position: ProofPosition, // Left | Right
}

Permit { data: String, signature: Binary }

PermitData {
  root_id: String,
  user: String,
  token_id: String,
  amount: Uint128,
  nonce: u64,
  destination: String,
  expiry: u64,
}
```

## Merkle Proof Rules

Merkle root verification is SHA-256 based:
- `leaf_hash = sha256(JSON(WithdrawalLeaf))`
- For each proof step:
  - `Left`: `hash = sha256(step.hash || current)`
  - `Right`: `hash = sha256(current || step.hash)`
- The computed hash must equal `current_root_hash`.

The `root_hash` and proof `hash` values must be 32 bytes.
