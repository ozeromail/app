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
    // 3: IMAP server settings, NULL for other kinds.
    "
    ALTER TABLE account ADD COLUMN server_host TEXT;
    ALTER TABLE account ADD COLUMN server_port INTEGER;
    ALTER TABLE account ADD COLUMN server_user TEXT;
    ",
    // 4: message bodies. Kept apart from the list columns so list reads stay small.
    "
    CREATE TABLE message_body (
        message_id INTEGER PRIMARY KEY REFERENCES message(id) ON DELETE CASCADE,
        text       TEXT NOT NULL
    );
    ",
    // 5: the HTML part of a body. Saved bodies are only a cache, so they download again with it.
    "
    DELETE FROM message_body;
    ALTER TABLE message_body ADD COLUMN html TEXT;
    ",
    // 6: actions waiting for the server. `undo` is the value before the first queued change.
    "
    CREATE TABLE op (
        id         INTEGER PRIMARY KEY,
        message_id INTEGER NOT NULL REFERENCES message(id) ON DELETE CASCADE,
        kind       TEXT NOT NULL,
        value      INTEGER NOT NULL,
        undo       INTEGER NOT NULL,
        not_before INTEGER NOT NULL,
        UNIQUE (message_id, kind)
    );
    ",
    // 7: the folder a move goes to, chosen when it is queued.
    "ALTER TABLE op ADD COLUMN target INTEGER REFERENCES folder(id) ON DELETE SET NULL;",
    // 8: the inbox tab. The next sync sorts known mail; a tab walks its own index by date.
    "
    ALTER TABLE message ADD COLUMN category TEXT NOT NULL DEFAULT 'people';
    CREATE INDEX message_tab ON message(category, date DESC, id DESC);
    -- Tab counts read only unread rows; `1` is the seen bit.
    CREATE INDEX message_unread ON message(folder_id, category) WHERE flags & 1 = 0;
    ",
];
