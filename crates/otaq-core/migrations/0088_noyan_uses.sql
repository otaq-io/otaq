-- Noyan's agent is one app setting, and agents have no role. Never edit once committed.
--
-- The agent that held the Noyan role (`agents.role = 'noyan'`) becomes the agent Noyan uses: the setting
-- `agents.noyan` holds its id as a JSON string. Each stage names its agent; nothing else ever read a role, so the
-- column and its index go. A database with no Noyan agent gets no setting (the user picks one in Settings → Agents).
INSERT INTO settings (key, value_json, updated_at)
SELECT 'agents.noyan', json_quote(id), CAST(strftime('%s', 'now') AS INTEGER) * 1000
FROM agents
WHERE role = 'noyan'
LIMIT 1
ON CONFLICT (key) DO NOTHING;

DROP INDEX IF EXISTS agents_role;
ALTER TABLE agents DROP COLUMN role;
