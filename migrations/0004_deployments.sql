CREATE TABLE deployments (
    id TEXT PRIMARY KEY NOT NULL,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    commit_sha TEXT,
    commit_message TEXT,
    commit_author TEXT,
    status TEXT NOT NULL CHECK (status IN ('queued','fetching','checking_out','preparing','installing_tools','installing_dependencies','building','publishing','validating','activating','success','failed','cancelled')),
    triggered_by TEXT NOT NULL DEFAULT 'manual',
    build_settings_snapshot TEXT NOT NULL,
    config_snapshot TEXT,
    release_path TEXT,
    error_summary TEXT,
    log TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TEXT,
    finished_at TEXT
);

CREATE INDEX deployments_site_created_idx ON deployments(site_id, created_at DESC);
CREATE UNIQUE INDEX deployments_one_active_per_site_idx ON deployments(site_id)
WHERE status IN ('queued','fetching','checking_out','preparing','installing_tools','installing_dependencies','building','publishing','validating','activating');
