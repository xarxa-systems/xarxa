-- Up
CREATE TABLE workflows (
  id                  UUID PRIMARY KEY,                   -- uuidv7, will be generated on code side
  namespace_id        UUID NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
  key                 CITEXT NOT NULL,                    -- unique workflow key for each namespace
  display_name        TEXT NOT NULL,                      -- workflow name, by default has the same name as wasm filename
  description         TEXT,
  active_version_id   UUID,                               -- link to workflow_versions.version
  is_archived         BOOLEAN NOT NULL DEFAULT false,
  created_by          UUID NOT NULL REFERENCES users(id),
  created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (namespace_id, key)
);

CREATE INDEX workflow_ns_id_idx ON workflows (namespace_id) WHERE NOT is_archived;

CREATE OR REPLACE FUNCTION workflows_update_updated_at_column() RETURNS TRIGGER AS $$
BEGIN NEW .updated_at = NOW();
RETURN NEW;
END;
$$ LANGUAGE 'plpgsql';

CREATE TRIGGER update_workflows_modtime BEFORE UPDATE
    ON workflows FOR EACH ROW EXECUTE PROCEDURE workflows_update_updated_at_column();
