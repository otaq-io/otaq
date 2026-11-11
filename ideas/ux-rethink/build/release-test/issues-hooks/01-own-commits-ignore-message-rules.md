# 01 · Otaq's own commits can't pass a project's commit-message rules

**Kind:** bug + design · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 1, scenarios 2 and 7

## What happens
- The squash commit Approve & merge makes always starts with the ticket key ("HOOK-1 Add subtraction…"): a `feat:`/
  `fix:` (Conventional Commits, commitlint) commit-msg hook always rejects it, so every merge needs the message fixed
  by hand.
- The checkpoint commit `wip: paused by Otaq` (a reopened ticket with uncommitted files) can never pass any message
  rule, so such a ticket can never start; the only way on is to commit or delete the work by hand.
- The error for the checkpoint says "Your **pre-commit** hook rejected Otaq's temporary checkpoint" when it was the
  commit-msg hook, under a "creating the worktree failed" heading, and drops the hook's message.
- PR rewording (`git/reword.rs`, `git commit-tree`) never runs the commit-msg hook, and the PR dialog doesn't say what
  the project's rule is.
- Agents' own commits use `<KEY>: …` (commit skill step 4) first, so each costs one refused try; with husky's
  `.githooks` and a CONTRIBUTING in view the agent got it right first time.

## Evidence
Screenshots in `../shots-hooks/`: `s2-merge-dialog-message-starts-with-key.jpg`, `s2-commitmsg-rejects-merge-toast.jpg` (+ `-zoom`),
`s7-checkpoint-rejected-by-commitmsg-says-precommit.jpg`, `s7-close-keeps-worktree-with-uncommitted.jpg`,
`s1-commitmsg-rejects-agent-adapts-feat.jpg`

## Where to start
`crates/otaq-core/src/git/merge.rs` (squash, `commit --allow-empty -F`), `git/checkpoint.rs`, `git/reword.rs`, the
delivery agent's message instructions, `plugin/skills/commit/SKILL.md` step 4.

## Open questions for the product spec
- Where does the format come from: a project setting (a message template), the project's own rules the agent reads
  (CONTRIBUTING, commitlint config, Agent instructions), or learning from a refused try?
- Does the checkpoint need to be a commit at all (stash-like ref, `--no-verify` for Otaq's private commit, a message
  that follows the template)? Bypassing a user's hook is a product decision.
- Should PR rewording run the commit-msg hook, or check the template, before it rewrites?
