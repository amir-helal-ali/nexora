# Changelog

All notable changes to Nexora are documented in this file.

## [1.3.0] — 2026-07-02

### Added — Email Notification Adapter
- EmailAdapter with SMTP configuration via environment variables
- 4 email methods: send, send_welcome, send_payment_confirmation, send_security_alert
- Severity-prefixed subject lines ([INFO], [SUCCESS], [WARNING], [ALERT])
- No-op fallback when SMTP not configured (logging only)
- 7 unit tests

### Added — Global Search
- GET /api/search?q=... — unified search across 5 data types:
  events, packages, users, workflows, notifications
- Frontend /search page with debounced input, grouped results, color-coded types
- EventPayload::payload_text() helper for search/display
- 64 HTTP routes total

### Added — System Info & About Page
- Frontend /about page with platform summary, tech stack, compliance table,
  16 Rust crates table, 9 services grid, live stats, uptime counter

### Added — PgTenancyStore (7th PostgreSQL store)
- PostgreSQL-native store for organizations, memberships, teams
- All 7 PostgreSQL stores now complete:
  PgUserStore, PgEventStore, PgPackageStore, PgBillingStore,
  PgWorkflowStore, PgNotificationStore, PgTenancyStore

### Statistics (v1.3.0)
- 16 Rust crates
- 18 frontend pages (+2: search, about)
- 64 HTTP routes (+1: global search)
- 330+ unit tests (+10)
- 7 PostgreSQL-native stores (+1: PgTenancyStore)
- 13 SQLite tables
- 0 unsafe blocks
- Email adapter with SMTP support
- Global search across all platform data

## [1.2.0] — 2026-07-02

### Changed — PostgreSQL is now the default database
- nexora-storage: `default = ["postgres"]` (was `["sqlite"]`)
- PostgreSQL is always-on in docker-compose (not optional profile)
- DATABASE_URL=postgresql://nexora:nexora@postgres:5432/nexora
- SQLite remains available for Tier-1 edge only (`--no-default-features --features sqlite`)
- Dockerfile.backend: added libpq-dev (build) + libpq5 (runtime) + curl

### Added — PostgreSQL-native stores (6)
- PgUserStore: create, count, record_login, delete
- PgEventStore: publish (write + broadcast), replay (with filter), count
- PgPackageStore: save, mark_installed, mark_uninstalled, count
- PgBillingStore: save_invoice, save_payment, save_subscription, 3 counts
- PgWorkflowStore: save_workflow, save_execution, 2 counts
- PgNotificationStore: save, mark_read, count, unread_count
- All use bb8 connection pool + ON CONFLICT UPSERT + BIGSERIAL + BYTEA + LIKE

### Added — Workflow Persistence
- 2 new SQLite tables: workflows, workflow_executions
- SqliteWorkflowStore: save_workflow, save_execution, load_workflows, load_executions
- JSON serialization for triggers, steps, step results
- 9 unit tests

### Added — Notification Persistence
- New SQLite table: notifications (with indexes on user_id + unread)
- SqliteNotificationStore: save, mark_read, mark_all_read, delete, delete_read,
  count, unread_count, load_for_user, load_all
- 12 unit tests

### Added — Organization/Tenancy Persistence
- 3 new SQLite tables: organizations, org_memberships, teams
- SqliteTenancyStore: save_org, save_membership, delete_membership, save_team,
  delete_team, org_count, membership_count, team_count
- 6 unit tests

### Statistics (v1.2.0)
- 16 Rust crates
- 16 frontend pages
- 63 HTTP routes
- 320+ unit tests (+50 since v1.1.0)
- 13 SQLite tables (100% persistence coverage)
- 6 PostgreSQL-native stores
- 0 unsafe blocks
- Database: PostgreSQL (primary) + SQLite (edge fallback)

## [1.1.0] — 2026-07-02

### Added — Multi-Tenancy (Part 2 Law 23)
- nexora-tenancy crate: organizations, teams, memberships
- 5 org tiers: Individual, Team, Organization, Enterprise, MSP
- 5 roles: Owner, Admin, Member, Viewer, Billing (hierarchical)
- Max members per tier enforcement
- Team creation + member management
- 8 HTTP routes + 20 unit tests
- Frontend /organizations page with create + member management

### Added — API Rate Limiting (Part 6 + Part 9)
- Sliding window rate limiter (100 req/60s default)
- Per-user + per-IP client identification
- HTTP 429 with X-RateLimit-* headers
- Configurable + disable-able
- 9 unit tests

### Added — Audit Log Page (Part 9)
- Immutable event history with 4 filters (text, category, time-range, sort)
- Category badges (clickable filters)
- JSON export
- Color-coded categories
- Frontend /audit page

### Added — Metrics & Analytics (Part 13)
- 4 pure SVG chart components (Sparkline, Donut, BarChart, Gauge)
- Event Activity sparkline (time-series)
- Revenue Trend sparkline
- Cluster Health donut
- Billing Breakdown donut
- Workflow Success Rate gauge
- Event Distribution bar chart
- Platform Summary stats grid
- Auto-refresh every 15 seconds + SSE live updates
- Frontend /metrics page

### Added — API Explorer
- 24 curated endpoints across 10 categories
- Request builder (path params, query, JSON body)
- Response viewer (status, time, formatted JSON)
- Auto-auth with Bearer token
- Frontend /api-explorer page

### Added — Settings & User Management
- 6 HTTP routes (list, create, delete users, profile, sessions, change password)
- Profile tab with API token display
- Sessions tab
- Users tab with create + delete
- Frontend /settings page

### Added — Cluster Topology Page
- Node grid with role icons + status badges
- Register new node form
- Send heartbeat per node
- Stats grid + by-role/by-region breakdowns
- Frontend /cluster page

### Added — Workflow Management Page
- Create form with step builder (4 action types)
- Trigger button per workflow
- Execution history with per-step results
- Frontend /workflows page

### Added — WebSocket Terminal
- Bidirectional real-time communication
- 6 message types (ping, publish_event, core_ping, billing_stats, workflow_stats, marketplace_list)
- Live event push from EventBus
- Frontend /terminal page

### Added — Notification Bell
- Live unread count + dropdown panel
- Mark read + mark all read
- SSE live updates
- Severity-colored icons

### Added — Unified Dashboard Stats API
- GET /api/dashboard/stats — single endpoint aggregating all 9 services
- Parallel aggregation via tokio::join!
- Redesigned Dashboard with service stats grid

### Added — User Management API
- 6 routes for user CRUD + profile + sessions + password change

### Statistics (v1.1.0)
- 16 Rust crates (+1: nexora-tenancy)
- 16 frontend pages (+6: metrics, audit, settings, organizations, workflows, cluster, terminal, api-explorer)
- 63 HTTP routes (+8: tenancy)
- 270+ unit tests (+20: tenancy)
- 0 unsafe blocks
- 4 SVG chart components
- 14 nav items

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
