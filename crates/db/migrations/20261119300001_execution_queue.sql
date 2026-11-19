CREATE TABLE execution_scheduler_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    max_concurrent INTEGER NOT NULL CHECK (max_concurrent BETWEEN 1 AND 4294967295)
);
INSERT INTO execution_scheduler_config (id, max_concurrent) VALUES (1, 4);

CREATE TABLE execution_queue (
    position INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_process_id BLOB NOT NULL UNIQUE REFERENCES execution_processes(id) ON DELETE CASCADE,
    dispatched INTEGER NOT NULL DEFAULT 0 CHECK (dispatched IN (0, 1)),
    priority INTEGER NOT NULL DEFAULT 0,
    queued_at TEXT NOT NULL
);
CREATE INDEX execution_queue_priority_fifo ON execution_queue(priority DESC, position ASC);

CREATE TABLE execution_follow_ups (
    session_id BLOB PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    payload TEXT NOT NULL,
    queued_at TEXT NOT NULL,
    revision BLOB NOT NULL
);
