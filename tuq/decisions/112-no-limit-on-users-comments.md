# 112 · The user's comments have no length limit

- **Date:** 2026-11-15
- **Status:** Accepted
- **Partly supersedes:** 003
- **Related:** 001, 007, 110, 111

## Summary

The user is never blocked by a comment's length: whatever they type or paste stays text, is posted as written and is
read in full by every later agent. Agents' comments keep the 2,000-character limit.

## Decision

- **No limit on the user's comments** in any comment box: no counter, no red box, and the send button never turns off
  because of length. Editing a posted comment follows the same rule.
- **A paste always goes into the text**, however long. The box no longer turns long pasted text into a "Pasted text"
  chip; this replaces 003's "Text over the box's limit still becomes a 'Pasted text' chip". Images and files pasted,
  dropped or attached are added as attachments, as before (003).
- **Long comments in the thread** show their first lines (about 15) and then "Show more", which opens the rest in
  place.
- **Agents read the whole text** in `prime`, like any other comment.
- **Agents' comments keep the limit:** at most 2,000 characters, and over it `otaq comment` is refused with today's
  message pointing to `--report-file`.

## Why

The user often copies text from somewhere else (another agent, Jira) and expects the agents to read it. Blocking them,
or quietly turning the text into an attachment, gets in the way of that. The limit stays on agents, whose hand-overs are
read by every later stage and should be short. Turned down: posting text over the limit as an attachment with a short
note (no blocking, but pasted text ends up behind a chip and it adds a hidden rule) and a higher limit for everyone. If
very large pastes turn out to crowd agents' context, that is a separate decision.
