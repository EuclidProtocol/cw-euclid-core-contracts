# SendPacketEncoded event spec

Status: implemented. The `euclid-encoding` codec and the events described here are wired into both VMs by the wire encoding rollout (see `docs/superpowers/specs/2026-07-11-wire-encoding-in-contracts-design.md`). The event names are final and the two open items this document once carried are closed (see section 5). The normative field ordering and `topic0` tables live in that spec's section 3.1; this document records the event schema and how the two mirror each other.

This spec defined the versioned successor to the former Solidity `SendPacket` event and its CosmWasm attribute equivalent, so both sides implement from a single source of truth. The former `SendPacket` / `euclid-send-packet` and `WriteAcknowledgment` / `euclid-write-acknowledgement` events (and their emit sites) are deleted; there was no dual emit window.

## 1. Solidity event

`SendPacketEncoded` keeps the types and order of the six fields already emitted by `SendPacket` (`contracts/solidity/src/Factory/modules/CrossChain/CrossChain.lib.sol`), renames `packet_sequence` to the canonical `sequence`, then appends two new fields, `version` and `encoding`.

```solidity
/// @notice Emitted when a packet is sent to another chain (versioned envelope).
/// @param msg The message payload, encoded per `encoding`.
/// @param sequence The sequence number of the packet.
/// @param source_port The port from which the packet was sent.
/// @param destination_port The port to which the packet is sent.
/// @param timeout The timeout for the packet.
/// @param destination_chain_type The type of the destination chain.
/// @param version Protocol version of the payload schema ("0.0.1").
/// @param encoding Wire encoding of `msg`: 0 = JSON, 1 = ABI.
event SendPacketEncoded(
    bytes msg,
    uint256 sequence,
    string source_port,
    string destination_port,
    uint256 timeout,
    string destination_chain_type,
    string version,
    uint8 encoding
);
```

The field names and their order are the canonical cross-VM schema: the CosmWasm attribute list in section 3 carries the same names in the same order, so one decoder serves both legs.

No parameter is indexed, matching `SendPacket`. The only topic is `topic0`; all eight fields live ABI encoded in the log data. This means the relayer's existing field extraction logic for the first six fields ports over unchanged; only the event name and the two appended reads change.

For reference, today's event:

```solidity
event SendPacket(
    bytes msg,
    uint256 packet_sequence,
    string source_port,
    string destination_port,
    uint256 timeout,
    string destination_chain_type
);
```

## 2. Selector derivation

An event's `topic0` is `keccak256` of its canonical signature: the event name followed by the parenthesized, comma separated canonical parameter types, with no spaces, no parameter names, and no `indexed` markers.

Both hashes below were derived and verified in this environment with Foundry's `cast` (`cast Version: 1.5.1 stable`):

```bash
cast keccak "SendPacket(bytes,uint256,string,string,uint256,string)"
# 0x28fd299841189e1ff8bb4a89851bb9e8a5de093310275cec6bc96c2ff2cf9074

cast keccak "SendPacketEncoded(bytes,uint256,string,string,uint256,string,string,uint8)"
# 0x6d0c5960e12265710f05de5cddd6a373258a1760dd9fe8bf1c5dda3d94c06415
```

| Event | Signature | topic0 |
|---|---|---|
| `SendPacket` (current) | `SendPacket(bytes,uint256,string,string,uint256,string)` | `0x28fd299841189e1ff8bb4a89851bb9e8a5de093310275cec6bc96c2ff2cf9074` |
| `SendPacketEncoded` (this spec) | `SendPacketEncoded(bytes,uint256,string,string,uint256,string,string,uint8)` | `0x6d0c5960e12265710f05de5cddd6a373258a1760dd9fe8bf1c5dda3d94c06415` |

The name change alone already forces a new selector; the two appended parameters change it again on top of that. The old `SendPacket` event and its emit sites are deleted, so relayers subscribe to the new `topic0` only; there is no dual emit window.

Parameter names are not part of the canonical signature, so the later rename of `packet_sequence` to `sequence` left `SendPacketEncoded`'s `topic0` untouched. Only an event whose name or parameter types change gets a new `topic0` (section 4's write-ack event did).

## 3. CosmWasm attribute equivalent

The former CosmWasm mirror of `SendPacket` was the `euclid-send-packet` event (now deleted). Its successor `euclid-send-packet-encoded` (`euclid/src/events.rs`, `send_packet_encoded_event`), emitted by the router (`hub/router/src/execute/relay.rs`), carries the same eight fields as the Solidity event, under the same names and in the same order. The legacy `send_packet_event` ordering (which led with the ports) is gone; the attribute order below is the canonical cross-VM one.

```
Event type: "euclid-send-packet-encoded"        (indexed by Tendermint as "wasm-euclid-send-packet-encoded")

Attributes (order fixed, canonical cross-VM schema):
  msg                      payload bytes; raw JSON text when encoding is 0,
                            0x prefixed lowercase hex when encoding is 1
  sequence                 u128 as decimal string
  source_port              e.g. "vsl.<router_addr>"
  destination_port         e.g. "<chain_uid>.<factory_addr>"
  timeout                  unix seconds as decimal string
  destination_chain_type   "cosmos" | "evm" | "tvm" | "native"
  version                  PROTOCOL_VERSION, "0.0.1"
  encoding                 "0" | "1" (Encoding::as_u8 as decimal string)
```

Representation rule (Amendment B, 2026-07-11, `docs/superpowers/specs/2026-07-11-wire-encoding-in-contracts-design.md` section 13.2): `msg` is raw JSON text when `encoding` is `0`, and `0x` prefixed lowercase hex when `encoding` is `1`. This supersedes the base64 rule this document previously recorded for the ABI case. The underlying payload bytes are unchanged in both encodings, and Solidity `bytes` already render as `0x` hex everywhere, so both VMs' relayer now sees exactly one representation per encoding.

The same representation rule governs the `String` entry point fields on both the router and factory `ExecuteMsg` pairs: `ReceivePacket.msg` and `AcknowledgePacket.msg`/`.ack`, which changed type from `Binary` to `String` in Amendment B. `AcknowledgePacket` carries no `encoding` field; the handler resolves the representation from the stored `PendingPacket.encoding` before parsing.

Relayer migration note: the production relayer must send these new `String` forms (raw JSON text or `0x` hex, per the leg encoding) to `ReceivePacket` and `AcknowledgePacket`, and must read the new attribute representations off these events. This ships in the same lockstep cutover as the event rename (Amendment A); there is no dual format window.

### Helper signature

Implemented in `euclid::events` next to `send_packet_event`. The parameters follow the canonical attribute order:

```rust
pub const EUCLID_SEND_PACKET_ENCODED_EVENT: &str = "euclid-send-packet-encoded";

pub fn send_packet_encoded_event(
    msg: &str,
    sequence: u128,
    source_port: &str,
    destination_port: &str,
    timeout: u64,
    destination_chain_type: &str,
    version: &str,       // pass euclid_encoding::PROTOCOL_VERSION
    encoding: u8,        // pass Encoding::as_u8()
) -> Event
```

## 4. Companion: WriteAcknowledgementEncoded

The former Solidity companion to `SendPacket` was `WriteAcknowledgment(bytes msg, uint256 packet_sequence, string source_port, string destination_port, bytes acknowledgment, string destination_chain_type, string type_)`, with the matching CosmWasm side being `euclid-write-acknowledgement`. Both are deleted. Their versioned successor is `WriteAcknowledgementEncoded` (CosmWasm `euclid-write-acknowledgement-encoded`), which appends `version` and `encoding` the same way `SendPacketEncoded` extends `SendPacket`. The event name takes the British spelling "Acknowledgement" on both VMs, and the legacy `packet_sequence`, `acknowledgment`, and `type_` fields take the canonical names `sequence`, `ack`, and `ack_type`.

### Solidity `WriteAcknowledgementEncoded`

The field order is the old seven fields in their existing order, then `version`, then `encoding` appended:

```solidity
event WriteAcknowledgementEncoded(
    bytes msg,
    uint256 sequence,
    string source_port,
    string destination_port,
    bytes ack,
    string destination_chain_type,
    string ack_type,         // "success" | "error"
    string version,          // PROTOCOL_VERSION, "0.0.1"
    uint8 encoding           // encoding of both msg and ack: 0 = JSON, 1 = ABI
);
```

No parameter is indexed, matching the other events.

| Event | Signature | topic0 |
|---|---|---|
| `WriteAcknowledgementEncoded` | `WriteAcknowledgementEncoded(bytes,uint256,string,string,bytes,string,string,string,uint8)` | `0x5a0910765c30c6e14978ee31f1b9f544f01e962637c3a6bd484852c0d3791697` |

The earlier American spelling `WriteAcknowledgmentEncoded` hashed to `0x037fd99f2e7dcfe023b3576c7b898a93a64e2eb97b685da6506fb8a9efacb482`. That `topic0` is dead and no deployment ever emitted it.

Both `topic0` values in this document (`SendPacketEncoded` in section 2, `WriteAcknowledgementEncoded` here) were derived with `cast keccak` in this environment and confirmed against the compiled ABI. Spec section 3.1 carries the same two rows and is the normative table.

### CosmWasm `euclid-write-acknowledgement-encoded`

Emitted exactly once, complete, from the receive reply handler (Amendment A of the wire encoding spec). The old two part pattern (a header event at receive time plus a bare `ack` attribute event from the reply) is gone, and with it every positional header/payload join in the parsers.

```
Event type: "euclid-write-acknowledgement-encoded"   (indexed by Tendermint as "wasm-euclid-write-acknowledgement-encoded")

Attributes (order fixed, canonical cross-VM schema):
  msg                      the original packet payload, raw JSON text when encoding is 0,
                           0x prefixed lowercase hex when encoding is 1
  sequence                 u128 as decimal string
  source_port              the acknowledging contract's port
  destination_port         the original sender's port (swapped orientation)
  ack                      acknowledgement payload, same representation rule as msg
  destination_chain_type   "cosmos" | "evm" | "tvm" | "native"
  ack_type                 "success" | "error" (same field name on both VMs)
  version                  PROTOCOL_VERSION, "0.0.1"
  encoding                 "0" | "1" (Encoding::as_u8 as decimal string)
```

The names and order match the Solidity event above field for field. `ack_type` lets bridges classify an ack as success or error without decoding the payload.

## 5. Items carried from the implementation plan (resolved)

1. **Event naming: final** (spec decision 4). The names `SendPacketEncoded`, `WriteAcknowledgementEncoded`, `euclid-send-packet-encoded`, and `euclid-write-acknowledgement-encoded` are frozen as speced in section 3 of the wire encoding spec; no alternative such as `EuclidSendPacket` was taken. Field names and their order are the canonical cross-VM schema recorded in sections 1, 3, and 4, identical on both VMs. The `topic0` table above is normative.
2. **Sentinel success ack: kept** (spec decision 3). The sentinel `Ok(b"1")` (`make_ack_success`, modeled in the codec as `AcknowledgementMsg<Vec<u8>>`) stays; no typed empty success ack was introduced. The sentinel appears only in JSON internal paths, and the two empty denom acks are already modeled as empty wire payloads.
3. `Uint128` fields ride the wire as `uint128`, not widened to `uint256` (see the package's canonical ABI schema tables). Any Solidity mirror generated from this spec must declare `uint128` to match. (Unchanged technical note, still accurate.)
