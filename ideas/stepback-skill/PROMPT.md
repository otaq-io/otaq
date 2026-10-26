# Idea: a `stepback` skill

Logged 2026-10-25, not built. Paste everything below the line into a session that writes skills (for example with the
`skill-creator` skill) when you want it made.

---

Write a Claude Code skill named **`stepback`** for this repository (Otaq). It makes the agent stop the current line of
work, look at the whole picture, and check whether the decisions taken so far still serve what the user is trying to
achieve. Put it in `.claude/skills/stepback/SKILL.md`.

## When it triggers
- The user types `/stepback`, or says things like: "step back", "let's step back and look at the big picture", "what
  are we trying to achieve", "are our decisions correct", "this is causing lots of gaps", "I don't understand why we
  need X", "it should just work without anything special".
- Also suggest it (don't run it unasked) when a design discussion keeps producing new gaps, special cases or
  exceptions for the same feature, for example a gap review that returns many rows tied to one concept.

## What the agent does (the procedure that worked on 2026-10-25)
1. **Stop adding rules.** Don't answer the next gap or patch the next special case. Say plainly that you are stepping
   back.
2. **Restate the goal in the user's words**, as a concrete example, not in internal jargon. For example: "One card
   goes through your flow; at each stage one agent is in charge; in planning it works alone, in building it splits the
   work among helpers; everything else works as for any ticket."
3. **Find the one root cause behind the gaps.** List the recent gaps, exceptions and workarounds (decision files,
   gap reviews, audits) and name the single assumption they share. For example: "We kept the old idea that an epic's
   children are tickets that walk the flow themselves, so two layers of flow sit on top of each other; every gap came
   from fitting them together."
4. **Propose the simpler model** that removes that assumption, and show which gaps disappear with it.
5. **State the costs honestly:** what is lost (for example per-child review), what built work is redone, and which
   accepted decisions it reverses, by number and with a short quote (`decisions/README.md` rules: conflicts are
   raised, not silently resolved).
6. **Pause downstream work** that depends on the old model (designer updates, tickets, builds) and say so.
7. **Ask one decision**, with the user's choice between the simpler model, the current one, or talking it through.
   Use the question tool, one question at a time (see the repo's working style). Recommend one option and say why.
8. **Once the user chooses**, log it as a new decision (Proposed until its open questions are settled), list the
   open questions in it, and settle them one at a time. Mark the replaced decisions only when it is accepted.

## Rules for the skill
- Plain language with concrete examples (the user's preference); no internal jargon.
- Read before judging: `decisions/README.md`, the decisions in play, `DESIGN.md`, and any recent gap review or audit
  in `design-briefs/`. Check the code when a claim depends on it.
- Don't edit files during steps 1–7; the step back ends with the user's choice.
- Keep it short: the goal, the root cause, the simpler model, the costs, one question.

## Example to include in the skill
The 2026-10-25 session on planning on the board: decisions 0129–0132 (Spec first, several check agents, fails go to,
epics walking the planning stages) produced a 20-row gap review and then designer questions about an "Epics wait
here" mark and children's skips. The user said: "we should step back and understand what we are trying to achieve;
if it's causing lots of issues with the product or a lot of gaps, we need to step back and take a look at the big
picture and then understand if our decisions are correct." The step back found the root cause (children walking the
flow as tickets), proposed one card walking the flow with helpers inside a stage, and the user chose it: decision
0133.
