-- Add up migration script here
CREATE TABLE namespaces (
  id            UUID PRIMARY KEY,                       -- uuidv7, will be generated on code side
  slug          TEXT UNIQUE NOT NULL,                   -- human friendly 'id'
  created_by    UUID NOT NULL REFERENCES users(id),
  created_at    TIMESTAMPTZ NOT NULL default now()
);

CREATE TYPE namespace_role AS ENUM ('owner','admin','editor','viewer');

CREATE TABLE namespace_members (
  namespace_id  UUID NOT NULL REFERENCES namespaces(id) ON DELETE CASCADE,
  user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role          namespace_role NOT NULL,
  joined_at     TIMESTAMPTZ NOT NULL default now(),
  PRIMARY KEY (namespace_id, user_id)
);

CREATE INDEX namespace_members_user_id_idx ON namespace_members (user_id);