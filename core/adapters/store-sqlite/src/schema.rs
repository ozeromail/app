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
    // 9: sending. Known IMAP accounts get the usual SMTP host next to their IMAP one, on 465.
    // `recipients` holds one address per line; `failed` is why the server refused it for good.
    "
    ALTER TABLE account ADD COLUMN smtp_host TEXT;
    ALTER TABLE account ADD COLUMN smtp_port INTEGER;
    ALTER TABLE account ADD COLUMN smtp_user TEXT;
    UPDATE account SET smtp_host = 'smtp.' || substr(server_host, 6), smtp_port = 465,
                       smtp_user = server_user
     WHERE server_host LIKE 'imap.%';
    CREATE TABLE outbox (
        id          INTEGER PRIMARY KEY,
        account_id  INTEGER NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        recipients  TEXT NOT NULL,
        subject     TEXT NOT NULL,
        text        TEXT NOT NULL,
        reply_to    INTEGER REFERENCES message(id) ON DELETE SET NULL,
        in_reply_to TEXT,
        message_id  TEXT NOT NULL,
        date        INTEGER NOT NULL,
        not_before  INTEGER NOT NULL,
        failed      TEXT
    );
    CREATE INDEX outbox_due ON outbox(account_id, not_before) WHERE failed IS NULL;
    ",
    // 10: `fresh` marks a message saved since the last `take_fresh`; mail already here is not.
    "
    ALTER TABLE message ADD COLUMN fresh INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX message_fresh ON message(id) WHERE fresh = 1;
    ",
    // 11: where the folder's last sync stopped (IMAP UIDNEXT); 0 means unknown.
    "ALTER TABLE folder ADD COLUMN next_id INTEGER NOT NULL DEFAULT 0;",
    // 12: conversations. `thread_id` is the id of one of its messages; a reset checkpoint
    // fetches the newest headers again, now with their references.
    "
    ALTER TABLE message ADD COLUMN thread_id INTEGER;
    UPDATE message SET thread_id = id;
    CREATE INDEX message_thread ON message(thread_id, folder_id, date DESC, id DESC);
    CREATE INDEX message_message_id ON message(message_id) WHERE message_id IS NOT NULL;
    CREATE TABLE message_ref (
        message_id INTEGER NOT NULL REFERENCES message(id) ON DELETE CASCADE,
        ref        TEXT NOT NULL,
        PRIMARY KEY (message_id, ref)
    ) WITHOUT ROWID;
    CREATE INDEX message_ref_ref ON message_ref(ref);
    UPDATE folder SET next_id = 0;
    ",
    // 13: full-text search over subject, sender and body, or the snippet until the body is
    // saved. `message_search_source` says what is indexed; triggers reindex a message from
    // it. `remove_diacritics 2` lets "e" find "é"; ё is folded by hand, the tokenizer keeps
    // it. The prefix indexes keep searches of the first letters typed quick.
    "
    CREATE VIRTUAL TABLE message_search USING fts5(
        subject, sender, text, tokenize = 'unicode61 remove_diacritics 2', prefix = '1 2 3'
    );
    CREATE VIEW message_search_source (id, subject, sender, text) AS
        SELECT m.id,
               replace(replace(m.subject, 'ё', 'е'), 'Ё', 'Е'),
               replace(replace(coalesce(m.from_name || ' ', '') || m.from_email, 'ё', 'е'), 'Ё', 'Е'),
               replace(replace(coalesce(b.text, m.snippet), 'ё', 'е'), 'Ё', 'Е')
        FROM message m LEFT JOIN message_body b ON b.message_id = m.id;
    INSERT INTO message_search (rowid, subject, sender, text)
        SELECT * FROM message_search_source;
    CREATE TRIGGER message_search_insert AFTER INSERT ON message BEGIN
        INSERT INTO message_search (rowid, subject, sender, text)
            SELECT * FROM message_search_source WHERE id = new.id;
    END;
    -- A sync rewrites these with each flag change; only a real change costs a reindex.
    CREATE TRIGGER message_search_update AFTER UPDATE OF subject, from_name, from_email, snippet
        ON message
        WHEN old.subject IS NOT new.subject OR old.from_name IS NOT new.from_name
            OR old.from_email IS NOT new.from_email OR old.snippet IS NOT new.snippet
    BEGIN
        DELETE FROM message_search WHERE rowid = new.id;
        INSERT INTO message_search (rowid, subject, sender, text)
            SELECT * FROM message_search_source WHERE id = new.id;
    END;
    CREATE TRIGGER message_search_delete AFTER DELETE ON message BEGIN
        DELETE FROM message_search WHERE rowid = old.id;
    END;
    CREATE TRIGGER message_search_body AFTER INSERT ON message_body BEGIN
        DELETE FROM message_search WHERE rowid = new.message_id;
        INSERT INTO message_search (rowid, subject, sender, text)
            SELECT * FROM message_search_source WHERE id = new.message_id;
    END;
    CREATE TRIGGER message_search_body_update AFTER UPDATE OF text ON message_body
        WHEN old.text IS NOT new.text
    BEGIN
        DELETE FROM message_search WHERE rowid = new.message_id;
        INSERT INTO message_search (rowid, subject, sender, text)
            SELECT * FROM message_search_source WHERE id = new.message_id;
    END;
    ",
    // 14: which colour marks an account's mail, kept so it stays when others come and go.
    "
    ALTER TABLE account ADD COLUMN color INTEGER NOT NULL DEFAULT 0;
    UPDATE account SET color = (SELECT count(*) FROM account a WHERE a.id < account.id);
    ",
    // 15: account ids are never used again, so nothing kept by id passes to a new account;
    // and the palette grew from six colours to ten, so each keeps its look.
    "
    CREATE TABLE account_seq (last INTEGER NOT NULL);
    INSERT INTO account_seq SELECT coalesce(max(id), 0) FROM account;
    UPDATE account SET color = CASE color % 6
        WHEN 0 THEN 1 WHEN 1 THEN 2 WHEN 2 THEN 3 WHEN 3 THEN 5 WHEN 4 THEN 8 ELSE 9 END;
    ",
    // 16: who a message also went to, for Reply all, one address per line; and the quote
    // under an answer.
    "
    ALTER TABLE message ADD COLUMN answer_to TEXT NOT NULL DEFAULT '';
    ALTER TABLE message ADD COLUMN recipients TEXT NOT NULL DEFAULT '';
    ALTER TABLE outbox ADD COLUMN quote TEXT NOT NULL DEFAULT '';
    ",
    // 17: senders whose remote images load without asking.
    "
    CREATE TABLE image_sender (email TEXT PRIMARY KEY COLLATE NOCASE) WITHOUT ROWID;
    ",
    // 18: the user's own name for an account.
    "
    ALTER TABLE account ADD COLUMN label TEXT NOT NULL DEFAULT '';
    ",
    // 19: messages being written, kept on the device. `recipients` is the field as typed;
    // `saved` is in milliseconds.
    "
    CREATE TABLE draft (
        id         INTEGER PRIMARY KEY,
        account_id INTEGER REFERENCES account(id) ON DELETE CASCADE,
        recipients TEXT NOT NULL,
        subject    TEXT NOT NULL,
        text       TEXT NOT NULL,
        reply_to   INTEGER REFERENCES message(id) ON DELETE SET NULL,
        quote      TEXT NOT NULL,
        saved      INTEGER NOT NULL
    );
    ",
    // 20: how many days back an account's mail is kept, 0 for all of it; and whether a
    // folder's server has no older mail to load.
    "
    ALTER TABLE account ADD COLUMN keep_days INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE folder ADD COLUMN older_done INTEGER NOT NULL DEFAULT 0;
    ",
    // 21: server search. `found` marks a message saved only because a search found it,
    // older than the rest of its folder: lists and syncs leave it out. `search_hit` holds
    // what the last server search of each account found, by the words it looked for.
    "
    ALTER TABLE message ADD COLUMN found INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE search_hit (
        message_id INTEGER PRIMARY KEY REFERENCES message(id) ON DELETE CASCADE,
        words      TEXT NOT NULL
    );
    ",
    // 22: the files sent with a message, listed when its body is saved; saved bodies
    // download again to list theirs.
    "
    DELETE FROM message_body;
    CREATE TABLE attachment (
        message_id INTEGER NOT NULL REFERENCES message(id) ON DELETE CASCADE,
        part       INTEGER NOT NULL,
        name       TEXT NOT NULL,
        mime       TEXT NOT NULL,
        size       INTEGER NOT NULL,
        PRIMARY KEY (message_id, part)
    ) WITHOUT ROWID;
    ",
    // 23: an account whose login the server refused, until the person signs in again.
    "ALTER TABLE account ADD COLUMN needs_sign_in INTEGER NOT NULL DEFAULT 0;",
    // 24: the forwarded message of a draft or a message waiting to be sent, whose files go
    // with it.
    "
    ALTER TABLE draft ADD COLUMN forward INTEGER REFERENCES message(id) ON DELETE SET NULL;
    ALTER TABLE outbox ADD COLUMN forward INTEGER REFERENCES message(id) ON DELETE SET NULL;
    ",
    // 25: a moved message shows in the folder it goes to at once, as a copy. `stand_in` marks
    // the copy until a sync of that folder finds the server's own message; `op.copy` is the
    // copy a move made, dropped with the move.
    "
    ALTER TABLE message ADD COLUMN stand_in INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX message_stand_in ON message(folder_id) WHERE stand_in = 1;
    ALTER TABLE op ADD COLUMN copy INTEGER REFERENCES message(id) ON DELETE SET NULL;
    CREATE INDEX op_copy ON op(copy) WHERE copy IS NOT NULL;
    ",
    // 26: whether the server refused actions that were then undone on the device, until the
    // app has told the person.
    "
    CREATE TABLE notice (reverted INTEGER NOT NULL);
    INSERT INTO notice VALUES (0);
    ",
    // 27: a message waiting to be sent shows in the sent folder at once, as a stand-in
    // (`outbox.copy`) that a sync of that folder later gives the server's id.
    "
    ALTER TABLE outbox ADD COLUMN copy INTEGER REFERENCES message(id) ON DELETE SET NULL;
    CREATE INDEX outbox_copy ON outbox(copy) WHERE copy IS NOT NULL;
    ",
    // 28: the client certificate a server asks for, by the platform's name for it.
    "
    ALTER TABLE account ADD COLUMN server_certificate TEXT;
    ALTER TABLE account ADD COLUMN smtp_certificate TEXT;
    ",
    // 29: what a provider keeps between sessions, such as ActiveSync's sync keys.
    "
    CREATE TABLE provider_state (
        account_id INTEGER NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        key        TEXT NOT NULL,
        value      TEXT NOT NULL,
        PRIMARY KEY (account_id, key)
    ) WITHOUT ROWID;
    ",
    // 30: files the user added to a message, with a draft while it is written and with the
    // outbox once it is sent; their bytes apart, so moving a file never writes them again. And
    // when a thrown-away draft goes for good, in milliseconds, so it can come back till then.
    "
    CREATE TABLE draft_file (
        id     INTEGER PRIMARY KEY,
        draft  INTEGER REFERENCES draft(id) ON DELETE CASCADE,
        outbox INTEGER REFERENCES outbox(id) ON DELETE CASCADE,
        name   TEXT NOT NULL,
        mime   TEXT NOT NULL,
        size   INTEGER NOT NULL
    );
    CREATE TABLE file_data (
        file INTEGER PRIMARY KEY REFERENCES draft_file(id) ON DELETE CASCADE,
        data BLOB NOT NULL
    );
    CREATE INDEX draft_file_draft ON draft_file(draft) WHERE draft IS NOT NULL;
    CREATE INDEX draft_file_outbox ON draft_file(outbox) WHERE outbox IS NOT NULL;
    ALTER TABLE draft ADD COLUMN gone INTEGER;
    ",
    // 31: when snoozed mail comes back, by the message's key (its Message-ID). A row that
    // `woke` keeps the time it came back: the inbox shows the message on top, dated then.
    "
    CREATE TABLE snooze (
        account_id INTEGER NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        message_id TEXT NOT NULL,
        wake_at    INTEGER NOT NULL,
        woke       INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (account_id, message_id)
    ) WITHOUT ROWID;
    CREATE INDEX snooze_due ON snooze(wake_at) WHERE woke = 0;
    ",
    // 32: when the phone looks for an account's new mail with the app closed: 0 as soon as it
    // comes, -1 only with the app open, else every so many minutes.
    "
    ALTER TABLE account ADD COLUMN new_mail INTEGER NOT NULL DEFAULT 0;
    ",
    // 33: signatures: an account's own, empty for the default one kept in `setting`, and
    // the one of each message being written or sent.
    "
    ALTER TABLE account ADD COLUMN signature TEXT NOT NULL DEFAULT '';
    ALTER TABLE draft ADD COLUMN signature TEXT NOT NULL DEFAULT '';
    ALTER TABLE outbox ADD COLUMN signature TEXT NOT NULL DEFAULT '';
    CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
    ",
    // 34: drafts on the server. A draft's copy is in its account's drafts folder under the
    // draft's own `message_id`; `pushed` is the `saved` time the copy has. `draft_gone` holds
    // copies to delete: of drafts sent, thrown away or moved to another account.
    "
    ALTER TABLE draft ADD COLUMN message_id TEXT;
    ALTER TABLE draft ADD COLUMN pushed INTEGER;
    ALTER TABLE draft ADD COLUMN remote_account INTEGER;
    ALTER TABLE draft ADD COLUMN remote_folder TEXT;
    ALTER TABLE draft ADD COLUMN remote_validity INTEGER;
    ALTER TABLE draft ADD COLUMN remote_id TEXT;
    CREATE TABLE draft_gone (
        id         INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        folder     TEXT NOT NULL,
        validity   INTEGER NOT NULL,
        remote_id  TEXT NOT NULL,
        message_id TEXT
    );
    CREATE TRIGGER draft_copy_gone AFTER DELETE ON draft
    WHEN OLD.remote_id IS NOT NULL AND EXISTS (SELECT 1 FROM account WHERE id = OLD.remote_account)
    BEGIN
        INSERT INTO draft_gone (account_id, folder, validity, remote_id, message_id)
        VALUES (OLD.remote_account, OLD.remote_folder, OLD.remote_validity, OLD.remote_id,
                OLD.message_id);
    END;
    CREATE TRIGGER draft_copy_moved AFTER UPDATE OF account_id ON draft
    WHEN OLD.remote_id IS NOT NULL AND NEW.account_id IS NOT OLD.remote_account
    BEGIN
        INSERT INTO draft_gone (account_id, folder, validity, remote_id, message_id)
        SELECT OLD.remote_account, OLD.remote_folder, OLD.remote_validity, OLD.remote_id,
               OLD.message_id
        WHERE EXISTS (SELECT 1 FROM account WHERE id = OLD.remote_account);
        UPDATE draft SET remote_account = NULL, remote_folder = NULL, remote_validity = NULL,
                         remote_id = NULL, pushed = NULL
        WHERE id = NEW.id;
    END;
    ",
    // 35: no connection kept open with the app closed any more: accounts that waited on one
    // check every 15 minutes, as new ones do.
    "
    UPDATE account SET new_mail = 15 WHERE new_mail = 0;
    ",
    // 36: copies (`Cc`) of a message being written or sent, as `recipients`. A message's own
    // `recipients` now holds its `To` and `Cc` with their names (see `people_column`).
    "
    ALTER TABLE draft ADD COLUMN cc TEXT NOT NULL DEFAULT '';
    ALTER TABLE outbox ADD COLUMN cc TEXT NOT NULL DEFAULT '';
    ",
    // 37: the order the person put the accounts in, whether an account's inbox is part of the
    // unified one, and the folders pinned among an account's usual ones.
    "
    ALTER TABLE account ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
    UPDATE account SET position = id;
    ALTER TABLE account ADD COLUMN in_all INTEGER NOT NULL DEFAULT 1;
    ALTER TABLE folder ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
    ",
    // 38: how to leave the list a message came from: its `List-Unsubscribe` header, and
    // whether `List-Unsubscribe-Post` takes a one-click POST.
    "
    ALTER TABLE message ADD COLUMN unsubscribe TEXT;
    ALTER TABLE message ADD COLUMN one_click INTEGER NOT NULL DEFAULT 0;
    ",
];
