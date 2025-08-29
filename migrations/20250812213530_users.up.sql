-- Add up migration script here
CREATE EXTENSION IF NOT EXISTS citext;
CREATE TABLE users (
  id            UUID PRIMARY KEY,                   -- uuidv7, will be generated on code side
  email         CITEXT UNIQUE NOT NULL,
  token_hash    TEXT NOT NULL,
  super_admin   BOOLEAN NOT NULL DEFAULT false,      
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);