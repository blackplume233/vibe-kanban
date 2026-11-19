ALTER TABLE workspaces ADD COLUMN local_issue_id BLOB REFERENCES issues(id) ON DELETE SET NULL;

CREATE INDEX idx_workspaces_local_issue ON workspaces(local_issue_id)
    WHERE local_issue_id IS NOT NULL;
