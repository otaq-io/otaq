# Noyan is intake only; the flow splits the work

Agreed with the maintainer on 2026-11-04, in a grilling session. The old `decisions/` folder is lost, so this file
is the record. Decision numbers cited in code and tests (d0129, d0133, d0108, d0087) are history only.

**Code comments and tests name the rule in words, never by its number here** ("Noyan is intake only", "only the
user skips a stage"). This file isn't permanent, and main already moved off numbered placeholders (commit 31c6b06a,
"Comments name the rule in words instead of the d{RUN}/d{GUIDE} placeholders"). A task reference such as "(T1)" is
fine.

## The problem

- When an agent gets a ticket that is too big, its context grows until it gets confused.
- Noyan sometimes skipped the planning stages (Product spec, Tech spec, Milestone plan) on medium and large work. The
  `noyan` skill made skipping the default ("Ordinary work skips planning"), and nearly everything looked ordinary.
- Otaq runs on projects whose domain nobody knows in advance, so "small" can't be judged up front. A change can look
  easy and turn complicated once the code is open.
- Asking Noyan to split work enough meant deep investigation. On Spec first that duplicated the spec stages' work. On
  quick flows it interrupted users who chose speed.

## Decisions

### N1. Noyan is intake only

Noyan turns a request into tickets. It doesn't plan the work, split it for size, or write a Plan of pieces.
*Replaces:* the `noyan` skill's "One ticket, or several", where Noyan decided the shape, including "One ticket with a
Plan", and its "Writing a Plan".

### N2. Several tickets only for clearly separate deliverables

Noyan files several tickets only when the request holds separate deliverables, each reviewed and merged on its own
(two unrelated bugs, a tracker import). One deliverable stays one ticket, however big. A proposal is still shown first
for two or more tickets.

### N3. A light look at the code, and notes handed on

Noyan reads enough to write a useful ticket. The description carries three things:

- where the code lives;
- what it saw, with `file:line` where it can;
- the **open questions it noticed, left unanswered**.

Noyan asks the user only about the *request* ("which page feels slow?"), never about design or architecture, which
belong to the spec stages or the Dev agent. The next stage starts from these notes instead of from zero.

### N4. Noyan sets no stage choices of its own

Noyan sets no skips and no agent picks. Every ticket runs every stage of its flow with each stage's own agent. Noyan
only passes on choices the user named, with `--by-user` ("1 skips planning", "2: Dev: Quick developer"). The proposal
table's Choices column shows only the user's choices.
*Replaces:* d0129's "Ordinary work skips planning", and the skill's "Setting stage choices" ("a cheaper agent for a
minor fix…").

### N5. Planning stages are found the way the board finds them

A flow has no new property. A **planning check** is a human check with another human check after it
(`crates/otaq-view/src/view_models/planning.rs`, `checks_planning`). Planning stages are the agent stages before it.
This works for flows users import.
*Note:* since N4, Noyan no longer needs this to decide skips. It still applies wherever a skill talks about "planning
stages".

### N6. The size measure is concrete signs, not time

Every agent that decides pieces (Milestone plan, the Dev agent, a helper suggesting a split) uses the same measure. Use
the canonical text below word for word.

> **A piece fits one agent when** it changes one behaviour, in one area or layer (for example the API, or the page,
> not both), and the agent needs to read only a handful of files to do it. **Split it** when it crosses layers,
> bundles several behaviours, or needs a lot of unfamiliar code read first. **When unsure, split.**

*Replaces:* "in about a day" (`milestone-plan`) and "one agent, about a day" (`noyan`).

### N7. Otaq doesn't measure agents' context use

Context use can't be read the same way for every provider. Claude Code's status payload may report it, but Codex and
others don't in the same way, so Otaq builds nothing provider-specific. The measure stays N6's signs.

### N8. The Dev agent decides the split up front, and re-splits mid-way

On a ticket that arrives without pieces (any quick-flow ticket, a single-piece Plan, or planning the user skipped), the
Dev agent works in two steps:

1. **Up front**, after its gap check and before it writes code, it decides by N6 whether to build alone or to write a
   Plan of pieces on its own ticket and start helpers.
2. **Partway through**, if the work turns out bigger than it judged, it stops coding, commits what it has, writes a Plan
   for the *remaining* work, and hands that to helpers.

It doesn't ask the user about size. It still asks about design choices the ticket didn't settle, as today.
*Replaces:* the earlier idea of the build agent stopping to ask the user, and the idea of an `otaq stage replan`
command. Both were dropped.

### N9. A helper whose piece is too big hands back early

The helper commits what it built and runs `otaq helper done` with what is left and a **suggested split** ("piece 3 also
needs the export API; suggest 3a export API, 3b page"). The agent in charge merges the finished part, adds the new
pieces to the Plan and starts helpers for them. Helpers stay **one level deep**, and only the agent in charge changes
the Plan.
*Rejected:* helpers starting their own helpers. Reasons: the helper's context is already the fullest, it can't see its
sibling pieces, the Plan, board and CLI assume one level, and reviews would stack up.

### N10. Skills carry behaviour; code enforces what the app needs

Users can replace any skill with their own (Import skill folder…), add instructions, or have Noyan write a stage
skill. So:

- whatever the board, its data or its labels depend on is **enforced by the service**, with a clear refusal;
- how an agent works (piece size, re-splitting, investigation depth) lives in **skills**, and users may change it.

A custom skill that drops a behaviour rule loses that safety net but doesn't break the app. A custom skill that breaks
an app rule gets a CLI refusal.

### N11. The service refuses Noyan's own stage choices

This puts N4 into code (by N10), because the board's "set by you" label depends on it. A stage choice from the Project
agent is refused unless it carries `--by-user`. Refusal text, used word for word by the code and the skill:

> Noyan sets only the stage choices the user names: add --by-user, or leave the stage's own agent

### N12. Agents never skip a stage; only the user does (from O1)

A ticket's own agent may still pick a different **agent** for a later stage of its ticket, with a reason (for
example a stronger Validator when the change turned out risky). It may never set `skip`; the service refuses it.
`required` (turning a skippable stage back on) stays allowed. Together with N11, no agent can make a ticket skip a
stage: every skip on the board was the user's. Refusal text, word for word:

> Only the user skips a stage: pick another agent for it, or leave it

### N13. Noyan is "this project's coordinator" where agents read it (from O2)

In agent-facing text (the `noyan` skill, `otaq prime --project`) Noyan is no longer "the planning agent", and not
"the project agent" either. Always use the descriptor together with its defining sentence, word for word:

> You are Noyan, this project's coordinator. You turn the user's requests into tickets, look into how tickets are
> doing, and change the board's flow when asked. The flow's stages plan and build the work.

The website keeps "your planning agent" (hero, flow section, demo Ask bar). It's a pitch to people, and from the
user's side Noyan is where planning starts.

### N14. Choices that agents already stored, and the new rules refuse, are cleared once (from O3)

A one-time migration removes every stored choice the new rules would refuse, on a stage the ticket hasn't started:

- Noyan's own choices (`set_by = 'project_agent'`): skips and agent picks;
- ticket agents' skips (`set_by = 'agent'`, `choice = 'skip'`).

Those stages go back to their own agent. Kept:

- the user's choices, including agents' `--by-user` ones, which are stored as `user`;
- ticket agents' agent picks and `usual`/required;
- any choice on a stage that has already started (it's locked).

Each affected ticket's History says what was cleared and why.

## Dropped along the way

- "Every skip needs evidence". Moot for Noyan after N4. See open question O1 for the Dev agent.
- Noyan investigating until every piece passes a test (it duplicated the spec stages).
- The build agent escalating size problems to the user; `otaq stage replan`.
- A `planning: true` property on flow stages (see N5).

## Open questions (not decided; don't act on them)

- ~~O1. The Dev agent's later-stage choices.~~ Decided 2026-11-04: N12 (agent picks yes, skips no).
- ~~O2. "Planning agent".~~ Decided 2026-11-04: N13 ("this project's coordinator" for agents; website unchanged).
- ~~O3. Old choices on existing tickets.~~ Decided 2026-11-04: N14 (cleared once by a migration).
