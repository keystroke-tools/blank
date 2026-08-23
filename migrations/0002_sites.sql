CREATE TABLE sites (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    repository_url TEXT NOT NULL,
    branch TEXT NOT NULL DEFAULT 'main',
    project_directory TEXT NOT NULL DEFAULT '.',
    install_command TEXT,
    build_command TEXT,
    publish_directory TEXT NOT NULL DEFAULT 'dist',
    build_enabled INTEGER NOT NULL DEFAULT 1 CHECK (build_enabled IN (0, 1)),
    auto_deploy INTEGER NOT NULL DEFAULT 0 CHECK (auto_deploy IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE site_domains (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    domain TEXT NOT NULL COLLATE NOCASE UNIQUE,
    PRIMARY KEY (site_id, domain)
);

CREATE INDEX site_domains_site_id_idx ON site_domains(site_id);
