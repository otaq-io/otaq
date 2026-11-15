-- A stage's task skill (tuq decision 081), typed after Otaq's `work-ticket` once that first turn ends: the
-- `task_skill` kind, so the run of its stage ending removes it at once, whatever ends the stage. Never edit once
-- committed.
CREATE TABLE pending_deliveries_new (
    id           TEXT   NOT NULL PRIMARY KEY,
    session_id   TEXT   NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    kind         TEXT   NOT NULL CHECK (kind IN ('agent_message', 'ticket_change', 'continue_after_limit', 'skill',
                                                 'validation', 'stage_reminder', 'task_skill')),
    text         TEXT   NOT NULL,
    created_at   BIGINT NOT NULL,
    message_id   TEXT,
    message_part TEXT
);
INSERT INTO pending_deliveries_new (id, session_id, kind, text, created_at, message_id, message_part)
SELECT id, session_id, kind, text, created_at, message_id, message_part FROM pending_deliveries;
DROP TABLE pending_deliveries;
ALTER TABLE pending_deliveries_new RENAME TO pending_deliveries;
CREATE INDEX pending_deliveries_session ON pending_deliveries (session_id, created_at);
