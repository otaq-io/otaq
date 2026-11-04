# T3. `milestone-plan` uses the size measure (N6)

**Depends on:** nothing. **Runs in parallel with:** T1, T2, T4, T7.

Read `ideas/noyan-intake/DECISIONS.md` first (N6).

## What to change in `plugin/skills/milestone-plan/SKILL.md`

- **Step 4, "Decide the pieces":** replace "A piece is a part one agent can build, test and hand back on its own, in
  about a day" with N6's canonical text, word for word. Keep "One piece when the work is one" and "Order them by what
  each needs".
- **Step 2's example** "pieces of about a day" (an example of the user's own instructions): change it to an example
  that doesn't teach time as the measure, for example "pieces that touch one crate each".
- **Each piece's details:** say they must stand on their own, because a helper starts with a fresh context. Name the
  files, screens and decisions it builds on, so the helper doesn't have to explore again. The current example already
  does this; make it a rule.
- **The "Asking" example** says "each about three days of work": reword it without days.

## Tests

- In `crates/otaq-core/tests/suite/plugin_texts.rs`, `milestone_plan_writes_the_plan_files_nothing_and_keeps_the_users_edits`
  (line ~307): add needles for N6's text, and assert that "about a day" is gone.

## Done when

`scripts/check.sh test` passes.
