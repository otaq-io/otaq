-- The user's own edit of a ticket's delivery message, kept until the ticket lands: Approve & merge's squash message,
-- and Approve & open PR's title and description and the per-commit messages a Push to PR rewords (a JSON object of
-- commit sha to message). NULL (or an empty object) is a field the user didn't change: the dialog shows the proposal
-- there. Kept apart from `delivery_messages`, which is asked for again when the work changes: a Send back keeps the
-- edit. Never edit once committed.
CREATE TABLE edited_messages (
    ticket_id      TEXT   NOT NULL PRIMARY KEY REFERENCES tickets (id) ON DELETE CASCADE,
    merge_message  TEXT,
    pr_title       TEXT,
    pr_description TEXT,
    pr_commits     TEXT,
    updated_at     BIGINT NOT NULL
);
