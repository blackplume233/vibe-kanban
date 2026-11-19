CREATE TABLE execution_usage (
    id INTEGER PRIMARY KEY,
    execution_process_id BLOB NOT NULL REFERENCES execution_processes(id) ON DELETE CASCADE,
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    executor TEXT NOT NULL,
    model TEXT,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL,
    cost_usd REAL,
    recorded_at TEXT NOT NULL
);
CREATE INDEX execution_usage_session_time ON execution_usage(session_id, recorded_at);
CREATE INDEX execution_usage_time ON execution_usage(recorded_at);

CREATE TABLE execution_prices (
    executor TEXT NOT NULL,
    model TEXT NOT NULL,
    input_usd_per_million REAL NOT NULL,
    output_usd_per_million REAL NOT NULL,
    cache_read_usd_per_million REAL NOT NULL,
    cache_write_usd_per_million REAL NOT NULL,
    PRIMARY KEY (executor, model)
);

CREATE TABLE execution_usage_baselines (
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    executor TEXT NOT NULL,
    agent_session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    counters TEXT NOT NULL,
    PRIMARY KEY (session_id, executor, agent_session_id, model)
);
