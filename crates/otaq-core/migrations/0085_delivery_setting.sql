-- A flow's delivery is a setting of the flow (`delivery:`), not a stage. Never edit once committed.
--
-- A ticket being delivered (`to_deliver`, `pr_open`, `merged_into`) stays in its flow's last stage, so a ticket
-- whose stage changes while it is delivered opens no stage run: the delivery isn't a stage and has no run. Older
-- `delivery` runs stay as the record of what ran. The trigger is as 0075 recreated it, with that state left out of
-- the run it opens.
--
-- Right after this migration, Store::open rewrites every stored flow version that has a Delivery stage (or a stage's
-- `column:`) with the delivery as the flow's `delivery:`, and moves every ticket being delivered into its project's
-- current flow's last stage (the Approved section and the Delivery stage go: a ticket there is in Review as To
-- deliver, PR open or Merged into). Moving it ends the Delivery stage's open run, as any stage change does.
DROP TRIGGER stage_runs_follow_tickets;
CREATE TRIGGER stage_runs_follow_tickets AFTER UPDATE OF stage_id, state ON tickets
WHEN NEW.stage_id IS NOT OLD.stage_id
  OR (NEW.stage_id IS NOT NULL
      AND OLD.state IN ('queued', 'waiting_for_slot', 'held', 'parent_closed', 'start_interrupted')
      AND NEW.state NOT IN ('queued', 'waiting_for_slot', 'held', 'parent_closed', 'start_interrupted'))
BEGIN
    UPDATE stage_runs
    SET ended_at = CAST(unixepoch('subsec') * 1000 AS INTEGER),
        result = COALESCE(result, CASE WHEN NEW.state = 'merged' OR NEW.stage_id IS NOT NULL THEN 'done'
                                       ELSE 'cancelled' END)
    WHERE ticket_id = NEW.id AND ended_at IS NULL;
    INSERT INTO stage_runs (id, ticket_id, stage_id, kind, session_id, agent_id, agent, model, effort, started_at)
    SELECT lower(hex(randomblob(16))), NEW.id, NEW.stage_id, k.kind, s.id, s.agent_id, s.agent_name,
           COALESCE(s.model_ran, s.model), s.effort, CAST(unixepoch('subsec') * 1000 AS INTEGER)
    FROM (SELECT CASE
              WHEN NEW.state IN ('validating', 'validator_needs_you', 'validation_failed', 'validation_no_result')
                OR NEW.interrupted_from IN ('validating', 'validator_needs_you') THEN 'check'
              WHEN NEW.state = 'ready_for_review' THEN 'you'
              ELSE 'agent'
          END AS kind) k
    LEFT JOIN sessions s ON s.id = (
        SELECT id FROM sessions
        WHERE ticket_id = NEW.id AND ended_at IS NULL
          AND kind = CASE k.kind WHEN 'agent' THEN 'agent' WHEN 'check' THEN 'validator' ELSE 'delivery' END
        ORDER BY started_at DESC LIMIT 1)
    WHERE NEW.stage_id IS NOT NULL
      AND NEW.state NOT IN ('queued', 'waiting_for_slot', 'held', 'parent_closed', 'start_interrupted',
                            'to_deliver', 'pr_open', 'merged_into');
END;
