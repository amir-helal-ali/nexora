# NXP — Nexora Exchange Protocol
## RFC v1.0 (Draft)

**Status:** Draft
**Last Updated:** 2026-07-01
**Document Owner:** Nexora Platform Engineering
**Classification:** Internal — Engineering Specification

---

## 1. Abstract

NXP is a binary, connection-oriented, event-driven application protocol designed as the native communication language of the Nexora ecosystem. It replaces HTTP/WebSocket/gRPC for all internal platform communication. HTTP is permitted only at the external API gateway boundary for browser and third-party compatibility.

NXP is layered on top of QUIC (RFC 9000) for transport, providing TLS 1.3, multiplexing, and 0-RTT connection resumption out of the box. On top of QUIC, NXP defines a session layer, a command layer, and a payload layer, all of which are binary-framed, encrypted, and integrity-verified.

## 2. Design Goals

| Goal | Target |
|------|--------|
| Connection setup | < 20 ms (with 0-RTT resume) |
| Frame parse latency | microseconds |
| Heap allocations per frame | ≤ 1 (ideally 0) |
| Internal wire overhead vs JSON | < 30% |
| Backpressure | Native (QUIC flow control + NXP credit) |
| Replay protection | Mandatory (nonce + replay window) |
| Forward secrecy | Mandatory (X25519 ECDHE per session) |

## 3. Layered Architecture

```
┌─────────────────────────────────────────────────┐
│ L5  Application Layer (Marketplace, ERP, etc.)  │
├─────────────────────────────────────────────────┤
│ L4  Payload Layer (MessagePack / CBOR binary)   │
├─────────────────────────────────────────────────┤
│ L3  Command Layer (opcodes + frame header)      │
├─────────────────────────────────────────────────┤
│ L2  Session Layer (auth, keys, heartbeat)       │
├─────────────────────────────────────────────────┤
│ L1  Transport Layer (QUIC + TLS 1.3)            │
└─────────────────────────────────────────────────┘
```

### 2.1 Layer 1 — Transport (QUIC)

- Reliable, ordered-stream transport over UDP
- TLS 1.3 baked into the handshake (no separate TLS layer)
- Native multiplexing: each NXP command stream maps to a QUIC bidirectional stream
- 0-RTT connection resumption supported for repeat clients
- Connection migration (mobile/edge clients can change IP without dropping the session)

### 2.2 Layer 2 — Session

- A session is established by a `HELLO` command immediately after the QUIC handshake
- X25519 ECDHE derives a shared secret; HKDF-SHA256 expands it into:
  - `tx_key` / `rx_key` (ChaCha20-Poly1305 AEAD keys, one per direction)
  - `session_id` (16 bytes)
  - `replay_window` seed
- Heartbeats every 15s; 3 missed heartbeats ⇒ session considered dead
- Sessions are short-lived (max 1h) and rotated
- Session resumption tokens are signed by the server and valid for 24h

### 2.3 Layer 3 — Command & Frame

Every NXP message is a **Frame**. A frame consists of:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Magic (0x4E 'N') | Ver (1B) |  Flags (2B)   |  Opcode (2B)  |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       Stream ID (4B)                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                    Request ID    (8B)                         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      Timestamp (8B, μs)                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Nonce (12B)                            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                    Payload Length (4B)                         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                  Payload (variable, encrypted)                |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|              Auth Tag (16B, ChaCha20-Poly1305)                |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|              Signature (64B, Ed25519, optional)               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

**Total fixed header: 48 bytes** (without signature). With Ed25519 signature: 112 bytes.

#### Frame Field Semantics

| Field | Size | Purpose |
|-------|------|---------|
| Magic | 2B | `0x4E58` (`'NX'`) — protocol identification |
| Version | 1B | NXP version, currently `0x01` |
| Flags | 2B | Bitfield: COMPRESSED, ENCRYPTED, SIGNED, STREAM_END, ERROR, ETC. |
| Opcode | 2B | Command code (see §4) |
| Stream ID | 4B | Multiplexing identifier (maps to QUIC stream) |
| Request ID | 8B | Unique per-request, used for correlating responses |
| Timestamp | 8B | Microseconds since UNIX epoch (UTC) |
| Nonce | 12B | Per-frame AEAD nonce (never reused within a session) |
| Payload Length | 4B | Length of ciphertext payload (max 16 MiB) |
| Payload | variable | AEAD-encrypted MessagePack payload |
| Auth Tag | 16B | ChaCha20-Poly1305 tag |
| Signature | 64B | Ed25519 signature over (header ‖ ciphertext ‖ tag), optional |

#### Flags Bitfield

```
Bit 0: COMPRESSED   — payload is zstd-compressed before encryption
Bit 1: ENCRYPTED    — payload is AEAD-encrypted (always set after session setup)
Bit 2: SIGNED       — Ed25519 signature appended
Bit 3: STREAM_END   — last frame of a stream
Bit 4: ERROR        — frame carries an error response
Bit 5: BATCHED      — payload contains multiple sub-frames
Bit 6: COMPACT      — uses CBOR instead of MessagePack
Bit 7: ACK_REQUIRED — sender requests explicit ACK
Bits 8-15: reserved
```

### 2.4 Layer 4 — Payload

Payloads are **binary-serialized**. The following are allowed:

| Format | Use Case |
|--------|----------|
| MessagePack | Default for command payloads (compact, schemaless) |
| CBOR | Alternative for streaming payloads |
| Cap'n Proto | Reserved for high-throughput streams (not implemented in v1.0) |
| FlatBuffers | Reserved for future zero-copy paths (not implemented in v1.0) |

**JSON is forbidden** for internal NXP communication. JSON encoding only happens at the external API Gateway (HTTP ↔ NXP translation layer).

### 2.5 Layer 5 — Application

Application modules (Marketplace, Billing, Auth, etc.) register command handlers and event publishers with the Core. They never speak raw NXP frames — they use the SDK's typed command/event API.

## 4. Command Opcodes

Opcodes are 16-bit unsigned integers. The opcode space is divided:

| Range | Purpose |
|-------|---------|
| `0x0000`–`0x00FF` | Protocol control (HELLO, PING, BYE, etc.) |
| `0x0100`–`0x0FFF` | Core system commands (auth, session, registry) |
| `0x1000`–`0x1FFF` | Identity & access commands |
| `0x2000`–`0x2FFF` | Marketplace commands |
| `0x3000`–`0x3FFF` | Billing & subscription commands |
| `0x4000`–`0x4FFF` | Project & deployment commands |
| `0x5000`–`0x5FFF` | Storage & file commands |
| `0x6000`–`0x6FFF` | Plugin & module commands |
| `0x7000`–`0x7FFF` | Analytics & observability commands |
| `0x8000`–`0x8FFF` | Reserved for future AI layer (DEFERRED — Part 11) |
| `0x9000`–`0xBFFF` | Reserved for future expansion |
| `0xC000`–`0xFFFF` | Application-defined (marketplace-published packages) |

### 4.1 Protocol Control Opcodes (v1.0)

| Opcode | Name | Direction | Purpose |
|--------|------|-----------|---------|
| `0x0001` | `HELLO` | C→S | Initiate session, negotiate capabilities |
| `0x0002` | `HELLO_ACK` | S→C | Accept session, return session ID + capabilities |
| `0x0003` | `PING` | bidirectional | Heartbeat probe |
| `0x0004` | `PONG` | bidirectional | Heartbeat response |
| `0x0005` | `BYE` | bidirectional | Graceful session close |
| `0x0006` | `ERROR` | bidirectional | Error response |
| `0x0007` | `ACK` | bidirectional | Explicit acknowledgment |
| `0x0008` | `RESUME` | C→S | Resume a previously-issued session token |
| `0x0009` | `RESUME_ACK` | S→C | Resume accepted |

### 4.2 Core System Opcodes (v1.0 subset)

| Opcode | Name | Purpose |
|--------|------|---------|
| `0x0100` | `AUTH_LOGIN` | Login with credentials / OAuth token |
| `0x0101` | `AUTH_LOGOUT` | End authenticated session |
| `0x0102` | `AUTH_REFRESH` | Rotate session keys |
| `0x0110` | `REGISTER_SERVICE` | Service self-registers with Core |
| `0x0111` | `DISCOVER_SERVICE` | Lookup service by logical name |
| `0x0112` | `SUBSCRIBE_EVENT` | Subscribe to event stream |
| `0x0113` | `PUBLISH_EVENT` | Publish an event |
| `0x0114` | `REPLAY_EVENTS` | Replay events from offset |
| `0x0120` | `EXECUTE_COMMAND` | Generic command dispatch |
| `0x0121` | `STREAM_OPEN` | Open a long-lived stream |
| `0x0122` | `STREAM_CLOSE` | Close a stream |

### 4.3 Reserved AI Opcodes (DEFERRED — Part 11)

Defined as identifiers only, **no runtime implementation**:

| Opcode | Name | Status |
|--------|------|--------|
| `0x8001` | `AI_REQUEST` | Reserved |
| `0x8002` | `AI_STREAM` | Reserved |
| `0x8003` | `AI_CONTEXT_SYNC` | Reserved |
| `0x8004` | `AI_AGENT_EXEC` | Reserved |
| `0x8005` | `AI_MODEL_QUERY` | Reserved |

## 5. Security Model

### 5.1 Frame-Level Security

Every frame carries:
1. **ChaCha20-Poly1305 AEAD** ciphertext (confidentiality + integrity)
2. **Per-frame nonce** (12 bytes, never reused within a session)
3. **Timestamp** (microsecond precision, ±60s skew tolerance)
4. **Ed25519 signature** (optional, mandatory for privileged commands)

### 5.2 Replay Protection

Each session maintains a sliding **replay window** of 1024 nonces. Any frame with a nonce already seen, or older than the window's lowest nonce, is rejected.

### 5.3 Authentication

- **Session establishment:** X25519 ECDHE + HKDF-SHA256
- **Service identity:** Ed25519 keypair (public key registered with Core)
- **User identity:** OAuth2/OIDC token validated by Core, exchanged for session
- **Plugin identity:** Signed manifest + Ed25519 keypair scoped to declared capabilities

### 5.4 Authorization

Authorization is enforced at the Core's Permission Engine, not in NXP itself. NXP carries the **identity context** (session ID + actor ID + scope), and the Core decides whether the opcode + payload is permitted for that identity.

### 5.5 Forward Secrecy

- All session keys are derived from ephemeral X25519 keypairs
- Private keys are zeroized after key derivation
- Server-side session keys are stored in locked memory (`mlock`) where supported
- Session keys never touch disk

## 6. Performance Budget

| Operation | Budget |
|-----------|--------|
| Frame encode (1 KiB payload) | < 5 μs |
| Frame decode (1 KiB payload) | < 5 μs |
| AEAD encrypt (1 KiB) | < 2 μs |
| AEAD decrypt + verify (1 KiB) | < 3 μs |
| Ed25519 sign | < 50 μs |
| Ed25519 verify | < 150 μs |
| Heap allocations per frame path | ≤ 1 (target: 0) |
| Connection setup (cold) | < 50 ms |
| Connection setup (0-RTT) | < 20 ms |

## 7. Backpressure & Flow Control

- **QUIC-level flow control** is inherited (per-stream and per-connection windows)
- **NXP-level credit:** each `STREAM_OPEN` carries an initial credit (in frames). The receiver grants more credit via `ACK` frames with a `credit_grant` field
- **Slow start:** new streams start with a credit of 16 frames; the receiver ramps up credit by 2x each round trip until the receiver's buffer pressure crosses a threshold
- **Backpressure propagation:** when a service's downstream queue is full, the service stops ACK'ing; the sender blocks within one RTT

## 8. Error Model

Errors are carried in `ERROR` frames (opcode `0x0006`). An error frame contains:

```rust
pub struct NxpError {
    pub code: u32,           // Stable, namespace-scoped error code
    pub scope: ErrorScope,   // PROTOCOL / SESSION / AUTH / AUTHZ / APP / INTERNAL
    pub message: String,     // Human-readable (English), for logs only
    pub retryable: bool,     // Whether the caller should retry
    pub details: Value,      // Arbitrary structured details (MessagePack)
}
```

### Error Scope Namespaces

| Scope | Range | Examples |
|-------|-------|---------|
| PROTOCOL | 0x0000–0x00FF | Malformed frame, bad magic, version mismatch |
| SESSION | 0x0100–0x01FF | Expired session, replay detected, heartbeat timeout |
| AUTH | 0x0200–0x02FF | Invalid token, missing credentials |
| AUTHZ | 0x0300–0x03FF | Insufficient permissions |
| APP | 0x1000–0xFFFF | Application-defined (per opcode namespace) |
| INTERNAL | 0xFF00–0xFFFF | Catch-all for internal failures (always retryable: false) |

## 9. Extensibility

New opcodes, event types, and capabilities are registered at runtime via the Core's Capability Registry. No protocol-core modification is required to add:
- A new command (new opcode + handler)
- A new event type (new event namespace + publisher)
- A new stream type (new stream flag + handler)
- A new capability (negotiated in `HELLO`)

The `HELLO` frame carries a `capabilities` bitmask, and the server responds with the intersection of capabilities it supports. Unknown capabilities are silently ignored (forward compatibility).

## 10. Wire Format Example

A `PING` frame (no payload, no signature):

```
4E 58                    -- Magic 'NX'
01                       -- Version 1
00 20                    -- Flags: ENCRYPTED (bit 1)
00 03                    -- Opcode: PING (0x0003)
00 00 00 07              -- Stream ID 7
00 00 00 00 00 00 00 2A  -- Request ID 42
00 00 00 00 65 9D 1E 20  -- Timestamp (microseconds)
00 00 00 00 00 00 00 00  -- Nonce (12B)
00 00 00 00
00 00 00 10              -- Payload length: 16 bytes (ciphertext of empty payload)
[16 bytes of ciphertext] -- Encrypted empty payload
[16 bytes auth tag]      -- ChaCha20-Poly1305 tag
```

## 11. Versioning

- NXP follows semantic versioning at the protocol level
- The `Version` byte in the frame header is the **wire format version**, currently `0x01`
- Wire format version changes are backwards-incompatible and require a new `HELLO` capability negotiation
- Opcode additions within the same wire version are always backwards-compatible
- Opcode deprecations require one wire-version cycle of overlap

## 12. Future Work (Out of v1.0 Scope)

- zstd compression for large payloads (flag reserved, not yet implemented)
- Cap'n Proto zero-copy payload path
- 0-RTT session resumption across regions (requires global session token store)
- AI opcode implementations (Part 11 — DEFERRED)
- NXP packet analyzer CLI tool (`nxp sniff`, `nxp replay`)

## 13. References

- RFC 9000 — QUIC: A UDP-Based Multiplexed and Secure Transport
- RFC 9001 — Using TLS to Secure QUIC
- RFC 8446 — TLS 1.3
- RFC 7748 — Elliptic Curves for Security (X25519)
- RFC 8032 — EdDSA (Ed25519)
- RFC 8439 — ChaCha20-Poly1305 AEAD
- RFC 5869 — HKDF
- MessagePack specification — https://github.com/msgpack/msgpack/blob/master/spec.md
- Nexora Engineering Specification, Parts 1–15
