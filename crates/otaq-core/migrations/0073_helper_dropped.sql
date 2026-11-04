-- no-transaction
-- The agent in charge drops a piece (T10). Never edit once committed.
--
-- `otaq helper drop <n> --stdin` stops the piece's helper, removes its worktree and branch and marks it `dropped`, its
-- reason as its note. A dropped piece is never merged and can't start again. `helpers.status` gains `dropped`; SQLite
-- can't change a CHECK in place, so the table is rebuilt the documented way, as 0067 rebuilt `sessions`: foreign keys
-- off (outside a transaction, so dropping the old table cascades nothing to `human_questions.helper_id`), copy, swap,
-- index back, foreign keys on. Every row and id stays.
PRAGMA foreign_keys = OFF;
PRAGMA legacy_alter_table = ON;
BEGIN;

CREATE TABLE helpers_new (
    id            TEXT    NOT NULL PRIMARY KEY,
    ticket_id     TEXT    NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    piece         BIGINT  NOT NULL CHECK (piece >= 1),
    title         TEXT    NOT NULL,
    branch        TEXT    NOT NULL,
    worktree      TEXT    NOT NULL,
    session_id    TEXT,
    status        TEXT    NOT NULL
                  CHECK (status IN ('working', 'held', 'sent_back', 'being_reviewed', 'merged', 'dropped')),
    note          TEXT,                                        -- the latest note from the agent in charge; dropped: why
    held_reason   TEXT,
    port          BIGINT,
    started_at    BIGINT  NOT NULL,
    updated_at    BIGINT  NOT NULL,
    merged_at     BIGINT,
    files_json    TEXT,
    prompt        TEXT,
    prompt_at     BIGINT,
    resume_prompt TEXT,
    UNIQUE (ticket_id, piece)
);
INSERT INTO helpers_new (id, ticket_id, piece, title, branch, worktree, session_id, status, note, held_reason, port,
                         started_at, updated_at, merged_at, files_json, prompt, prompt_at, resume_prompt)
SELECT id, ticket_id, piece, title, branch, worktree, session_id, status, note, held_reason, port, started_at,
       updated_at, merged_at, files_json, prompt, prompt_at, resume_prompt
FROM helpers;
DROP TABLE helpers;
ALTER TABLE helpers_new RENAME TO helpers;
CREATE INDEX helpers_session ON helpers (session_id);

COMMIT;
PRAGMA legacy_alter_table = OFF;
PRAGMA foreign_keys = ON;
