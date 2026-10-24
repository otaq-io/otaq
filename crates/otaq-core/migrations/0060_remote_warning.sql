-- M40 An unreachable remote is a warning (T-0493). Decisions 0121, 0059, 0079. Never edit once committed.
--
-- `remote_warning`: JSON `{went_on, branch, remote, reason}`. The fetch before the ticket's start (`started`) or its
-- Local merge (`merged`) failed, and the ticket went on from the local base branch; `reason` is git's. Cleared by the
-- next fetch for the ticket that works.
ALTER TABLE tickets ADD COLUMN remote_warning TEXT;
