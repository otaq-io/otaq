-- A new user's first run: the prefilled first ticket in Backlog, the one-time hints at its first question, first
-- permission prompt, first checker fail, first review and after the first approve, and the box for new work, which
-- shows only once a ticket was approved. A data folder that already has projects belongs to someone past all of that:
-- its first-run hints count as seen and its first approve as reached, so an upgrade neither shows first-run hints nor
-- hides the box. A data folder without projects (a fresh one, or one whose projects were all removed) gets the first
-- run.
INSERT INTO settings (key, value_json, updated_at)
SELECT hint.key, 'true', CAST(strftime('%s', 'now') AS INTEGER) * 1000
FROM (
    SELECT 'hints.seen.first_ticket' AS key
    UNION ALL SELECT 'hints.seen.first_question'
    UNION ALL SELECT 'hints.seen.permission_prompt'
    UNION ALL SELECT 'hints.seen.checker_sent_back'
    UNION ALL SELECT 'hints.seen.first_review'
    UNION ALL SELECT 'hints.seen.that_was_your_flow'
    UNION ALL SELECT 'hints.seen.new_work'
    UNION ALL SELECT 'first_run.first_approve'
) AS hint
WHERE EXISTS (SELECT 1 FROM projects)
ON CONFLICT (key) DO NOTHING;

-- No project waits in Flow setup any more: every project starts on Otaq default and its flow changes with Edit flow.
-- A project still in Flow setup (first run left on its Flow step, or a project added later whose Flow setup page was
-- never settled) is on its flow in use from now on.
UPDATE projects SET flow_setup_since = NULL WHERE flow_setup_since IS NOT NULL;
