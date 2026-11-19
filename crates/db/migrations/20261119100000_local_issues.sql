CREATE TABLE project_statuses (
    id BLOB NOT NULL PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    color TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('backlog', 'unstarted', 'started', 'completed', 'canceled')),
    sort_order INTEGER NOT NULL,
    UNIQUE (project_id, id),
    UNIQUE (project_id, name)
);

CREATE TABLE issue_cycles (
    id BLOB NOT NULL PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    start_date TEXT NOT NULL,
    end_date TEXT NOT NULL CHECK (end_date >= start_date),
    UNIQUE (project_id, id),
    UNIQUE (project_id, name)
);

CREATE TABLE issues (
    issue_number INTEGER PRIMARY KEY AUTOINCREMENT,
    id BLOB NOT NULL UNIQUE,
    simple_id TEXT GENERATED ALWAYS AS ('VO-' || issue_number) STORED UNIQUE,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    status_id BLOB NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT,
    priority TEXT CHECK (priority IN ('urgent', 'high', 'medium', 'low')),
    start_date TEXT,
    target_date TEXT,
    sort_order REAL NOT NULL DEFAULT 0,
    parent_issue_id BLOB REFERENCES issues(id) ON DELETE SET NULL CHECK (parent_issue_id <> id),
    cycle_id BLOB REFERENCES issue_cycles(id) ON DELETE SET NULL,
    triage INTEGER NOT NULL DEFAULT 0 CHECK (triage IN (0, 1)),
    extension_metadata TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(extension_metadata)),
    external_provider TEXT,
    external_id TEXT,
    external_url TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    UNIQUE (project_id, id),
    CHECK (start_date IS NULL OR target_date IS NULL OR start_date <= target_date),
    CHECK ((external_provider IS NULL) = (external_id IS NULL)),
    FOREIGN KEY (project_id, status_id) REFERENCES project_statuses(project_id, id),
    -- The single-column foreign keys above clear the optional IDs on deletion;
    -- these two also require that non-null references belong to this project.
    FOREIGN KEY (project_id, parent_issue_id) REFERENCES issues(project_id, id),
    FOREIGN KEY (project_id, cycle_id) REFERENCES issue_cycles(project_id, id)
);

CREATE TRIGGER issue_parent_must_be_acyclic
BEFORE UPDATE OF parent_issue_id ON issues
WHEN NEW.parent_issue_id IS NOT NULL AND NEW.parent_issue_id IS NOT OLD.parent_issue_id
BEGIN
    SELECT RAISE(ABORT, 'An issue cannot be its own ancestor')
    WHERE EXISTS (
        WITH RECURSIVE ancestors(id) AS (
            VALUES (NEW.parent_issue_id)
            UNION ALL
            SELECT issues.parent_issue_id FROM issues JOIN ancestors USING (id)
            WHERE issues.parent_issue_id IS NOT NULL
        )
        SELECT 1 FROM ancestors WHERE id = NEW.id
    );
END;

CREATE INDEX idx_issues_project_sort ON issues (project_id, sort_order, issue_number);
CREATE INDEX idx_issues_status ON issues (project_id, status_id);
CREATE INDEX idx_issues_parent ON issues (parent_issue_id);
CREATE INDEX idx_issues_cycle ON issues (cycle_id);
CREATE UNIQUE INDEX idx_issues_external_source ON issues (project_id, external_provider, external_id)
    WHERE external_provider IS NOT NULL;

-- The existing tags table stores text shortcuts. Issue labels have their own project scope.
CREATE TABLE issue_labels (
    id BLOB NOT NULL PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL COLLATE NOCASE CHECK (length(trim(name)) > 0),
    color TEXT NOT NULL,
    UNIQUE (project_id, id),
    UNIQUE (project_id, name)
);

CREATE TABLE issue_tags (
    project_id BLOB NOT NULL,
    issue_id BLOB NOT NULL,
    tag_id BLOB NOT NULL,
    PRIMARY KEY (issue_id, tag_id),
    FOREIGN KEY (project_id, issue_id) REFERENCES issues(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, tag_id) REFERENCES issue_labels(project_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_issue_tags_label ON issue_tags (project_id, tag_id);

CREATE TABLE issue_relationships (
    id BLOB NOT NULL PRIMARY KEY,
    project_id BLOB NOT NULL,
    issue_id BLOB NOT NULL,
    related_issue_id BLOB NOT NULL CHECK (related_issue_id <> issue_id),
    relationship_type TEXT NOT NULL CHECK (relationship_type IN ('blocking', 'related', 'duplicate')),
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    UNIQUE (project_id, issue_id, related_issue_id, relationship_type),
    CHECK (relationship_type <> 'related' OR issue_id < related_issue_id),
    FOREIGN KEY (project_id, issue_id) REFERENCES issues(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, related_issue_id) REFERENCES issues(project_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_issue_relationships_incoming ON issue_relationships (project_id, related_issue_id);

CREATE TABLE issue_comments (
    id BLOB NOT NULL PRIMARY KEY,
    project_id BLOB NOT NULL,
    issue_id BLOB NOT NULL,
    parent_id BLOB REFERENCES issue_comments(id) ON DELETE SET NULL CHECK (parent_id <> id),
    message TEXT NOT NULL CHECK (length(trim(message)) > 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    UNIQUE (issue_id, id),
    FOREIGN KEY (issue_id, parent_id) REFERENCES issue_comments(issue_id, id),
    FOREIGN KEY (project_id, issue_id) REFERENCES issues(project_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_issue_comments_issue ON issue_comments (project_id, issue_id, created_at);
CREATE INDEX idx_issue_comments_parent ON issue_comments (parent_id);

CREATE TABLE issue_saved_views (
    id BLOB NOT NULL PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    filter TEXT NOT NULL CHECK (json_valid(filter) AND json_type(filter) = 'object'),
    UNIQUE (project_id, name)
);
