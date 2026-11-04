# T5. `work-ticket`: decide the split, re-split mid-way, helpers hand back early (N6, N8, N9)

**Depends on:** T4 (read `ideas/noyan-intake/tasks/T4-findings.md` for the real commands). **Runs in parallel with:**
T1, T2, T3, T7, once T4 is done.

Read `ideas/noyan-intake/DECISIONS.md` first (N6, N8, N9, N10).

## What to change in `plugin/skills/work-ticket/SKILL.md`

- **Step 4, "Plan, then implement":** after the gap check, with no Pieces in `prime`, **decide by N6** (canonical
  text, word for word) whether to build alone or to write a Plan of pieces on this ticket and work as the agent in
  charge. Show the Plan's shape (`## Pieces`, numbered, "— waits for …", details indented, each piece standing on its
  own) and the command from T4's findings.
- **A new section, "When the work grows":** if, partway through, the work turns out bigger than you judged, stop
  coding, commit what you have, write a Plan for the *remaining* work and continue as the agent in charge. Don't ask
  the user about size. Ask only about design choices the ticket didn't settle (the existing rule).
- **"As a helper", a new step, "Piece too big?":**
  1. Commit what you built.
  2. Hand back early with `otaq helper done --stdin`, saying what's done, what's left, and a suggested split (example
     from N9).
  3. Never start helpers yourself.
- **"With a Plan: work as the agent in charge", a new step:** when a helper hands back with a suggested split:
  1. Check and merge the finished part.
  2. Add the new pieces to the Plan (T4's command).
  3. Start their helpers.
  You alone change the Plan.
- **Fit with what main added since these tasks were written** (`Prove it works`, `Run the app`, `When a finding comes
  back`, and the gap check's "What will prove it's done?" criteria list). Don't rewrite those sections. Do this:
  - Place the up-front split decision **after** the gap check has listed its criteria.
  - When you write a Plan, each piece names which of those criteria it proves.
  - The agent in charge still proves every criterion after merging, as today ("Helpers get no Run: the agent in charge
    runs the app after merging the pieces").
  - On a mid-way re-split, stop any Run you started before handing work to helpers, and keep the evidence you already
    gathered in `$OTAQ_RESOURCE_DIR`.
  - "When a finding comes back" is about a wrong premise, not size: don't merge the two.
- **Update the front matter `description`** to mention deciding the split and re-splitting.
- **Rules bullet on later-stage choices (N12):** the agent may pick another agent for a later stage with a reason;
  it never skips a stage. Only the user does. Quote N12's refusal text as what the CLI says if it tries.

## Tests

- In `crates/otaq-core/tests/suite/plugin.rs`, `work_ticket_runs_helpers_as_the_agent_in_charge` (line ~1023): add
  needles for:
  - the up-front decision;
  - "When the work grows";
  - the helper's early hand-back;
  - the agent in charge applying a split;
  - N6's text.
- The existing heredoc and stdin assertions must still pass.

## Done when

`scripts/check.sh test` passes, and every command in the new text matches T4's findings.
