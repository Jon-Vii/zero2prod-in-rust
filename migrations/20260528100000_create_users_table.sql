CREATE TABLE users(
    user_id uuid NOT NULL,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    PRIMARY KEY (user_id),
    UNIQUE (username)
);

INSERT INTO users (user_id, username, password_hash)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    'admin',
    '$argon2id$v=19$m=15000,t=2,p=1$gZiV/M1gPc22ElAH/Jh1Hw$CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno'
);
