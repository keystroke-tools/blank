CREATE TABLE administrators_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    identifier TEXT NOT NULL COLLATE NOCASE UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO administrators_new (id, identifier, password_hash, created_at)
SELECT id, identifier, password_hash, created_at FROM administrators;

CREATE TABLE sessions_new (
    token_hash BLOB PRIMARY KEY NOT NULL,
    administrator_id INTEGER NOT NULL REFERENCES administrators_new(id) ON DELETE CASCADE,
    csrf_token TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO sessions_new (token_hash, administrator_id, csrf_token, expires_at, created_at)
SELECT token_hash, administrator_id, csrf_token, expires_at, created_at FROM sessions;

DROP TABLE sessions;
DROP TABLE administrators;
ALTER TABLE administrators_new RENAME TO administrators;
ALTER TABLE sessions_new RENAME TO sessions;

CREATE INDEX sessions_expires_at_idx ON sessions(expires_at);
