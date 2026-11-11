# 019 · Choices the release-fix piece agents made are kept

- **Date:** 2026-11-10
- **Status:** Accepted
- **Related:** 009, 016, 017, 018

## Summary

The release-fix plan left some behaviours open, and the piece agents chose them while they worked. The user confirmed
these choices as the product's behaviour.

## Decision

- **Crash in a request:** a request that crashes inside Otaq answers with an error, "This is a bug in Otaq; please
  report it", instead of not answering.
- **Couldn't check:** a tool whose state can't be checked shows an amber "couldn't check" state with Re-check, for
  example an older Claude Code that can't report its sign-in, or Copilot while offline.
- **Tool-error Idle:** Resume on an agent that stopped on a tool error types "continue" to it. The card's action is
  Open.
- **Delivery session:** the delivery session stops whenever the ticket moves out of Review, To deliver or PR open,
  whatever the move.
- **Held tickets:** a held ticket counts as in its stage, so reorders and flow switches wait for it too.
- **Move on anyway:** after a check gives no result, "Move on anyway" shows on skippable checks only. A check that ran
  out of rounds can always be sent on (009), and Overview shows that on every flow.
- **Search:** Enter on a search with no match opens the words box and sends nothing.
- **Own-agent line:** a ticket's own-agent line also shows the agent picked for a later stage while the ticket is in
  an earlier one.
- **Free agent wording:** the "free agent" wording shows only while a ticket waits for a free agent slot.
- **Palette:** the palette says "Resume <project name>" (it said "Resume this project").
- **Permission with no command:** when an agent asks for permission and there is no command to show, the line reads
  "<agent> wants your permission".
- **Plan hint:** the "1 of 3" hint is in the agent's own words, and "returns to Review" is fixed text.

## Why

The plan didn't say how these should behave, so each piece's agent chose (issue 19). The user went through them
before the release and kept these, so later work treats them as intended rather than as accidents. The choices
changed instead are in 016 (the drafting line), 017 (the send-back comment), and 018 keeps piece 6's "unsent text"
change.
