-- Add down migration script here

DROP INDEX IF EXISTS namespace_members_user_id_idx;
DROP TABLE IF EXISTS namespace_members;
DROP TYPE IF EXISTS namespace_role CASCADE;
DROP TABLE IF EXISTS namespaces;