-- A stage whose skill Otaq can't find (no-skill-copies F1). Never edit once committed.
--
-- When a session starts a stage with a skill Otaq's finder doesn't find (a user's skill the stage names; Otaq's own
-- are always found), the stage still starts and the session keeps that skill's name: the card shows "You don't have
-- the skill `<name>`" while the session's stage runs, and Otaq doesn't remind the session that its stage is still
-- open. Null for every other session, and for every session before this change.
ALTER TABLE sessions ADD COLUMN missing_skill TEXT;
