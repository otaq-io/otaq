# T12. The website demo's Noyan table has no Agent column (N18)

**Depends on:** T6 (merged). **Runs in parallel with:** T9, T10, T11.

Read `ideas/noyan-intake/DECISIONS.md` first (N2, N4, N13, N18).

## What to change

- **`website/src/demo/Noyan.tsx`** (~lines 34-44): the proposal table becomes **# · Title · Description**, with no
  Agent column, matching the `noyan` skill's table "When no stage has options" (`plugin/skills/noyan/SKILL.md`).
- **`website/src/demo/data.ts`:** remove the per-row agents of `NOYAN0` (~795-806) and `agent: "Docs updater"` in
  `noyanTickets` (~774), and any type field that only existed for them. If the demo's tickets later show an agent on
  the board because of that field, they show the stage's own agent instead.
- Keep "your planning agent" (the Ask bar, Hero, Flow): that's N13's exception.
- Check the rest of the demo for Noyan writing a Plan or picking agents, and align it with N1–N4.
- Fix any website test that asserted the old column.

## Done when

`npm --prefix website run check` passes. Only Node 26 is installed on this Mac while the website wants Node 24: run
it anyway and report the real output (an `EBADENGINE` warning from `npm ci` is expected).
