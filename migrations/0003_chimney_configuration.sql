CREATE TABLE site_chimney_configs (
    site_id TEXT PRIMARY KEY NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    config_json TEXT NOT NULL,
    config_toml TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('generated', 'repository', 'dashboard')),
    imported_hash TEXT,
    imported_commit TEXT,
    upstream_hash TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
