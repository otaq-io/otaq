# A check stage has one agent

Agreed with the maintainer on 2026-11-06, while going through the questions left by `ideas/no-skill-copies`. The old
`decisions/` folder is lost, so this file is the record (it reverses decision 0130's "one Check stage, several check
agents", built in T-0525, T-0527, T-0529–T-0531). Not built yet. Refined the same day in the gap review
(`ideas/gap-review/REPORT.md`, G-numbers; marked "gap review" below).

## Principles

**The flow is as simple as possible.** A stage kind or a stage feature exists only when it is truly necessary. Agents
working on one ticket in parallel is not something to add for one kind of stage: if Otaq ever runs agents in parallel,
it does so for every kind (developers, investigators, checkers) as one feature, designed on its own.

**A flow file says everything a flow does** (gap review). People share an agent or a flow as a YAML file with no
other instructions, so what a flow does is what its file says. Otaq's code has no hidden rule that changes a flow's
behaviour in some situation (a stage skipped because others were, a Delivery changed because a check was skipped).
A format default that is the same everywhere is fine.

## Decision

- An agent check (Validate and the like) has **one** check agent, like an Agent stage has one agent.
- Two checks are two stages, one after the other, each with its own agent, skill, rounds and "fails go to".
- Removed: the flow editor's **Check agents** list and **Add a check agent**, the round shared by several agents, the
  card's row per check agent and the combined verdict ("Validate failed round 1: Security reviewer failed, Validator
  passed").
- **The file format** (gap review, G20): a check stage names its agent with `agent:`, like an Agent stage. The
  presets write `agent: Validator`. `agents: [X]` is still read as `agent: X`.
- **Noyan and the flow skill** (gap review, G19): `plugin/skills/flow` writes one stage per check and never lists
  several agents; a proposal or flow file that lists several is split as below.
- **A validator's round** (gap review, G26) is always its own stage's round: two check stages count separately.
- **The CLI** (gap review, G27): `otaq attach --agent` and `otaq show --terminal --stage --agent` are kept for one
  release and ignored, with a note that a check has one agent.

## Moving over

A flow with several check agents on one check stage (a stored version, a flow file, a proposal) is **split**: the
stage becomes one check stage per agent, in the listed order, one after the other. The first keeps the stage's id and
name; each other one gets a new id and the name `<stage> · <agent>` ("Validate · Security reviewer"), with " 2" added
if that name is taken (gap review, G21). Each copy keeps the stage's skill, rounds, "fails go to", options and why;
every copy after the first shares the first copy's column (`with-previous`), so the board looks as it did (gap
review, G21). Every check the user set up still runs, now in turn.

- **When** (gap review, G8): the split is done every time a flow is read, including older stored versions and Back to
  the previous flow, and is saved only with the user's next change to the flow. It is never stored as a version of
  its own.
- **A ticket's choices** (gap review, G7): a ticket's skip or "moved past" on the old stage applies to every copy; a
  ticket's chosen agent stays on the copy for that agent; every copy's rounds start at 0.
- **A ticket in the middle of such a stage** finishes its round as it is, or is restarted on the first copy (the
  implementation picks the simpler, and says which).
- **Old rounds** (gap review, G23): rounds stored before the split still show as they were, one round with every
  agent's report; only new rounds are one agent per stage.
- **Telling users** (gap review, G22): the release notes say that checks which ran side by side now run in turn.
  Nothing is shown in the app.

## Other stage features (gap review)

Each existing stage feature was checked against both principles (G24).

- **O1. A ticket can pick any agent for any stage.** An agent is now only a tool, a model (with effort) and
  instructions. A stage no longer lists the agents it allows: the agents in `options` are dropped (still read in old
  files, and ignored). For any stage that hasn't started, a ticket can pick any agent whose tool has the stage's
  skill (`ideas/no-skill-copies`, S5). Only the user picks, or Noyan passing on the user's words. Rejected: keeping
  the flow's list of allowed agents (the default flow allowed no change at all); no per-ticket agent.
- **O2. Only stages the flow marks as skippable can be skipped, and only by the user.** As today: `options: [skip]`
  marks a stage skippable; a stage that isn't marked, such as Dev, always runs, because a ticket that skips its work
  has no point. After O1, `skip` is the only thing `options` holds. Rejected: letting the user skip any stage.
- **O3. Nothing is skipped on its own.** The rule that skipped a skippable human check when every Agent stage before
  it was skipped ("Spec review: skipped · nothing before it runs") is removed: the user skips Spec review too if they
  want. The per-ticket choice stays, called "required" (`--stage "Spec review=required"`): it means the check
  happens, as the flow says (word kept 2026-11-06, before the PR).
- **O4. A skipped check doesn't change the Delivery.** The rule that made an automatic Delivery a click for a ticket
  that skipped a human check is removed. A flow with a skippable human check before an automatic Delivery is allowed;
  the flow editor says next to the skip mark that a ticket skipping it is delivered without that check. The same goes
  for a ticket moved past an agent check (Move on anyway, Skip this check): its Delivery does what the flow says, with
  no forced click (agreed 2026-11-06, while planning).
- **O5. Skip this check and Move on anyway only on skippable stages.** Skip this check (on a check waiting for its
  tool's limit) and Move on anyway (after a check's last failed round) show only on a check the flow marks skippable
  (O2). Elsewhere the user waits, picks another agent for the stage (O1), or sends the ticket back.
- **O6. Kept as they are:** `delivers: either` is click only (the flow check says so, and an automatic Delivery can't
  choose); `column: with-previous` (written in the file, it changes only the board's layout); a check without
  `fail_to` fails to the nearest Agent stage before it (a format default).
- **O7. "Delivered without all checks".** A delivered ticket carries this mark when a check before its Delivery didn't
  pass in its latest run (skipped, moved past or never run), or when the flow has no check. One passed check no longer
  hides a skipped one. A check that failed and then passed on a later round doesn't count. The mark was called
  "Delivered without checks" (agreed 2026-11-06, after the hands-on).
