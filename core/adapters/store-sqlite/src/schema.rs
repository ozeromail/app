//! Forward-only migrations. Never edit a released one; append a new one.

pub const MIGRATIONS: &[&str] = &[
    // 1: accounts, folders, message headers.
    "
    CREATE TABLE account (
        id           INTEGER PRIMARY KEY,
        kind         TEXT NOT NULL,
        email        TEXT NOT NULL UNIQUE COLLATE NOCASE,
        display_name TEXT NOT NULL
    );
    CREATE TABLE folder (
        id         INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        remote_id  TEXT NOT NULL,
        name       TEXT NOT NULL,
        role       TEXT NOT NULL,
        UNIQUE (account_id, remote_id)
    );
    CREATE INDEX folder_role ON folder(role);
    CREATE TABLE message (
        id         INTEGER PRIMARY KEY,
        folder_id  INTEGER NOT NULL REFERENCES folder(id) ON DELETE CASCADE,
        remote_id  TEXT NOT NULL,
        message_id TEXT,
        from_name  TEXT,
        from_email TEXT NOT NULL,
        subject    TEXT NOT NULL,
        snippet    TEXT NOT NULL,
        date       INTEGER NOT NULL,
        flags      INTEGER NOT NULL,
        UNIQUE (folder_id, remote_id)
    );
    CREATE INDEX message_list ON message(folder_id, date DESC, id DESC);
    -- Unified lists walk messages by date across folders and stop at the page limit.
    CREATE INDEX message_date ON message(date DESC, id DESC);
    ",
    // 2: what the folder's remote ids mean (IMAP UIDVALIDITY).
    "ALTER TABLE folder ADD COLUMN validity INTEGER NOT NULL DEFAULT 0;",
];
