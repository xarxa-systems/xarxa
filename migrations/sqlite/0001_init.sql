-- Local dev schema. No users, namespaces, or auth — plugins live on disk in plugins/.
-- Production uses PostgreSQL (see migrations/ root).
--
-- Recovery model:
--   The plugin owns replay logic — on resume the host passes back the full
--   serialized state (workflow_runs.state) and the plugin fast-forwards
--   through already-completed steps internally.
--   The host is responsible only for durably checkpointing that state blob
--   after each send-result call from the plugin.

-- One row per workflow execution.
-- status: running | completed | failed | suspended
CREATE TABLE IF NOT EXISTS workflow_runs (
    id         INTEGER  PRIMARY KEY AUTOINCREMENT,
    plugin     TEXT     NOT NULL,
    input      TEXT     NOT NULL,     -- original input passed to start-workflow
    state      BLOB,                  -- opaque state blob from the plugin (updated on each send-result)
    output     TEXT,                  -- final output when status = completed
    status     TEXT     NOT NULL DEFAULT 'running',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Append-only log of every send-result call from the plugin.
-- Useful for debugging and audit — not used for recovery (state blob is used instead).
CREATE TABLE IF NOT EXISTS workflow_events (
    id         INTEGER  PRIMARY KEY AUTOINCREMENT,
    run_id     INTEGER  NOT NULL REFERENCES workflow_runs(id),
    seq        INTEGER  NOT NULL,     -- monotonically increasing per run
    data       BLOB     NOT NULL,     -- raw bytes from send-result
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
