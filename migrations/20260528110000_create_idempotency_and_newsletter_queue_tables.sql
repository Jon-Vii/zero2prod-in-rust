CREATE TABLE idempotency (
    user_id uuid NOT NULL REFERENCES users(user_id),
    idempotency_key TEXT NOT NULL,
    response_status_code SMALLINT NULL,
    response_location TEXT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY(user_id, idempotency_key)
);

CREATE TABLE newsletter_issues (
    newsletter_issue_id uuid NOT NULL,
    title TEXT NOT NULL,
    text_content TEXT NOT NULL,
    html_content TEXT NOT NULL,
    published_at timestamptz NOT NULL,
    PRIMARY KEY(newsletter_issue_id)
);

CREATE TABLE issue_delivery_queue (
    newsletter_issue_id uuid NOT NULL REFERENCES newsletter_issues(newsletter_issue_id),
    subscriber_email TEXT NOT NULL,
    PRIMARY KEY(newsletter_issue_id, subscriber_email)
);
