-- Add up migration script here
CREATE TABLE workflow_runs (
  id                    UUID PRIMARY KEY,
  workflow_version_id   UUID REFERENCES workflow_versions(id) ON DELETE SET NULL,
  input                 JSONB,
  state                 TEXT,                                -- running/succeeded/failed/...
  started_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);