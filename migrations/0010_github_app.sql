CREATE TABLE github_app_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    app_id INTEGER NOT NULL,
    app_slug TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret TEXT NOT NULL,
    webhook_secret TEXT NOT NULL,
    private_key_pem TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE github_installations (
    installation_id INTEGER PRIMARY KEY,
    account_login TEXT NOT NULL,
    account_type TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
