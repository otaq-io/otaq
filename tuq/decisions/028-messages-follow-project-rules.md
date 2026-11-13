# 028 · Commit messages follow the project's own rules

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 010, 026

## Summary

The agent writing a ticket's merge message, and agents committing their own work, first find the project's commit
message rules and follow them. The ticket key leads the message only when the project says nothing. There is no
message-template setting.

## Decision

- Before writing a merge message (or a commit), the agent looks for the project's message rules: CONTRIBUTING and
  similar docs, a commitlint or similar config, the repository's hooks, the user's Agent instructions, and the style of
  recent commits on the target branch.
- When it finds a rule, it follows it (for example `feat: add subtraction (HOOK-1)`), keeping the ticket key in the
  message where the rule allows.
- When nothing says otherwise, today's default stays: the message starts with the ticket key.
- This covers the merge message the user sees in Approve & merge and agents' own commits in their worktrees. Otaq adds
  no message-template setting.
- When a hook still refuses the merge message, 026 applies: the ticket shows the hook's output and the user edits the
  message and retries.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/01`) found that a Conventional Commits commit-msg
hook rejected every squash message (each merge needed a hand fix) and every agent's first commit. With CONTRIBUTING
and the hooks in view, the agent got the format right first time, so reading the project's rules solves it with no
new setting for the user to find. Turned down: a project message template, and a template that wins over the rules
(more to build and explain).
