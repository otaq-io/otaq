# 033 · Agents push when the user asks, or when delivering

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 034

## Summary

An agent doesn't push unless the user asks it to (in the ticket, a comment or Tell now) or a delivery step (Approve &
open PR, Push to PR) has it push. "Never push" becomes "Don't push unless the user asks".

## Decision

- An agent pushes only when the user asked for it (in the ticket's text, a comment, or Tell now) or when it runs a
  delivery step that pushes (Approve & open PR, Push to PR). A plain request is enough: no "override" words are
  needed.
- When it pushes, it waits for the push to finish, however long the project's pre-push hook takes, and reports the
  real result. After a delivery push Otaq also checks the remote itself (034).
- A pre-push hook that refuses the push is never bypassed: the agent says what the hook said and asks the user how to go
  on, as it does for a refused commit.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/03`) found an agent refusing a push the user asked
for in a comment, because the skills said "Never push", until the user wrote "I override the no-push rule". It also
found an agent giving up on a slow (50 s) pre-push and reporting a push that had in fact succeeded. Turned down:
pushing only through the delivery steps (it refuses a plain request from the user).
