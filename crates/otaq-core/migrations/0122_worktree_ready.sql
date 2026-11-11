-- A start made the ticket's worktree ready (seeded, env files, resources and setup done) but hasn't launched its
-- agent yet: Retry start, or a start the pause kept, launches in that worktree without setting it up again. Cleared
-- once the agent launches, when the worktree is made afresh, and when a failed start removes it. Never edit once
-- committed.
ALTER TABLE tickets ADD COLUMN worktree_ready BOOLEAN NOT NULL DEFAULT FALSE;
