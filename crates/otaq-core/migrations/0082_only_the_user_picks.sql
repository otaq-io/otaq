-- Only the user picks a stage's agent: a ticket's own agent no longer picks one for a later stage of its ticket, so
-- the agent picks ticket agents stored that the service now refuses are cleared once. Never edit once committed.
--
-- Cleared, on a stage the ticket has no run of yet (a run of any result locks its stage's choice): every agent pick a
-- ticket's own agent set (`set_by = 'agent'`, `choice = 'agent'`). Those stages go back to their own agent. Kept: the
-- user's choices (Noyan's `--by-user` ones are stored as `user`), a ticket agent's `usual` (required) choices, and any
-- choice on a stage that has started.
--
-- Each cleared choice leaves a `stage_choice_cleared` event on its ticket, by Otaq, as the earlier clearing did:
-- payload `stage` (its name, from the latest `stage_choice_set` event for that stage; its id when there is none),
-- `stage_id`, `choice` (the picked agent's name), `set_by` (`agent`) and `by` (the ticket agent's name). History reads
-- "Dev: Spec writer's choice of Strong cleared" · "only the user picks a stage's agent now".
CREATE TEMPORARY TABLE cleared_picks AS
SELECT c.ticket_id, c.stage_id, c.set_by,
       COALESCE(c.by_name, 'the ticket''s agent') AS by_who,
       COALESCE(a.name, 'another agent') AS label,
       COALESCE((SELECT json_extract(e.payload_json, '$.stage')
                 FROM events e
                 WHERE e.ticket_id = c.ticket_id AND e.kind = 'stage_choice_set'
                   AND json_valid(e.payload_json) AND json_extract(e.payload_json, '$.stage_id') = c.stage_id
                 ORDER BY e.at DESC, e.rowid DESC LIMIT 1),
                c.stage_id) AS stage
FROM ticket_stage_choices c
LEFT JOIN agents a ON a.id = c.agent_id
WHERE c.set_by = 'agent' AND c.choice = 'agent'
  AND NOT EXISTS (SELECT 1 FROM stage_runs r WHERE r.ticket_id = c.ticket_id AND r.stage_id = c.stage_id);

INSERT INTO events (id, ticket_id, kind, actor, payload_json, at)
SELECT lower(hex(randomblob(16))), ticket_id, 'stage_choice_cleared', 'otaq',
       json_object('stage', stage, 'stage_id', stage_id, 'choice', label, 'set_by', set_by, 'by', by_who),
       CAST(strftime('%s', 'now') AS INTEGER) * 1000
FROM cleared_picks;

DELETE FROM ticket_stage_choices
WHERE EXISTS (SELECT 1 FROM cleared_picks x WHERE x.ticket_id = ticket_stage_choices.ticket_id
                                              AND x.stage_id = ticket_stage_choices.stage_id);

DROP TABLE cleared_picks;
