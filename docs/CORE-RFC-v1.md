# Nexora Core — RFC v1.0 (Draft)

**Status:** Draft
**Last Updated:** 2026-07-01
**Document Owner:** Nexora Platform Engineering
**Classification:** Internal — Engineering Specification
**Implements:** Nexora Engineering Specification, Part 4 (NEXORA CORE)

---

## 1. Abstract

Nexora Core is the **cloud operating system kernel** of the Nexora platform.
It is responsible for loading, managing, securing, monitoring, and
orchestrating every platform capability. Nothing inside the platform runs
independently of the Core.

This document specifies the v0.1 implementation of Nexora Core, which
provides 8 subsystems on top of the NXP protocol (RFC v1.0):

1. Module Manager
2. Service Registry
3. Event Bus
4. Permission Engine
5. Plugin Manager
6. Configuration Manager
7. Secret Manager
8. Health Monitor

Plus a Core NXP Handler that dispatches protocol-control and core-system
opcodes to these subsystems.

## 2. Architecture

```
                        ┌──────────────────────┐
                        │   NXP Transport      │
                        │   (QUIC via quinn)   │
                        └──────────┬───────────┘
                                   │
                        ┌──────────▼───────────┐
                        │   Core NXP Handler   │
                        │   (dispatch table)   │
                        └──────────┬───────────┘
                                   │
        ┌───────────┬──────────────┼──────────────┬───────────┐
        │           │              │              │           │
┌───────▼───┐ ┌─────▼─────┐ ┌──────▼──────┐ ┌─────▼────┐ ┌────▼─────┐
│  Modules  │ │  Registry │ │  Event Bus  │ │ Plugins  │ │ Health   │
│  Manager  │ │           │ │  (source of │ │ Manager  │ │ Monitor  │
│           │ │           │ │   truth)    │ │          │ │          │
└─────┬─────┘ └───────────┘ └─────┬───────┘ └────┬─────┘ └──────────┘
      │                           │              │
      └───────────┬───────────────┴──────────────┘
                  │
          ┌───────▼───────┐         ┌─────────────────┐
          │  Permission   │         │   Secrets       │
          │  Engine       │         │   Manager       │
          │  (RBAC + ABAC)│         │  (versioned)    │
          └───────────────┘         └─────────────────┘
```

## 3. Subsystems

### 3.1 Module Manager

Manages the lifecycle of platform modules. Every capability is a module
(Auth, Billing, Marketplace, etc.). The lifecycle is atomic and auditable:
every transition generates an event on the EventBus.

**States:** `Installed → Enabled ⇄ Paused → Removed`

**Operations:** `install`, `enable`, `pause`, `resume`, `uninstall`,
plus `get`, `list`, `list_in_state`.

### 3.2 Service Registry

Logical-name → service-instance lookup. Services register themselves and
discover peers by logical identity (never hardcoded addresses).

**Operations:** `register`, `deregister`, `heartbeat`, `mark_unhealthy`,
`lookup`, `lookup_healthy`, `pick_one` (priority-ordered).

### 3.3 Event Bus

The **source of truth** for the entire platform. Events are immutable,
append-only, replayable. Every state change generates an event.

- Monotonic 64-bit event IDs
- Broadcast channel for live subscribers (filter by name prefix)
- `replay(from_id)` and `replay_filtered(from_id, prefix)` for backfill
- In-process implementation; swappable with NATS/Kafka in Tier 2/3

### 3.4 Permission Engine

Hierarchical RBAC + ABAC. Principals hold Roles, Roles grant Permissions
on resource patterns (supports `*` trailing wildcard).

**Permissions:** `Read`, `Write`, `Create`, `Delete`, `Execute`, `Admin`

**Principal kinds:** `User`, `Service`, `Plugin`, `AiAgent`

### 3.5 Plugin Manager

Manages sandboxed, signed, resource-limited extensions. Every plugin
carries:
- A manifest with declared capabilities
- An Ed25519 signer public key + signature
- SHA-256 integrity hash (over canonical manifest bytes)
- Resource limits (CPU, memory, command rate, duration)

**States:** `Pending → Verified → Active ⇄ Stopped → Removed`

### 3.6 Configuration Manager

Dynamic, hot-reloadable key-value configuration. Supports strings, ints,
bools, floats, and nested maps.

### 3.7 Secret Manager

Versioned, audited secrets. Every put creates a new version; the previous
version is deactivated. Supports rollback to any prior version. In v0.1
secrets are stored in-memory; production deployments replace this with an
HSM-backed vault. The public API is identical.

### 3.8 Health Monitor

Aggregates subsystem health into a single status: `Healthy`, `Degraded`,
or `Unhealthy`. Worst-of wins.

## 4. Core NXP Handler

The Core implements an NXP handler that dispatches the following opcodes:

| Opcode | Implementation |
|--------|----------------|
| `PING` | Returns `{pong: true}` |
| `PONG` | Acknowledgment |
| `BYE` | Session close |
| `REGISTER_SERVICE` | Adds instance to ServiceRegistry |
| `DISCOVER_SERVICE` | Lookup instances by name |
| `SUBSCRIBE_EVENT` | Returns `{ok: true}` (transport layer holds the stream) |
| `PUBLISH_EVENT` | Publishes to EventBus; returns event_id |
| `REPLAY_EVENTS` | Returns events from offset, optional filter |
| `EXECUTE_COMMAND` | Dispatches to `module.*`, `plugin.*`, `principal.*` commands |
| AI opcodes (`0x8001`–`0x8005`) | **Rejected** (Part 11 deferred) |

The `EXECUTE_COMMAND` opcode requires a permission check against the
PermissionEngine before dispatching. Unauthorized commands return an
`AUTHZ` error.

## 5. Compliance with Nexora Specification

| Spec Section | Status |
|--------------|--------|
| Part 4 — Module Management | ✅ Implemented |
| Part 4 — Plugin Management | ✅ Implemented |
| Part 4 — Service Registry | ✅ Implemented |
| Part 4 — Permission Engine | ✅ Implemented |
| Part 4 — Event Bus | ✅ Implemented |
| Part 4 — Config Manager | ✅ Implemented |
| Part 4 — Secret Manager | ✅ Implemented |
| Part 4 — Health Monitor | ✅ Implemented |
| Part 4 — Self Healing | ⏳ Health monitoring only; auto-restart pending |
| Part 4 — Update Engine | ⏳ Pending v0.2 |
| Part 4 — Cluster Manager | ⏳ Pending v0.2 (single-node in v0.1) |
| Part 9 — Zero Trust Auth | ✅ Permission checks on every command |
| Part 11 — AI Deferred | ✅ AI opcodes rejected at dispatch |

## 6. Test Coverage

42 unit tests + 7 end-to-end smoke tests, all passing:

- **Module Manager (5):** install/enable/pause/resume/uninstall, duplicate
  rejection, invalid transition rejection, state filtering
- **Service Registry (3):** register/lookup/deregister, unhealthy filtering,
  duplicate rejection
- **Event Bus (5):** monotonic IDs, replay, filtered replay, subscriber
  filtering, immutability
- **Permission Engine (6):** admin/dev/viewer roles, unknown principal
  denial, assign/revoke, pattern matching
- **Plugin Manager (5):** register/verify/activate/stop/remove, state
  enforcement, integrity hash stability, integrity hash content sensitivity
- **Config Manager (3):** set/get, reload, defaults
- **Secret Manager (4):** put/get, versioning, rollback, delete
- **Health Monitor (4):** empty state, degraded propagation, unhealthy
  dominance, snapshot
- **Core Handler (6):** ping, AI rejection, service registration+discovery,
  event publish+replay, command execution with permission, permission
  denial
- **Smoke Test (7):** PING, PUBLISH_EVENT, EXECUTE_COMMAND install+enable,
  REPLAY_EVENTS, permission denial, AI rejection

## 7. Future Work (v0.2+)

- Cluster Manager (multi-node Core)
- Update Engine (rolling, blue/green, canary)
- Self-healing auto-restart
- Workflow Engine
- AI Runtime (Part 11 — still deferred)
- HSM-backed Signer for plugins
- Persistent event store (Postgres/ClickHouse backend)
- Distributed lock manager for cluster-wide module state
