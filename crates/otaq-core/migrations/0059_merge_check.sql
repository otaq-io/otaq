-- M40 Merge check (T-0491). Decisions 0120, 0083, 0079. Never edit once committed.
--
-- A failed Merge check counts toward the same "3 in a row" stop as a conflict; when the last one was a failed check,
-- the stop is "Merge check keeps failing on `<target>`" (`tickets.auto_merge_stop = 'keeps_failing_merge_check'`, a
-- column with no CHECK) and an epic child's first such stop goes to its lead like a conflict's. `lead_stuck.stop`
-- has a CHECK, so the table is rebuilt the documented way to allow the new stop. Nothing references `lead_stuck`, so
-- dropping it cascades nothing.
--
-- `tickets.merge_check_due`: a Local merge rebased the ticket's branch onto new commits and no Merge check passed on
-- them since (it failed, was stopped, or a later step refused). The next Local merge runs the check even though the
-- branch already has the target's commits. Cleared when the ticket merges.
--
-- A failed check is recorded like a conflict (`tickets.conflicts_with`, `conflict_reason = 'merge_check'`, no files);
-- that column has no CHECK either. Whether a check runs now is kept in memory only (it never outlives the service).
ALTER TABLE tickets ADD COLUMN merge_check_due BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE lead_stuck_new (
    child_id    TEXT    NOT NULL PRIMARY KEY REFERENCES tickets (id) ON DELETE CASCADE,
    epic_id     TEXT    NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    lead_id     TEXT    NOT NULL,
    stop        TEXT    NOT NULL CHECK (stop IN ('validation_failed', 'keeps_conflicting',
                                                 'keeps_failing_merge_check', 'idle')),
    detail      TEXT,
    rounds      INTEGER,
    state       TEXT    NOT NULL CHECK (state IN ('held', 'noted', 'handed_back')),
    held_at     BIGINT  NOT NULL,
    resolved_at BIGINT,
    note_id     TEXT    REFERENCES lead_notes (id) ON DELETE SET NULL,
    CHECK ((state = 'held') = (resolved_at IS NULL))
);
INSERT INTO lead_stuck_new (child_id, epic_id, lead_id, stop, detail, rounds, state, held_at, resolved_at, note_id)
SELECT child_id, epic_id, lead_id, stop, detail, rounds, state, held_at, resolved_at, note_id
FROM lead_stuck;
DROP TABLE lead_stuck;
ALTER TABLE lead_stuck_new RENAME TO lead_stuck;
CREATE INDEX lead_stuck_by_epic ON lead_stuck (epic_id);
