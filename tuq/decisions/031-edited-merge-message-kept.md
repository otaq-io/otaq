# 031 · An edited merge message is kept until the ticket lands

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 010, 026, 028

## Summary

Once the user edits a ticket's merge message (or its PR title and description), Approve shows the edit every time,
across closing the dialog, refusals and app restarts, until the ticket lands or the user goes back to the proposed
message.

## Decision

- The user's edit to the merge message in Approve & merge, or to the PR title and description in Approve & open PR,
  is kept on the ticket.
- Reopening the dialog, after a hook's refusal (026), a closed dialog or an app restart, shows the edit, not the
  proposed message.
- It is kept until the ticket lands (merged, or the PR opened with it), or until the user chooses **Use the proposed
  message**, which puts back the message the agent proposed.
- A Send back doesn't drop it.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/05`) found the edit thrown away after a refusal,
so a user fixing a message for a commit-msg hook had to fix it again on every try. Turned down: keeping it only while
the app runs (a restart mid-fix loses it), and dropping it on a Send back.
