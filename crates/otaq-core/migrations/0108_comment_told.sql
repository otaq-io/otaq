-- The user's comment may also go to the agent working its ticket, only when the user picks "Tell <agent> now": `told`
-- is that agent's name as it read then ("Strong"), so the thread tags the comment "· told Strong now". NULL on every
-- other comment, and on one whose tell found no agent running. Never edit once committed.
ALTER TABLE ticket_comments ADD COLUMN told TEXT;
