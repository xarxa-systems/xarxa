-- Add up migration script here
CREATE TABLE audit_logs (
  id            BIGSERIAL PRIMARY KEY,
  namespace_id  UUID REFERENCES namespaces(id) ON DELETE SET NULL,
  user_id       UUID REFERENCES users(id) ON DELETE SET NULL,
  action        TEXT NOT NULL,          -- 'workflow.create', 'workflow.publish', ...
  object_type   TEXT,                   -- 'workflow', 'namespace', ...
  object_id     UUID,
  meta          JSONB NOT NULL DEFAULT '{}',
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);