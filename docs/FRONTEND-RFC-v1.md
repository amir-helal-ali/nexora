# Nexora Frontend — RFC v1.0 (Draft)

**Status:** Draft
**Last Updated:** 2026-07-01
**Document Owner:** Nexora Platform Engineering
**Classification:** Internal — Engineering Specification
**Implements:** Nexora Engineering Specification, Part 7 (Frontend Architecture)

---

## 1. Abstract

The Nexora Frontend is a **Live Operational Interface** for Nexora Core.
It is NOT a website or a dashboard — it is a hybrid between an operating
system UI (like macOS), a developer platform (like GitHub/Vercel), and a
real-time control plane (like Cloudflare).

This implements Part 7 §"MISSION":
> The Nexora Frontend is a Live Operational Interface for the Nexora Core.
> Every UI action is a direct interaction with the Core via NXP.

## 2. Tech Stack (Part 7 mandatory)

| Layer | Choice | Compliant |
|-------|--------|-----------|
| Framework | SvelteKit 2 | ✅ |
| Language | TypeScript strict | ✅ |
| Styling | TailwindCSS 3 | ✅ |
| Font | Inter (UI) + JetBrains Mono (code) | ✅ |
| API client | Custom fetch wrapper with Bearer token | ✅ |
| State management | Svelte stores | ✅ |
| Routing | SvelteKit file-based routing | ✅ |

## 3. Architecture

```
   Browser (SvelteKit)
        │
        │ 1. Vite dev proxy: /api/* → http://127.0.0.1:8080
        │ 2. Production: same-origin or CORS-configured gateway
        │
        ▼
   ┌─────────────────┐
   │  API Gateway    │  (nexora-gateway, port 8080)
   │  HTTP ↔ NXP     │
   └────────┬────────┘
            │
            ▼
   ┌─────────────────┐
   │  AuthHandler    │  ← in-process
   │  CoreHandler    │  ← in-process
   └─────────────────┘
```

## 4. Project Layout

```
frontend/
├── package.json                # SvelteKit 2 + Svelte 5 + Tailwind 3
├── svelte.config.js
├── vite.config.ts              # /api proxy → gateway:8080
├── tailwind.config.js          # Custom Nexora palette
├── tsconfig.json               # Strict mode
└── src/
    ├── app.html                # HTML shell (Inter + JetBrains Mono)
    ├── app.css                 # Tailwind + custom utility classes
    ├── lib/
    │   ├── api/
    │   │   └── gateway.ts      # Typed API client (login, ping, events, modules, health)
    │   ├── stores/
    │   │   └── session.ts      # Svelte store for auth state
    │   └── components/
    │       ├── Layout.svelte   # Top nav + footer wrapper
    │       └── StatCard.svelte # Stat card component
    └── routes/
        ├── +layout.svelte      # Root layout (loads app.css)
        ├── +layout.ts          # Auth guard (redirects to /login if unauthenticated)
        ├── +page.svelte        # Dashboard: stats, ping, publish event, recent events
        ├── login/+page.svelte  # Login form
        ├── logout/+page.svelte # Logout redirect
        ├── events/+page.svelte # Event log with filter
        ├── modules/+page.svelte# Module grid
        └── health/+page.svelte # Subsystem health grid
```

## 5. Pages

### 5.1 `/login`
- Username + password form
- Calls `POST /api/auth/login` via Vite proxy
- Stores token in localStorage (`nexora.token`, `nexora.token_expires_at_ns`)
- Redirects to `/` on success
- Shows demo credentials hint (admin/admin123)

### 5.2 `/` (Dashboard)
- Stats grid: Modules count, latest event ID, overall health, subsystem count
- PING panel: button → `POST /api/core/ping` → shows pong + latency
- Publish Event panel: name + payload → `POST /api/core/events`
- Recent Events list: last 10 events (reversed)

### 5.3 `/events`
- Full event log with name-prefix filter
- Calls `GET /api/core/events?from_id=0&filter=...`
- Sorted newest-first

### 5.4 `/modules`
- Grid of installed modules
- Per-module: name, version, state (badge), owner, capabilities, transitions
- State badges: enabled (green), paused (amber), removed (red), installed (gray)

### 5.5 `/health`
- Grid of subsystem health cards
- Per-subsystem: name, status badge, message, last check time
- Overall status shown in header

## 6. Auth Flow

```
1. User visits /  →  +layout.ts auth guard checks localStorage
2. No token → redirect to /login
3. User submits login form → POST /api/auth/login
4. Gateway translates JSON → MessagePack → dispatches AUTH_LOGIN
5. AuthHandler verifies Argon2id password, issues Ed25519 token
6. Response: { token, expires_at, session_id, user_id, username }
7. Frontend stores token in localStorage
8. Subsequent requests include `Authorization: Bearer <token>` header
9. Gateway's require_token middleware verifies the token on every protected route
10. On 401, frontend clears token and redirects to /login
```

## 7. Design System

Per Part 7 §"UI PHILOSOPHY": Linear.app speed, Vercel simplicity, Notion
flexibility, Cloudflare density, Apple polish — with strict minimalism.

- **Palette**: Dark-first (zinc-950 bg, zinc-900 surface, zinc-800 border)
- **Accent**: Blue-500 (interactive elements)
- **Status**: emerald (success), amber (warning), red (error)
- **Typography**: Inter for UI, JetBrains Mono for code/IDs
- **Spacing**: Tailwind's default spacing scale
- **Components**: rounded-md/lg, border, subtle backgrounds, no shadows
- **Animations**: None except opacity/transition-colors (per "no unnecessary animations")

## 8. Compliance

| Spec Section | Status |
|--------------|--------|
| Part 7 — SvelteKit only | ✅ |
| Part 7 — TypeScript only | ✅ (strict mode) |
| Part 7 — TailwindCSS | ✅ |
| Part 7 — No business logic in frontend | ✅ (all logic in Core/Auth) |
| Part 7 — Frontend is thin real-time projection | ✅ |
| Part 7 — Server-authoritative state | ✅ (every action → backend) |
| Part 7 — All actions via NXP commands | ✅ (via Gateway → handlers) |
| Part 7 — Keyboard-first (TODO) | ⏳ Command palette pending |
| Part 7 — Theme engine (TODO) | ⏳ Dark mode only in v0.1 |
| Part 7 — Offline mode (TODO) | ⏳ Pending v0.2 |
| Part 7 — Plugin UI system (TODO) | ⏳ Pending v0.2 |

## 9. Future Work (v0.2+)

- Command palette (Cmd+K) for keyboard-first navigation
- Light/high-contrast themes
- Service Worker for offline mode
- WebSocket / SSE for live event streaming (currently polling)
- Plugin UI system (render UI from server schemas)
- IndexedDB for offline snapshots
- Multi-tab session sync
- Full accessibility audit (ARIA, focus management, screen reader)
