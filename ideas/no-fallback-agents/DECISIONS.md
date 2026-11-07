# Otaq drops fallback agents

Agreed with the maintainer on 2026-11-06, while going through the questions left by `ideas/no-skill-copies`. The old
`decisions/` folder is lost, so this file is the record. Not built yet. Refined the same day in the gap review
(`ideas/gap-review/REPORT.md`, G-numbers; marked "gap review" below).

## Decision

When a tool can't run, its agents wait for it. Otaq never moves their work to another tool on its own or in bulk.

**What is removed** (the features as the product has them; refined in the gap review, G2):

- **Fallback per tool** in Settings → Agents ("When Claude Code hits its limit → …");
- the card's **Use fallback** and the limit banner's **Use fallback now**;
- what came with them: the card's amber "On Codex · Claude Code limit" line, the "on the fallback" counts in the
  banner and its notification, and "has no fallback: set one in Settings → Agents".

The automatic setting (**Use fallback agents automatically**) and **Move waiting to…** were already gone before this
decision (user decision 2026-10-20: nothing moves to a fallback without the user's click).

**What waits** (gap review, G13, G14): a tool at its usage limit, a tool that is logged out, not installed, hidden or
needs an update, and an agent whose model its tool no longer lists. The card says why and offers **Open agent…** and
**Change agent…**.

What stays:

- **Every tool stays supported**, and more will be added later. An agent is still put on any tool on purpose (Codex as
  Validator, for example).
- **Limits per tool:** only the limited tool's agents wait, the rest keep going, and the banner says so.
- **One ticket at a time, by hand** (gap review, G1): for any stage of a ticket that hasn't started, the user can pick
  any agent whose tool has the stage's skill (`ideas/one-check-agent`, "Other stage features", O1). A waiting stage
  hasn't started, so the waiting card's **Change agent…** is that same choice.
- **Noyan** (gap review, G16) keeps **New agent on…** when its tool can't run: the user's click, a new conversation,
  offering only agents on a tool that can run.

Only the user picks another agent for a ticket's stage (or Noyan, passing on the user's words). A ticket's own agent
no longer picks an agent for a later stage of its ticket (gap review, G12): that would move work to a tool the user
didn't choose, as a fallback did.

Skip this check, on a check waiting for its tool's limit, shows only on a check the flow marks skippable
(`ideas/one-check-agent`, O5; gap review, G3).

## Why

A fallback runs the work on a tool that may not have what the stage relies on: the user's personal skills (a skill in
`~/.claude/skills` doesn't exist for Codex), the project's `CLAUDE.md`, the same hooks, a model of the same strength.
A ticket can then quietly run its stage without its skill, or pass or fail validation because of the tool rather than
the work. Waiting for a limit to reset costs hours at most; a ticket quietly done elsewhere can cost a review round.

Since Otaq no longer copies skills, this is a rule, not only a reason: **a stage runs only on a tool that has its
skill** (`ideas/no-skill-copies`, S5).

## Moving over (gap review, G15)

- The stored fallback settings are dropped, with no notice.
- A ticket the user sent to a fallback that hasn't started there yet waits for its own tool again, like every other
  ticket.
- A session already running on a fallback finishes where it is: a conversation never changes tools.
- History keeps its old "Started on Codex · Claude Code limit" lines.
