# 09 · Cancel in Edit flow leaves the skill the agent wrote; the stage keeps warning after it's written

**Kind:** bug · **From:** re-test NEW-7 and the piece 10 note

- "Ask the agent to write it…" wrote `.agents\skills\update-docs\SKILL.md` into shop's folder. After Cancel in Edit
  flow, the draft went away but the file stayed there, untracked (it was deleted by hand).
- While the draft was open, after the skill was written ("Written by the agent · Open · Undo"), the stage card still
  said "You don't have the skill update-docs". This may be on purpose, because the file isn't committed yet. If so, the
  warning should say that.

Evidence: `../shots-retest/p10-written-still-warns.jpg`.
