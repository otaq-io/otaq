-- M40 Manual checks for you (T-0498). Decisions 0125, 0109, 0079. Never edit once committed.
--
-- What an agent or a check couldn't check itself, one line each, from `--manual-check` on `stage next`, `stage
-- done` and `check pass|fail`; the review lists them above Approve and the user ticks them. `n` numbers a ticket's
-- items 1, 2, 3…, never reused. The author is the comment's (`ticket_comments`, 0055): `author_kind`, `author_id`,
-- `author_name`, `author_tool`, with `stage_id` and `round` the stage and check round it came from, and
-- `comment_id` the hand-over comment it came with. A later stage end from the same stage replaces that stage's
-- unticked items with its own (`replaced_at` set: the row stays, no list shows it, its `n` isn't reused); ticked ones
-- stay. `ticked_at` / `ticked_by` (the OS user) while ticked.
CREATE TABLE manual_checks (
    id          TEXT   NOT NULL PRIMARY KEY,
    ticket_id   TEXT   NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    n           BIGINT NOT NULL,
    body        TEXT   NOT NULL,
    author_kind TEXT   NOT NULL CHECK (author_kind IN ('user', 'agent', 'lead')),
    author_id   TEXT,
    author_name TEXT,
    author_tool TEXT,
    stage_id    TEXT,
    round       BIGINT,
    comment_id  TEXT   REFERENCES ticket_comments (id) ON DELETE SET NULL,
    added_at    BIGINT NOT NULL,
    ticked_at   BIGINT,
    ticked_by   TEXT,
    replaced_at BIGINT,
    UNIQUE (ticket_id, n)
);
