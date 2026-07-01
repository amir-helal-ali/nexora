# Changelog

All notable changes to Nexora are documented in this file.

## [1.0.0] — 2026-07-01

### Added — NXP Protocol (Part 3)
- Binary protocol over QUIC with 5 layers (transport, session, command, payload, application)
- 25 opcodes (protocol control, core system, reserved AI)
- ChaCha20-Poly1305 AEAD encryption with frame header bound as AAD
- Ed25519 signatures for privileged frames
- X25519 ECDHE for session key agreement + HKDF-SHA256 derivation
- 1024-entry sliding replay window per session
- MessagePack + CBOR payload serialization (JSON forbidden internally)
- `nxp` CLI tool (version, keygen, ping, sniff)
- 44 unit tests

### Added — Nexora Core (Part 4)
- 8 subsystems: Module Manager, Service Registry, Event Bus, Permission Engine, Plugin Manager, Config Manager, Secret Manager, Health Monitor
- Module lifecycle: Installed → Enabled ⇄ Paused → Removed
- Event Bus: immutable, append-only, replayable, broadcast subscribers
- Permission Engine: RBAC + ABAC with wildcard resource patterns
- Plugin Manager: Ed25519-signed manifests, SHA-256 integrity, resource limits
- 42 unit tests + 7 smoke tests

### Added — Marketplace (Part 5)
- 6 package types: Module, Plugin, AI Agent, Template, Service, Automation
- 13-step installation pipeline (signature → deps → security → compatibility → sandbox → deploy)
- Ed25519 signature verification + SHA-256 integrity hashing
- SemVer with version ranges (^, ~, >=, <, *, exact)
- Acyclic dependency graph with topological sort
- Auto-update system: Auto/Manual/Disabled policies
- Rollback to any previous version
- 5 billing models: OneTime, Subscription, UsageBased, Enterprise, Free
- 5 visibility levels: Public, Private, Org-only, Enterprise-only, Region-restricted
- Trust scores (security, performance, stability, community, enterprise)
- 66 unit tests

### Added — API Gateway (Part 6)
- 55 HTTP routes across 8 services
- Bearer token middleware with Ed25519 verification
- SSE live event streaming
- WebSocket bidirectional communication (6 message types)
- Rate limiting (sliding window, per-user/per-IP, X-RateLimit-* headers)
- OpenAPI 3.0 spec at /api/openapi.json
- JSON ↔ MessagePack translation
- CORS, body limit (16 MiB), request tracing
- 18 unit tests

### Added — Frontend (Part 7)
- SvelteKit 2 + Svelte 5 + TypeScript strict + TailwindCSS 3
- 15 pages: Dashboard, Metrics, Events, Audit, Modules, Marketplace, Billing, Workflows, Cluster, Terminal, API Explorer, Health, Settings, Login, Logout
- Command Palette (⌘K) with navigation + actions + package search
- Notification Bell with live unread count + dropdown
- 4 pure SVG chart components (Sparkline, Donut, BarChart, Gauge)
- SSE live event streaming on Dashboard + Events page
- WebSocket terminal page
- API Explorer with 24 curated endpoints
- Dark-first design (Linear/Vercel-inspired)
- 13 nav items, 14 Command Palette items

### Added — Data & Events (Part 8)
- Event sourcing: EventBus is the source of truth
- SQLite persistence for all data:
  - Users (with Argon2id password hashes)
  - Events (immutable, append-only, replayable)
  - Packages (multi-version, install tracking)
  - Invoices, Payments, Subscriptions
- Write-through cache pattern (in-memory reads, SQLite durability)
- Load-on-startup for all stores
- 23 unit tests + persistence demo

### Added — Security (Part 9)
- Argon2id password hashing (RFC 9106)
- Ed25519-signed session tokens (versioned, expiring, refreshable)
- Version-based token revocation (logout/refresh invalidates old tokens)
- RBAC + ABAC permission engine
- Zero-trust: every request validated independently
- Audit log: immutable record of all platform actions
- Rate limiting: per-user + per-IP throttling
- 30 unit tests

### Added — DevOps (Part 10)
- Docker multi-stage builds (backend ~30MB, frontend Node.js slim)
- docker-compose with healthcheck + persistent volume
- GitHub Actions CI/CD (3 jobs: Rust checks, frontend build, Docker build)
- SQLite embedded (no external DB server required)
- Single-binary deployment

### Added — AI (Part 11)
- 5 AI opcodes reserved (AI_REQUEST, AI_STREAM, AI_CONTEXT_SYNC, AI_AGENT_EXEC, AI_MODEL_QUERY)
- Rejected at dispatch with clear error message
- No runtime dependency, no AI code executed

### Added — Observability (Part 13)
- Structured logging via tracing
- Health monitor with aggregate status (Healthy/Degraded/Unhealthy)
- Metrics page with SVG visualizations
- Audit log with search, filters, export

### Added — Cluster Manager (Part 14)
- Node registration + heartbeat tracking
- Automatic failover detection (15s timeout → Offline)
- Load balancing (priority-weighted node selection)
- Per-region node discovery
- Cluster stats (by role, by region, by health)
- 24 unit tests

### Added — Workflow Engine (Part 4)
- Event-driven triggers (prefix match), Manual, Schedule
- 4 action types: PublishEvent, Log, Wait, Condition
- Trigger payload substitution ({{trigger}})
- Execution tracking with per-step results
- 21 unit tests

### Added — Billing Service
- Invoice lifecycle: Draft → Open → Paid/Void
- Payment lifecycle: Pending → Succeeded/Failed
- Subscription lifecycle: Active → Paused/Cancelled/Ended
- Auto-invoice generation on subscription creation
- Auto-mark-invoice-paid on successful payment (with amount validation)
- Revenue + outstanding tracking
- 17 unit tests

### Added — Notification Service
- Per-user notifications with severity levels (Info, Success, Warning, Error)
- Read/unread tracking + mark all read
- SSE live updates (notification.created events)
- 14 unit tests

### Added — Storage Layer
- SQLite backend for users, events, packages, billing
- Write-through cache pattern
- 6 tables + indexes
- 23 unit tests

### Statistics
- 15 Rust crates
- 15 frontend pages
- 55 HTTP routes
- 250+ unit tests
- 0 unsafe blocks
- 6 RFC documents
- 9 demo binaries
- ~16,000+ lines of Rust
- ~3,000+ lines of TypeScript/Svelte
