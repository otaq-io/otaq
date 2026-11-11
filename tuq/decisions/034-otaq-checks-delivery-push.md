# 034 · The delivery agent pushes; Otaq checks the push landed

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 033, 039

## Summary

In Approve & open PR and Push to PR the delivery agent still pushes and opens the PR. When it says it is done, Otaq
checks that the branch is on the remote at the right commit, and shows a mismatch on the ticket as Needs you.

## Decision

- Approve & open PR and Push to PR: the delivery agent pushes the branch and opens or updates the PR, as today, and
  pushes the way 033 says: it waits for the pre-push hook however long it takes, reports the real result, never
  bypasses a refusal, and asks the user how to go on.
- When the agent reports the push or the PR as done, Otaq checks that the remote has the ticket's branch at the commit
  it should. If it doesn't, the ticket shows Needs you: "The branch isn't on the remote as expected", with what the
  remote has.
- When a push is refused, the messages Otaq reworded for the PR stay reworded on the local branch (nothing was sent),
  and the user's edits are kept (031). Trying again only pushes again.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/03`) found an agent reporting "the push reported no
completion" for a slow push that had in fact succeeded, and the ticket went on with a wrong report. Otaq checking the
remote catches a wrong report without splitting delivery between Otaq and the agent. A first version of this decision
(agreed earlier the same day, never committed) had Otaq run the push itself. On a step back the user dropped it: it
split delivery in two, added a background step with its own refusal path, and left open what a refused push does to
the reworded history. Also turned down: trusting the agent's report as today.
