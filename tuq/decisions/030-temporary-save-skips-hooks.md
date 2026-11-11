# 030 · Otaq's temporary save of unfinished files isn't judged by hooks

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 028, 029

## Summary

When Otaq saves a worktree's uncommitted files for a moment so it can bring the branch up to date, the project's hooks
don't judge that save. It is undone right after and never lands anywhere. Every commit that does land still runs
the hooks.

## Decision

- Applies only to Otaq's temporary save (today the `wip: paused by Otaq` checkpoint) around bringing a branch up to
  date (029), for example at a reopened ticket's start or before Approve & merge. The files come back exactly as they
  were afterwards.
- The project's hooks (pre-commit, commit-msg and the rest) don't run on that save, so half-done work or the save's
  own message can't stop a ticket from starting or merging.
- The temporary save isn't signed either, for the same reason: it never lands, and a signing key with a passphrase or
  on a hardware token needs an answer Otaq, with no terminal, can't give, so a reopened ticket couldn't start. Every
  commit that lands is still signed as the user's git config says.
- This is the **only** exception. Every commit that reaches the branch, main or a remote, whether Otaq's or an
  agent's, runs the project's hooks, and agents still never bypass a hook.
- A hook that refuses another of Otaq's steps (the update itself, the merge commit) still shows on the ticket (026).

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/01`) found that a commit-msg rule refused the
checkpoint every time, so a reopened ticket with uncommitted files could never start, and the error blamed the wrong
hook. A lint pre-commit would refuse half-done work the same way. Nothing of this save lands, so the hooks have nothing
to protect there. The user accepted this one exception to "Otaq never bypasses a hook". Turned down: keeping the
hooks and making the save's message follow the project's rules (lint hooks would still refuse it, and the user would
have to commit or throw away the work by hand).

## Changelog

- 2026-11-11: refined: the temporary save also skips commit signing; every commit that lands is still signed.
