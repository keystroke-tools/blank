CREATE TABLE administrators (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    identifier TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE sessions (
    token_hash BLOB PRIMARY KEY NOT NULL,
    administrator_id INTEGER NOT NULL REFERENCES administrators(id) ON DELETE CASCADE,
    csrf_token TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX sessions_expires_at_idx ON sessions(expires_at);
