# Nexora API Gateway — RFC v1.0 (Draft)

**Status:** Draft
**Last Updated:** 2026-07-01
**Document Owner:** Nexora Platform Engineering
**Classification:** Internal — Engineering Specification
**Implements:** Nexora Engineering Specification, Part 6 (BACKEND ARCHITECTURE) — API Gateway

---

## 1. Abstract

The Nexora API Gateway is the **only HTTP surface** of the platform. Every
HTTP request from browsers, mobile clients, curl, or third-party tools must
pass through this gateway. The gateway translates each HTTP request into
an NXP-style dispatch and forwards it to the appropriate in-process service
(Auth, Core).

This implements Part 6 §"API GATEWAY RULE":
> External communication ONLY via API Gateway. API Gateway responsibilities:
> Authentication, Rate limiting, Request validation, Routing to services,
> Protocol translation (HTTP ↔ NXP).

## 2. Architecture

```
  Browser / curl / external HTTP client
                   │
                   ▼
          ┌────────────────────┐
          │   API Gateway      │  ← axum HTTP server (port 8080)
          │   (this crate)     │
          └─────────┬──────────┘
                    │
                    │ 1. Parse JSON body
                    │ 2. Validate Bearer token (if protected route)
                    │ 3. Encode as MessagePack (NXP payload format)
                    │ 4. Dispatch to in-process handler
                    │ 5. Decode MessagePack response back to JSON
                    │
        ┌───────────┴────────────┐
        │                        │
┌───────▼────────┐      ┌────────▼─────────┐
│  AuthHandler   │      │   CoreHandler    │
│  (AUTH_LOGIN,  │      │  (PING, events,  │
│   LOGOUT,      │      │   modules, ...)  │
│   REFRESH)     │      │                  │
└────────────────┘      └──────────────────┘
```

In v0.1 the gateway dispatches **in-process** (no NXP-over-QUIC round-trip).
In v0.2+, the gateway will be a separate process that connects to remote
Core/Auth services over real NXP frames.

## 3. Routes

### 3.1 Public Routes (no auth required)

| Method | Path | Description |
|--------|------|-------------|
| `GET`  | `/api/health` | Gateway liveness probe |
| `GET`  | `/api/openapi.json` | OpenAPI 3.0 spec |
| `POST` | `/api/auth/login` | Exchange credentials for a token |
| `POST` | `/api/auth/refresh` | Rotate a valid token |

### 3.2 Protected Routes (Bearer token required)

| Method | Path | NXP Opcode | Description |
|--------|------|------------|-------------|
| `POST` | `/api/auth/logout` | `AUTH_LOGOUT` | Revoke a token |
| `POST` | `/api/core/ping` | `PING` | Round-trip through Core |
| `GET`  | `/api/core/events` | `REPLAY_EVENTS` | Replay events (query: `from_id`, `filter`) |
| `POST` | `/api/core/events` | `PUBLISH_EVENT` | Publish an event |
| `GET`  | `/api/core/modules` | — (direct) | List installed modules |
| `GET`  | `/api/core/modules/:id` | — (direct) | Get a single module |
| `GET`  | `/api/core/sessions` | — (direct) | Session count (debug) |
| `GET`  | `/api/core/health` | — (direct) | Aggregate Core health |

## 4. Token Middleware

All protected routes pass through `require_token` middleware:

1. Extract `Authorization: Bearer <token>` header
2. Parse base64url-encoded Ed25519-signed token
3. Verify signature against `TokenVerifier`
4. Check expiry (`exp` claim)
5. Check version (matches the user's current version — rejects rotated/revoked tokens)
6. Inject `AuthContext { user_id, version, issued_at, expires_at }` into request extensions

Failure responses:
- Missing header → `401 missing Bearer token`
- Malformed token → `401 invalid token: ...`
- Expired → `401 token expired`
- Revoked → `401 token revoked`
- Version mismatch → `401 token version mismatch (rotated)`
- Bad signature → `401 invalid token signature`

## 5. JSON ↔ MessagePack Translation

The gateway accepts JSON request bodies (per HTTP convention) and translates
to MessagePack for internal dispatch (per NXP protocol). Responses are
translated back to JSON.

| Layer | Format | Reason |
|-------|--------|--------|
| External (HTTP body) | JSON | Universal browser/tooling support |
| Internal dispatch (NXP payload) | MessagePack | Compact, binary, RFC-compliant |
| Response back to client | JSON | Universal browser/tooling support |

This satisfies Law 15 (Binary Communication) for internal traffic while
keeping JSON for external compatibility only.

## 6. Error Mapping

NXP errors map to HTTP status codes:

| NXP Scope | HTTP Status |
|-----------|-------------|
| `Protocol` | 400 Bad Request |
| `Auth` | 401 Unauthorized |
| `Authz` | 403 Forbidden |
| `Session` | 401 Unauthorized |
| `App` | 500 Internal Server Error |
| `Internal` | 500 Internal Server Error |

All errors return JSON:
```json
{ "ok": false, "error": "<message>" }
```

## 7. CORS & Limits

- `CorsLayer::permissive()` — allows any origin (tighten in production)
- `RequestBodyLimitLayer::new(16 MiB)` — max body size matches `MAX_PAYLOAD_LEN`
- `TraceLayer::new_for_http()` — request/response tracing via `tracing`

## 8. Test Coverage

9 unit + integration tests, all passing:

**Routes (3):** auth login success, auth login wrong password returns 401,
core ping success

**Server (6):** health endpoint, openapi endpoint, login endpoint success,
login endpoint wrong password, protected route without token returns 401,
protected route with valid token succeeds

**End-to-end (manual, via curl):** login, wrong password, protected route
without/with token, publish event, replay events (returns 4 events including
the user.created events auto-emitted by Auth), list modules, core health.

## 9. Compliance

| Spec Section | Status |
|--------------|--------|
| Part 6 — External communication only via API Gateway | ✅ |
| Part 6 — Authentication | ✅ (Bearer token middleware) |
| Part 6 — Request validation | ✅ (axum + serde validation) |
| Part 6 — Routing to services | ✅ (Auth vs Core dispatch) |
| Part 6 — Protocol translation (HTTP ↔ NXP) | ✅ (JSON ↔ MessagePack) |
| Part 6 — Rate limiting | ⏳ Pending (use `tower::limit::ConcurrencyLimit` in v0.2) |
| Part 9 — Bearer token validation | ✅ |
| Part 10 — Low-resource | ✅ (axum is lightweight) |

## 10. Future Work (v0.2+)

- Rate limiting per IP / per user / per token
- Real NXP-over-QUIC dispatch to remote services (currently in-process)
- WebSocket endpoint for streaming NXP events to browsers
- gRPC mirror of every route (for high-throughput clients)
- OAuth2 client-credentials flow for service-to-service auth
- Request signing (HMAC or Ed25519) for non-browser clients
- IP allowlists per route
- WAF integration (modsecurity rules)
