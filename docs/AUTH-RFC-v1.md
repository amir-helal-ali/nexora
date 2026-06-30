# Nexora Auth/Identity Service — RFC v1.0 (Draft)

**Status:** Draft
**Last Updated:** 2026-07-01
**Document Owner:** Nexora Platform Engineering
**Classification:** Internal — Engineering Specification
**Implements:** Nexora Engineering Specification, Part 9 (Security & Zero Trust) — Authentication subsystem

---

## 1. Abstract

Nexora Auth is the first production service built on top of Nexora Core. It
provides user management, password hashing, session tokens, and an NXP
handler that dispatches `AUTH_LOGIN`, `AUTH_LOGOUT`, and `AUTH_REFRESH`
opcodes.

This service demonstrates the canonical pattern for building a Nexora
service:
- Owns its data (UserStore, SessionStore) — no shared DB
- Integrates with Core subsystems (PermissionEngine, EventBus)
- Speaks NXP natively via a dedicated handler
- Emits events on every state change
- Signs tokens with a long-term Ed25519 identity key

## 2. Architecture

```
                        ┌──────────────────────┐
                        │   NXP Transport      │
                        └──────────┬───────────┘
                                   │
                        ┌──────────▼───────────┐
                        │   Auth NXP Handler   │
                        │ (AUTH_LOGIN/LOGOUT/  │
                        │       REFRESH)       │
                        └──────────┬───────────┘
                                   │
        ┌──────────────┬───────────┼──────────────┐
        │              │           │              │
┌───────▼────┐  ┌──────▼─────┐ ┌───▼──────┐ ┌─────▼──────┐
│ UserStore  │  │SessionStore│ │  Token   │ │   Core     │
│            │  │            │ │ Verifier │ │   (Arc)    │
│ Argon2id   │  │ UUIDs,     │ │ Ed25519  │ │            │
│ hashing    │  │ 1h TTL     │ │ 1h / 24h │ │ Permission │
│            │  │            │ │  TTL     │ │   Engine   │
└──────┬─────┘  └────────────┘ └──────────┘ │            │
       │                                    │  Event Bus │
       └─────────────► publishes events ───►│            │
                                              │  Principal │
                                              │  auto-     │
                                              │  register  │
                                              └────────────┘
```

## 3. Subsystems

### 3.1 Password Hashing (`password.rs`)

- **Algorithm:** Argon2id (RFC 9106)
- **Salt:** random 16 bytes per password
- **Parameters:** m=19456 KiB, t=2, p=1 (tuned for Tier-1 VPS)
- **Storage:** PHC string (`$argon2id$v=19$m=19456,t=2,p=1$...`)
- **Verification:** constant-time
- **Memory safety:** `HashedPassword` zeroizes on drop; `Debug` impl
  never leaks the hash

### 3.2 User Store (`users.rs`)

- **User ID:** UUID v4
- **Username:** case-insensitive (stored lowercase)
- **Password:** Argon2id hash
- **Email:** optional
- **Roles:** Vec<String> (synced to PermissionEngine)
- **Active flag:** inactive users cannot login

**Operations:** `create`, `get`, `get_by_username`, `verify`, `record_login`,
`delete`, `list`

**Auto-integrations:**
- On `create`: registers a `Principal` in the PermissionEngine
- On `create`: emits `user.created` event
- On `record_login`: emits `user.logged_in` event
- On `delete`: emits `user.deleted` event + removes from indices

### 3.3 Session Store (`store.rs`)

- **Session ID:** UUID v4
- **Default TTL:** 1 hour
- **Per-user session tracking** (multiple sessions per user allowed)
- **Operations:** `create`, `revoke`, `revoke_all_for_user`, `touch`,
  `get`, `list_for_user`, `list_active`, `reap_expired`

### 3.4 Token Verifier (`token.rs`)

- **Algorithm:** Ed25519 (RFC 8032)
- **Format:** `claims_msgpack || signature_64B`, base64url-encoded
- **Claims:** `{ sub, iat, exp, ver }`
- **TTLs:** 1h for access token, 24h for refresh
- **Versioning:** each `issue`/`refresh`/`revoke` increments the user's
  version, invalidating all prior tokens
- **Verification:** signature + expiry + version match
- **Drop safety:** signing key bytes zeroized on drop

**Operations:** `issue`, `verify`, `revoke`, `refresh`, `public_key`

### 3.5 Auth NXP Handler (`handler.rs`)

Dispatches three opcodes:

| Opcode | Request | Response |
|--------|---------|----------|
| `AUTH_LOGIN` | `{ username, password, client? }` | `{ token, expires_at, session_id, user_id, username }` |
| `AUTH_LOGOUT` | `{ token, session_id? }` | `{ ok: true }` |
| `AUTH_REFRESH` | `{ token }` | `{ token, expires_at }` |

Error mapping follows RFC §8:
- Wrong password → `AUTH/INVALID_CREDENTIALS` (0x0201)
- Expired token → `AUTH/TOKEN_EXPIRED` (0x0202)
- Tampered signature → `AUTH/INVALID_CREDENTIALS` (0x0201)
- Revoked token → `AUTH/TOKEN_EXPIRED` (0x0202)

## 4. Compliance

| Spec Section | Status |
|--------------|--------|
| Part 9 — Password hashing (Argon2id) | ✅ |
| Part 9 — Short-lived rotating sessions | ✅ (1h TTL, version increment on refresh) |
| Part 9 — Ed25519 service identity | ✅ |
| Part 9 — Forward secrecy (per-session keys) | ⏳ Token signing is long-term; session keys are NXP-level (Part 3) |
| Part 4 — Service auto-registers with Core | ✅ (via PermissionEngine + EventBus) |
| Part 4 — NXP handler dispatches opcodes | ✅ |
| Part 8 — Events emitted on every state change | ✅ |
| Part 6 — Database-per-service | ✅ (in-memory store; will be Postgres in v0.2) |

## 5. Test Coverage

30 unit tests + 8 end-to-end smoke scenarios, all passing:

**Password (4):** hash+verify roundtrip, salt uniqueness, invalid hash
rejection, debug-impl non-leak

**Users (7):** create+get (case-insensitive), duplicate rejection,
verify, inactive user, create-emits-event, record_login-emits-event,
delete-emits-event, auto-register-principal

**Tokens (7):** issue+verify, expiry, revoke, refresh-invalidates-old,
tampered-signature, string-roundtrip, different-keys-reject

**Sessions (6):** create+get, revoke, revoke_all_for_user, list_for_user,
touch, reap_expired

**Handler (5):** login-returns-token, wrong-password-fails,
login-then-logout, refresh-invalidates-old, login-emits-events

**Smoke test (8):** create user, login success, login wrong password,
refresh, old-token-revoked, logout, token-revoked-after-logout,
events-emitted

## 6. Future Work (v0.2+)

- Persist user store to PostgreSQL (currently in-memory)
- WebAuthn / Passkey support (Part 9 §AUTHENTICATION)
- MFA / TOTP
- OAuth2 / OIDC provider mode (currently consumer only)
- HSM-backed token signing
- Refresh token rotation with reuse detection
