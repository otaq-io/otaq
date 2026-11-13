# 039 · Reworded PR commit messages go through the commit-msg hook

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 028, 031, 034

## Summary

In Approve & open PR, every commit message the user confirmed is checked by the project's commit-msg hook before Otaq
rewrites the branch or pushes. A refusal shows the hook's output, and nothing is rewritten or pushed.

## Decision

- Before Otaq rewrites the branch's commit messages for a PR, each confirmed message goes through the project's
  commit-msg hook.
- If the hook refuses any of them, nothing is rewritten and nothing is pushed. The refusal shows on the ticket (026),
  naming the message and the hook's output. The user's edits are kept (031), so they fix the message and try again.
- If every message passes, Otaq rewrites them and the delivery agent pushes (034).
- A repository with no commit-msg hook works as today.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/01`) found that rewording for a PR never ran the
commit-msg hook, so a message the team's rule rejects could reach the PR. Checking before anything changes keeps the
rule on every commit that leaves the machine. Turned down: leaving rewording hook-free and relying on the agent's
messages following the project's rules (028), since a hand-edited message can still break the rule.
