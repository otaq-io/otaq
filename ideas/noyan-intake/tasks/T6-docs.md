# T6. README and website match the new roles

**Depends on:** T2 and T5 (describe what the skills now say). Read T1's refusal text in DECISIONS.md N11.

Read `ideas/noyan-intake/DECISIONS.md` first.

## What to change

- **`README.md` ~line 122, "Agents suggest agents.":** "Noyan's plan and an agent's proposed tickets name an agent for
  each ticket…" is no longer true for Noyan (N4). Reword it: Noyan passes on the choices you name, and every ticket
  otherwise runs every stage's own agent. A ticket's agent may pick another agent for a later stage, but never skip one (N12).
- **`README.md` ~line 136, "Bigger features: a Plan and helpers":** the Plan is "written by Noyan, by a planning stage
  of your flow, or by you". Change it to "by a planning stage of your flow, by the Dev agent when it splits the work
  (N8), or by you". Mention re-splitting and helpers handing back early (N9) in a sentence.
- **`README.md` ~line 391:** "`noyan` (one ticket with a Plan, or several tickets)". Change it to "`noyan` (files
  tickets from a request, with notes for the next stage)".
- **`website/src/demo/Noyan.tsx`:** check whether the demo shows Noyan writing Plans or setting choices. If it does,
  align it with N1–N4. If unsure, list the lines in your report instead of changing them.
- **Keep "your planning agent" on the website** (Hero.astro, Flow.astro, flow.ts, the demo's Ask bar). That's
  N13's exception.
- **Search the rest of the README and website** for "Noyan" plus Plan, pieces, skip or choice, and fix anything that
  contradicts N1–N9.

## Done when

`scripts/check.sh frontend` passes (if the website changed), and no doc claims that Noyan plans, splits or picks
agents on its own.
