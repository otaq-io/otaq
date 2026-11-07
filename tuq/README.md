# tuq

Product decisions (and, later, tech specs, milestone plans and orchestrator prompts) produced by the tuq skills
(`tuq:product-spec`, `tuq:step-back`, `tuq:tech-spec`, `tuq:milestone-plan`).

## Layout

- `decisions/` — one file per product decision: `NNN-short-summary.md`, numbered across the repository.
- `tech-specs/` — one tech spec per feature: `<feature>.md`, how it is built (the architecture), with the decisions
  it builds on in its header.
- `plans/` — one milestone plan per feature: `<feature>.md`, the pieces and what each waits for.
- `prompts/` — the orchestrator prompt for each plan: `<feature>-orchestrator.md`, which the agent that builds the
  work follows.

## Rules

- Each new decision takes the highest number here plus one; a clash between parallel branches is fixed at merge by
  renumbering the later branch's files and their references. Numbers are never reused.
- Before a decision is saved it is checked against the existing ones; when the user proceeds despite a conflict, the
  earlier one is marked `Superseded by NNN` (or `Partly superseded by NNN`), not rewritten.
- The index below is generated from the decision files (`tuq index`): don't edit it by hand.

## Decision index

<!-- tuq:index:start -->

| #   | Decision | Status | Summary |
| --- | -------- | ------ | ------- |
| | _No decisions yet._ | | |

<!-- tuq:index:end -->
