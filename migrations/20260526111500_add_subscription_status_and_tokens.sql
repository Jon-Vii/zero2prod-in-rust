ALTER TABLE subscriptions ADD COLUMN status TEXT NULL;
UPDATE subscriptions SET status = 'confirmed';
ALTER TABLE subscriptions ALTER COLUMN status SET NOT NULL;

CREATE TABLE subscription_tokens(
    subscription_token TEXT NOT NULL,
    subscriber_id uuid NOT NULL
        REFERENCES subscriptions (id),
    PRIMARY KEY (subscription_token)
);
