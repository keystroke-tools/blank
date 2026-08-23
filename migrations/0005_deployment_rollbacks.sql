ALTER TABLE deployments ADD COLUMN rollback_of_deployment_id TEXT REFERENCES deployments(id);
