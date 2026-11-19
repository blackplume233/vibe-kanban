CREATE TABLE github_issue_sync (
    issue_id BLOB PRIMARY KEY NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    external_id TEXT NOT NULL,
    remote_issue_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    closed INTEGER NOT NULL CHECK (closed IN (0, 1))
);
