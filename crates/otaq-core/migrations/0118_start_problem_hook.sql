-- The user's hook that refused a ticket's start (post-checkout on making its worktree, or a hook refusing a reopened
-- ticket's update from its base): JSON `{name, step, what, output}`, the hook's whole output with the home folder
-- shortened. Written with the Start interrupted problem it belongs to, NULL for any other problem; kept even though a
-- new ticket's worktree is removed after a failed start. Never edit once committed.
ALTER TABLE tickets ADD COLUMN start_problem_hook TEXT;
