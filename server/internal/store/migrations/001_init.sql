-- 001_init.sql: Initial schema for Zima Connect server.

CREATE TABLE IF NOT EXISTS devices (
    id         TEXT PRIMARY KEY,
    name       TEXT    NOT NULL,
    platform   TEXT    NOT NULL DEFAULT '',
    public_key TEXT    NOT NULL DEFAULT '',
    paired_at  TEXT,
    last_seen  TEXT,
    is_online  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS messages (
    id          TEXT PRIMARY KEY,
    from_device TEXT    NOT NULL,
    to_device   TEXT    NOT NULL,
    type        TEXT    NOT NULL DEFAULT 'generic',
    payload     TEXT    NOT NULL DEFAULT '',
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    expires_at  TEXT,
    delivered_at TEXT,
    acked       INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (from_device) REFERENCES devices(id),
    FOREIGN KEY (to_device)   REFERENCES devices(id)
);

CREATE INDEX IF NOT EXISTS idx_messages_to_device   ON messages(to_device);
CREATE INDEX IF NOT EXISTS idx_messages_pending     ON messages(to_device, delivered_at, acked);
CREATE INDEX IF NOT EXISTS idx_messages_expires     ON messages(expires_at);

CREATE TABLE IF NOT EXISTS files (
    ref_id       TEXT PRIMARY KEY,
    from_device  TEXT    NOT NULL,
    to_device    TEXT    NOT NULL,
    filename     TEXT    NOT NULL,
    size         INTEGER NOT NULL DEFAULT 0,
    checksum     TEXT    NOT NULL DEFAULT '',
    path         TEXT    NOT NULL DEFAULT '',
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    expires_at   TEXT,
    downloaded_at TEXT,
    FOREIGN KEY (from_device) REFERENCES devices(id),
    FOREIGN KEY (to_device)   REFERENCES devices(id)
);

CREATE INDEX IF NOT EXISTS idx_files_to_device ON files(to_device);
CREATE INDEX IF NOT EXISTS idx_files_expires   ON files(expires_at);

CREATE TABLE IF NOT EXISTS pairing_requests (
    id            TEXT PRIMARY KEY,
    initiator_id  TEXT    NOT NULL,
    target_id     TEXT    NOT NULL,
    pin           TEXT    NOT NULL,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    expires_at    TEXT    NOT NULL,
    confirmed     INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (initiator_id) REFERENCES devices(id),
    FOREIGN KEY (target_id)    REFERENCES devices(id)
);

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
