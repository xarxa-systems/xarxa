-- Add up migration script here
ALTER TABLE workflows
  ADD CONSTRAINT fk_workflows_active_version
  FOREIGN KEY (active_version_id) REFERENCES workflow_versions(id)
  ON DELETE SET NULL;