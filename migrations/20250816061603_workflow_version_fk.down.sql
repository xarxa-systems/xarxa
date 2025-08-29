-- Add down migration script here
ALTER TABLE workflows 
DROP CONSTRAINT fk_workflows_active_version;