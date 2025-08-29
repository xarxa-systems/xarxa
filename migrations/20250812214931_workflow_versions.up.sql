-- Add up migration script here
CREATE TABLE workflow_versions (
  id                UUID PRIMARY KEY, 
  workflow_id       UUID NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  version           TEXT NOT NULL,                            -- 0.1.1, 0.2.1, 3.0.18....
  wasm_md5          BYTEA NOT NULL,                           -- hashsum
  wasm_size_bytes   BIGINT NOT NULL,
  storage_url       TEXT,                              
  created_by        UUID NOT NULL REFERENCES users(id),
  changelog         TEXT,
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT uniq_wf_ver UNIQUE (workflow_id, version),
  CONSTRAINT uniq_wf_wasm_md5 UNIQUE (workflow_id, wasm_md5)
);
