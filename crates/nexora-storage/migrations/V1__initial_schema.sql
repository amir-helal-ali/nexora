-- Nexora PostgreSQL schema v1.
-- Tables for: users, sessions, events, packages, billing, audit, secrets.

-- 1. Users
CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    email           TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    roles           JSONB   NOT NULL DEFAULT '[]'::jsonb,
    created_at      BIGINT  NOT NULL,
    updated_at      BIGINT  NOT NULL,
    disabled        BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX IF NOT EXISTS idx_users_email ON users (email);

-- 2. Sessions (auth tokens)
CREATE TABLE IF NOT EXISTS sessions (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    issued_at       BIGINT NOT NULL,
    expires_at      BIGINT NOT NULL,
    token_version   BIGINT NOT NULL,
    revoked         BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions (user_id);

-- 3. Events (event sourcing log)
CREATE TABLE IF NOT EXISTS events (
    id              BIGSERIAL PRIMARY KEY,
    name            TEXT NOT NULL,
    payload_kind    SMALLINT NOT NULL,
    payload_bytes   BYTEA NOT NULL,
    occurred_at     BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_name ON events (name);
CREATE INDEX IF NOT EXISTS idx_events_occurred ON events (occurred_at);

-- 4. Packages (marketplace catalog)
CREATE TABLE IF NOT EXISTS packages (
    id                  TEXT NOT NULL,
    version             TEXT NOT NULL,
    name                TEXT NOT NULL,
    package_type        TEXT NOT NULL,
    owner_public_key    TEXT NOT NULL,
    owner_name          TEXT NOT NULL,
    capabilities        JSONB NOT NULL DEFAULT '[]'::jsonb,
    resource_limits     JSONB NOT NULL,
    dependencies        JSONB NOT NULL DEFAULT '[]'::jsonb,
    nxp_capabilities    JSONB NOT NULL DEFAULT '[]'::jsonb,
    core_compatibility  TEXT NOT NULL,
    billing             TEXT NOT NULL,
    visibility          TEXT NOT NULL,
    signature           TEXT NOT NULL,
    description         TEXT NOT NULL,
    readme              TEXT NOT NULL,
    tags                JSONB NOT NULL DEFAULT '[]'::jsonb,
    integrity_hash      TEXT NOT NULL,
    published_at        BIGINT NOT NULL,
    install_count       BIGINT NOT NULL DEFAULT 0,
    active_install_count BIGINT NOT NULL DEFAULT 0,
    installed           BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (id, version)
);
CREATE INDEX IF NOT EXISTS idx_packages_owner ON packages (owner_public_key);
CREATE INDEX IF NOT EXISTS idx_packages_name ON packages (name);

-- 5. Billing
CREATE TABLE IF NOT EXISTS invoices (
    id                TEXT PRIMARY KEY,
    customer_id       TEXT NOT NULL,
    customer_name     TEXT NOT NULL,
    items             JSONB NOT NULL,
    total_minor       BIGINT NOT NULL,
    currency          TEXT NOT NULL,
    status            TEXT NOT NULL,
    created_at        BIGINT NOT NULL,
    due_at            BIGINT NOT NULL,
    paid_at           BIGINT,
    subscription_id   TEXT,
    payment_ids       JSONB NOT NULL DEFAULT '[]'::jsonb
);
CREATE INDEX IF NOT EXISTS idx_invoices_customer ON invoices (customer_id);
CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices (status);

CREATE TABLE IF NOT EXISTS payments (
    id                TEXT PRIMARY KEY,
    invoice_id        TEXT,
    customer_id       TEXT NOT NULL,
    amount_minor      BIGINT NOT NULL,
    currency          TEXT NOT NULL,
    status            TEXT NOT NULL,
    provider          TEXT,
    provider_txn_id   TEXT,
    created_at        BIGINT NOT NULL,
    completed_at      BIGINT
);
CREATE INDEX IF NOT EXISTS idx_payments_customer ON payments (customer_id);
CREATE INDEX IF NOT EXISTS idx_payments_invoice ON payments (invoice_id);

CREATE TABLE IF NOT EXISTS subscriptions (
    id                TEXT PRIMARY KEY,
    customer_id       TEXT NOT NULL,
    package_id        TEXT NOT NULL,
    status            TEXT NOT NULL,
    period_start      BIGINT NOT NULL,
    period_end        BIGINT NOT NULL,
    amount_minor      BIGINT NOT NULL,
    currency          TEXT NOT NULL,
    created_at        BIGINT NOT NULL,
    cancelled_at      BIGINT
);
CREATE INDEX IF NOT EXISTS idx_subs_customer ON subscriptions (customer_id);
CREATE INDEX IF NOT EXISTS idx_subs_status ON subscriptions (status);

-- 6. Audit log
CREATE TABLE IF NOT EXISTS audit_log (
    id              BIGSERIAL PRIMARY KEY,
    actor           TEXT NOT NULL,
    action          TEXT NOT NULL,
    target          TEXT,
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at     BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_log (actor);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log (action);
CREATE INDEX IF NOT EXISTS idx_audit_occurred ON audit_log (occurred_at);

-- 7. Secrets (encrypted at rest by the secrets subsystem)
CREATE TABLE IF NOT EXISTS secrets (
    key             TEXT PRIMARY KEY,
    ciphertext      BYTEA NOT NULL,
    nonce           BYTEA NOT NULL,
    created_at      BIGINT NOT NULL,
    rotated_at      BIGINT
);
