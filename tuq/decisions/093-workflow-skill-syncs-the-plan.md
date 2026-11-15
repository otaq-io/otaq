# 093 · Otaq's workflow skill keeps the ticket's Plan in step with the plan file

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 060, 077, 081

## Summary

tuq's Milestone plan only writes `tuq/plans/<feature>.md`; copying its Plan into the Otaq ticket, and reading the
ticket's Plan back into the file, is the job of Otaq's workflow skill. The `tuq` commands never call `otaq`.

## Decision

- tuq's `milestone-plan` skill and the `tuq-plan` command work on the plan file alone, the same in a plain terminal
  and in an Otaq stage.
- `tuq-plan` drops what talks to Otaq today: `pull` (which reads the ticket's Plan with `otaq show --json`) goes, and
  nothing in tuq names `otaq update --plan-file`.
- In an Otaq stage, Otaq's workflow skill puts the plan file's Plan on the ticket when the stage ends, and, when the
  ticket comes back, brings the ticket's current Plan into the file before the task skill starts.
- The plan file's shape (its markers and `## Pieces`) stays tuq's; the workflow skill reads it as tuq defines it.

## Why

`tuq-plan` still read and wrote the ticket through `otaq`, so a change to Otaq's output could break Milestone plan for
anyone on an older tuq, while decisions 060 and 081 promise that tuq never depends on Otaq and that any tuq version
runs. Otaq's workflow skill always comes from Otaq's own release, matching the app (decision 080), so the sync can
never be out of step with the app. Making the plan file itself the ticket's Plan (no copy) was turned down as a larger
app change; keeping the sync in tuq with a "needs Otaq" range was turned down because it breaks "any tuq version
runs".
