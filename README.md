# Nexora — Cloud Operating System

> A production-grade, full-stack cloud operating system built in Rust + SvelteKit.
> Implements a binary protocol (NXP), kernel, 8 services, marketplace, billing,
> cluster manager, and a real-time web UI — all in one repository.

[![CI](https://github.com/amir-helal-ali/nexora/actions/workflows/ci.yml/badge.svg)](https://github.com/amir-helal-ali/nexora/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.82+-orange.svg)](https://www.rust-lang.org/)
[![SvelteKit](https://img.shields.io/badge/SvelteKit-2-ff3e00.svg)](https://kit.svelte.dev/)

---

## What is Nexora?

Nexora is **not** a SaaS. It is **not** a dashboard. It is a **cloud operating system** —
a distributed platform where every capability is a module, every communication flows
through a binary protocol (NXP over QUIC), and every action is auditable.

The platform was designed from a 15-part engineering specification enforcing:
Rust-only backend, SvelteKit frontend, zero-trust security, event-driven architecture,
binary internal communication, low-resource deployment, and deferred AI (reserved opcodes).

### Quick Numbers

| Metric | Value |
|--------|-------|
| Rust crates | 15 |
| Frontend pages | 15 |
| HTTP routes | 55 |
| WebSocket message types | 6 |
| Unit tests | 250+ |
| `unsafe` blocks | 0 |
| Docker images | 2 |
| CI/CD jobs | 3 |
| RFC documents | 6 |

---

## Architecture

```
  ┌──────────────────────────────────────────────────────────────┐
  │                     Browser (SvelteKit)                       │
  │  15 pages · Command Palette (⌘K) · Notification Bell ·       │
  │  SVG Charts · WebSocket Terminal · API Explorer              │
  └──────────────────────────┬───────────────────────────────────┘
                             │ HTTP / WebSocket / SSE
  ┌──────────────────────────▼───────────────────────────────────┐
  │                   API Gateway (Rust + axum)                   │
  │  55 routes · Rate limiting · Bearer auth · JSON ↔ MsgPack    │
  │  SSE live events · WebSocket bidirectional · OpenAPI spec    │
  └──────────────────────────┬───────────────────────────────────┘
                             │ In-process dispatch
  ┌──────────────────────────▼───────────────────────────────────┐
  │                      Nexora Core (Kernel)                     │
  │  Modules · Registry · EventBus · Permissions · Plugins ·     │
  │  Config · Secrets · Health                                   │
  └──┬───────┬───────┬───────┬───────┬───────┬───────┬──────────┘
     │       │       │       │       │       │       │
  ┌──▼──┐ ┌─▼──┐ ┌─▼──┐ ┌─▼──┐ ┌─▼──┐ ┌─▼──┐ ┌─▼──────┐
  │Auth │ │Mkt │ │Bill│ │Work│ │Clus│ │Notif│ │Storage │
  │     │ │place│ │ing │ │flow│ │ter │ │     │ │(SQLite)│
  └─────┘ └────┘ └────┘ └────┘ └────┘ └─────┘ └────────┘
```

### NXP Protocol (Part 3)

The Nexora Exchange Protocol is the **native binary language** of the platform.
It replaces HTTP for all internal communication:

- **Layer 1:** QUIC transport (TLS 1.3, multiplexing, 0-RTT)
- **Layer 2:** Session (X25519 ECDHE, HKDF-SHA256, ChaCha20-Poly1305)
- **Layer 3:** Command frames (43-byte header, 25 opcodes, 16-bit flags)
- **Layer 4:** Payload (MessagePack / CBOR — JSON forbidden internally)
- **Layer 5:** Application (marketplace, billing, auth, etc.)

---

## Quick Start

### Option A: Docker (Recommended)

```bash
git clone https://github.com/amir-helal-ali/nexora.git
cd nexora
docker-compose up
```

- **Frontend:** http://localhost:3000
- **Backend:** http://localhost:8080
- **Login:** `admin` / `admin123`
- Data persists to a Docker volume.

### Option B: Manual

```bash
# Backend
cargo build --release --workspace
./target/release/gateway-demo 127.0.0.1:8080

# Frontend (separate terminal)
cd frontend && npm install && npm run dev -- --host 0.0.0.0 --port 3000
```

Open http://localhost:3000 → login with `admin` / `admin123`.

---

## Services (8)

| Service | Crate | Description |
|---------|-------|-------------|
| **Core** | `nexora-core` | Kernel: 8 subsystems (modules, registry, events, permissions, plugins, config, secrets, health) |
| **Auth** | `nexora-auth` | User management, Argon2id passwords, Ed25519-signed tokens, sessions |
| **Gateway** | `nexora-gateway` | HTTP API (55 routes), SSE, WebSocket, rate limiting, OpenAPI |
| **Marketplace** | `nexora-marketplace` | 6 package types, 13-step install pipeline, Ed25519 signatures, SemVer, auto-update + rollback |
| **Billing** | `nexora-billing` | Invoices, payments, subscriptions, revenue tracking |
| **Workflow** | `nexora-workflow` | Event-driven automation pipelines, conditions, trigger substitution |
| **Cluster** | `nexora-cluster` | Multi-node coordination, discovery, failover, load balancing |
| **Notifications** | `nexora-notifications` | Per-user notifications, read/unread tracking, severity levels |
| **Storage** | `nexora-storage` | SQLite persistence for all data (users, events, packages, billing) |

---

## Frontend Pages (15)

| Page | Description |
|------|-------------|
| `/` | Dashboard — unified stats from all services + live events |
| `/metrics` | Metrics & Analytics — SVG charts (sparkline, donut, bar, gauge) |
| `/events` | Event log with live SSE updates + filters |
| `/audit` | Audit log — immutable history with search, time-range, export |
| `/modules` | Module grid with state badges |
| `/marketplace` | Package store — search, install, uninstall, trust scores |
| `/billing` | Invoices, payments, subscriptions with revenue display |
| `/workflows` | Workflow builder — create, trigger, execution history |
| `/cluster` | Cluster topology — node grid, register, heartbeat |
| `/terminal` | WebSocket terminal — bidirectional real-time communication |
| `/api-explorer` | Built-in API testing tool — 24 endpoints, request/response viewer |
| `/health` | Subsystem health grid |
| `/settings` | Profile, change password, sessions, user management |
| `/login` | Login form |
| `/logout` | Logout redirect |

### Frontend Features

- **Command Palette** (⌘K) — keyboard-first navigation across all pages
- **Notification Bell** — live unread count + dropdown with mark-read
- **SSE** — server-sent events for live event streaming
- **WebSocket** — bidirectional terminal for real-time interaction
- **Dark-first design** — Linear/Vercel-inspired, TailwindCSS, zero external chart libraries
- **Rate limit headers** — `X-RateLimit-*` on every response

---

## Tech Stack

### Backend (Rust)

| Category | Technology |
|----------|-----------|
| Language | Rust 1.82+ |
| Async runtime | tokio |
| HTTP server | axum 0.7 (with ws feature) |
| QUIC transport | quinn 0.11 |
| Serialization | MessagePack (rmp-serde) + CBOR |
| Cryptography | ChaCha20-Poly1305, Ed25519, X25519, HKDF-SHA256, Argon2id |
| Database | SQLite (rusqlite, bundled) |
| Observability | tracing + OpenTelemetry |

### Frontend (SvelteKit)

| Category | Technology |
|----------|-----------|
| Framework | SvelteKit 2 + Svelte 5 |
| Language | TypeScript (strict) |
| Styling | TailwindCSS 3 |
| Charts | Pure SVG (no external libraries) |
| Real-time | EventSource (SSE) + WebSocket |

### Infrastructure

| Category | Technology |
|----------|-----------|
| Containerization | Docker (multi-stage builds) |
| Orchestration | docker-compose |
| CI/CD | GitHub Actions |
| Persistence | SQLite (Tier-1), PostgreSQL-ready (Tier 2/3) |

---

## Compliance with Nexora Engineering Specification

| Part | Status | Details |
|------|--------|---------|
| Part 1 (Vision) | ✅ | Engineering-grade, production-ready |
| Part 2 (Constitution) | ✅ | Rust only, doc-first, zero-trust, modular, event-driven |
| Part 3 (NXP) | ✅ | 5 layers: QUIC + session + frames + MessagePack + app |
| Part 4 (Core) | ✅ | 8 subsystems + Cluster Manager + Workflow Engine |
| Part 5 (Marketplace) | ✅ | 6 package types, 13-step pipeline, auto-update + rollback |
| Part 6 (Backend) | ✅ | API Gateway (55 routes, SSE, WebSocket, rate limiting) |
| Part 7 (Frontend) | ✅ | SvelteKit, 15 pages, Command Palette, Notification Bell |
| Part 8 (Data) | ✅ | Event sourcing, SQLite persistence for all data |
| Part 9 (Security) | ✅ | Argon2id, Ed25519 tokens, RBAC+ABAC, rate limiting, audit log |
| Part 10 (Low-resource) | ✅ | SQLite embedded, single-binary, Docker |
| Part 11 (AI) | ✅ | Opcodes reserved, rejected at dispatch (deferred) |
| Part 13 (Observability) | ✅ | Health monitoring, structured logging, metrics page |
| Part 14 (Deployment) | ✅ | Cluster manager, Docker, CI/CD, edge-ready |

---

## Project Structure

```
nexora/
├── Cargo.toml                    # Rust workspace (15 crates)
├── docker-compose.yml            # Full-stack deployment
├── Dockerfile.backend            # Multi-stage Rust build
├── Dockerfile.frontend           # Multi-stage SvelteKit build
├── .github/workflows/ci.yml      # CI/CD: fmt + clippy + tests + Docker
├── docs/                         # 6 RFC documents
│   ├── NXP-RFC-v1.md
│   ├── CORE-RFC-v1.md
│   ├── AUTH-RFC-v1.md
│   ├── GATEWAY-RFC-v1.md
│   ├── MARKETPLACE-RFC-v1.md
│   └── FRONTEND-RFC-v1.md
├── crates/                       # 15 Rust crates
│   ├── nxp-core/                 # NXP protocol (frames, opcodes, errors)
│   ├── nxp-payload/             # MessagePack / CBOR
│   ├── nxp-security/            # AEAD, Ed25519, X25519, replay window
│   ├── nxp-session/             # HELLO handshake, session manager
│   ├── nxp-transport/           # QUIC transport (quinn)
│   ├── nexora-core/             # Kernel: 8 subsystems + handler
│   ├── nexora-auth/             # Users, sessions, Ed25519 tokens
│   ├── nexora-gateway/          # HTTP API (55 routes, SSE, WS, rate limit)
│   ├── nexora-marketplace/      # Packages, signatures, auto-update
│   ├── nexora-billing/          # Invoices, payments, subscriptions
│   ├── nexora-workflow/         # Event-driven automation
│   ├── nexora-cluster/          # Multi-node coordination
│   ├── nexora-notifications/    # User notifications
│   ├── nexora-storage/          # SQLite persistence
│   └── nxp-cli/                 # `nxp` command-line tool
├── frontend/                     # SvelteKit 2 + Svelte 5 + Tailwind 3
│   └── src/
│       ├── lib/
│       │   ├── api/gateway.ts   # Typed API client
│       │   ├── stores/          # Svelte stores
│       │   ├── components/      # Layout, StatCard, CommandPalette, NotificationBell
│       │   └── charts/          # Sparkline, Donut, BarChart, Gauge (pure SVG)
│       └── routes/              # 15 pages
└── demo/                         # 9 demo binaries
    └── src/
        ├── server.rs / client.rs         # NXP demos
        ├── core_demo.rs / core_smoke_test.rs
        ├── auth_demo.rs / auth_smoke_test.rs
        ├── gateway_demo.rs               # Main entry point
        └── storage_demo.rs
```

---

## Testing

```bash
# Run all 250+ tests
cargo test --workspace

# Run a specific crate
cargo test -p nexora-core
cargo test -p nexora-marketplace
```

All tests pass with **zero `unsafe` blocks** in the entire codebase.

---

## License

Dual-licensed under MIT OR Apache-2.0.

---

## Author

Built by [amir-helal-ali](https://github.com/amir-helal-ali)
