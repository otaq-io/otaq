-- The message a ticket's delivery uses, written by the flow's delivery agent while the ticket waits in Review: the
-- squash commit's (its title line, then its description) and, for a PR, its title, description and branch. `writing`
-- until the agent sends it, then `ready`; `head` is the branch commit it was asked for, so an arrival with new commits
-- asks again. Never edit once committed.
CREATE TABLE delivery_messages (
    ticket_id   TEXT   NOT NULL PRIMARY KEY REFERENCES tickets (id) ON DELETE CASCADE,
    state       TEXT   NOT NULL CHECK (state IN ('writing', 'ready')),
    agent       TEXT   NOT NULL,
    head        TEXT,
    title       TEXT   NOT NULL DEFAULT '',
    description TEXT   NOT NULL DEFAULT '',
    branch      TEXT,
    updated_at  BIGINT NOT NULL
);
