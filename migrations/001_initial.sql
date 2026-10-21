-- CNT Desk Multi-Tenant Schema
-- Migration: 001_initial.sql
-- Database: PostgreSQL (recommended) / MySQL compatible

-- ============================================================
-- TENANTS
-- ============================================================
CREATE TABLE IF NOT EXISTS tenants (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        VARCHAR(255) NOT NULL,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);

COMMENT ON TABLE tenants IS 'Multi-tenant root entity';

-- ============================================================
-- LICENSES
-- ============================================================
CREATE TABLE IF NOT EXISTS licenses (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         UUID         NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    max_simultaneous  INT          NOT NULL CHECK (max_simultaneous > 0),
    expires_at        TIMESTAMPTZ  NOT NULL,
    created_at        TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX idx_licenses_tenant       ON licenses(tenant_id);
CREATE INDEX idx_licenses_expires      ON licenses(expires_at);
CREATE INDEX idx_licenses_tenant_exp   ON licenses(tenant_id, expires_at);

COMMENT ON TABLE licenses IS 'Per-tenant license with concurrent access limit';

-- ============================================================
-- DEVICES
-- ============================================================
CREATE TABLE IF NOT EXISTS devices (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID         NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    device_key  VARCHAR(128) NOT NULL,
    tags        JSONB        NOT NULL DEFAULT '{}',
    last_seen   TIMESTAMPTZ,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_devices_tenant_key ON devices(tenant_id, device_key);
CREATE INDEX idx_devices_tenant           ON devices(tenant_id);
CREATE INDEX idx_devices_last_seen        ON devices(last_seen);
CREATE INDEX idx_devices_tags             ON devices USING GIN(tags);

COMMENT ON TABLE devices IS 'Registered devices per tenant with JSON tags';

-- ============================================================
-- SESSIONS
-- ============================================================
CREATE TABLE IF NOT EXISTS sessions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id   UUID         NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    viewer_id   VARCHAR(128) NOT NULL,
    started_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    ended_at    TIMESTAMPTZ
);

CREATE INDEX idx_sessions_device       ON sessions(device_id);
CREATE INDEX idx_sessions_viewer       ON sessions(viewer_id);
CREATE INDEX idx_sessions_started      ON sessions(started_at);
CREATE INDEX idx_sessions_active       ON sessions(device_id) WHERE ended_at IS NULL;

COMMENT ON TABLE sessions IS 'Active and historical remote desktop sessions';

-- ============================================================
-- DOWNLOAD LINKS
-- ============================================================
CREATE TABLE IF NOT EXISTS download_links (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID         NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    token       VARCHAR(255) NOT NULL,
    expires_at  TIMESTAMPTZ  NOT NULL,
    used        BOOLEAN      NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_download_links_token ON download_links(token);
CREATE INDEX idx_download_links_tenant       ON download_links(tenant_id);
CREATE INDEX idx_download_links_expires      ON download_links(expires_at);
CREATE INDEX idx_download_links_tenant_used  ON download_links(tenant_id, used) WHERE used = FALSE;

COMMENT ON TABLE download_links IS 'Ephemeral single-use download tokens per tenant';