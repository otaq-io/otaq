# T2. Rewrite the `noyan` skill: intake only (N1–N4)

**Depends on:** nothing. It quotes T1's refusal text, which is fixed in DECISIONS.md N11. **Runs in parallel with:**
T1, T3, T4, T7.

Read `ideas/noyan-intake/DECISIONS.md` first (N1–N4, N10, N11).

## What to change in `plugin/skills/noyan/SKILL.md`

- **Front matter `description`:** drop "decide whether the work is one ticket (with a Plan of pieces when it is big)
  or several" and "their stage choices". Say: a light look, tickets with notes handed on, several only for separate
  deliverables, the user's choices passed on.
- **Steps 4–5:** replace "Investigate…" and "Decide the shape…" with N3's light look and N2's rule. Keep "never edit,
  never commit".
- **Delete:**
  - "One ticket, or several", replaced by a short "One ticket, or several" that states N2;
  - "Writing a Plan";
  - "Setting stage choices";
  - "Planning stages";
  - the stage-choice examples with `--reason`.
- **Add "What a ticket carries" (N3):** where the code lives, what you saw (`file:line`), open questions left
  unanswered. Ask the user only about the request. Give one example ticket body with the three parts.
- **Proposals:** keep the table, the count line, "Say yes, or change rows", and the above-10-tickets rule. Change the
  Choices column so it shows only choices the user named, and "—" otherwise. Drop the "skips planning / runs
  planning" and "Plan · 3 pieces" examples. Keep "When no stage has options" (leave the column out).
- **Replace "Set by the user, or by you"** with: every choice comes from the user, so pass it with `--by-user`, in
  `otaq create` and in `-f` files. If the CLI refuses with N11's text, the choice wasn't the user's, so leave it out.
- **Rules:** add "Never set a stage choice the user didn't name."
- **Keep unchanged:** "Offer options…", `prime --project`, naming the conversation, `otaq ask`, Backlog-only, stdin
  rules.
- **N13:** the title becomes "Work as Noyan, an Otaq project's coordinator". The opening paragraph starts with
  N13's defining sentence, word for word. The front matter description says "coordinator", not "planning agent".
  Leave "project agent" out of the text too.

## Tests

- **`crates/otaq-core/tests/suite/plugin.rs`:** `project_agent_investigates_and_files_backlog_tickets` (line ~914)
  holds the d0129 needles ("a bug fix, a small feature you already understand)", "when you would write a Plan of
  several pieces…"). Replace them with N3/N4 needles. `project_agent_proposes_with_a_table_and_sets_stage_choices`
  (line ~1082): update it to the user-only Choices column. Assert that the skill no longer contains "ordinary work",
  "--reason" or "Writing a Plan".
- **`crates/otaq-core/tests/suite/plugin_texts.rs`:** check it for noyan text assertions.
- **The heredoc and `ask human` reads-stdin assertions** must still pass.

## Done when

- `scripts/check.sh test` passes.
- The skill reads coherently top to bottom, with no leftover references to Plans written by Noyan or to Noyan's own
  choices.
