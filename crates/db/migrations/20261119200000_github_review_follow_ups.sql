CREATE TABLE github_review_follow_ups (
    pull_request_id TEXT NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    comment_id INTEGER NOT NULL,
    PRIMARY KEY (pull_request_id, comment_id)
);
