# 07 · No progress, Stop or time limit while hooks run

**Kind:** missing · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 7, scenarios 5 and 6

## What happens
- Slow pre-commit (45 s) and post-checkout (40 s, run twice): the Approve & merge dialog stayed on step 1 "Fetch main
  and bring it up to date" for the whole run (46 s and 82 s) while git was on later steps. Nothing says a hook runs.
- `Git::run` has no timeout (only kill-on-drop): a hanging hook keeps "Merging…" up forever. Same for a start's
  `worktree add` (post-checkout).
- The free rebase stopped the delivery agent mid-message; the merge then used the plain message.

## Evidence
Screenshots in `../shots-hooks/`: `s5-slow-precommit-merge-shows-fetch-step-12s.jpg`, `s5-slow-precommit-merge-done-46s.jpg`,
`s5-slow-postcheckout-merge-still-fetch-step-33s.jpg` (`-60s`), `s5-slow-postcheckout-merge-done-82s.jpg`

## Where to start
`Git::run`, the merge progress steps, `start/worktree.rs`.

## Open questions for the product spec
- The real step and "running your pre-commit hook… 40 s", with the hook's live output? A Stop? What timeout, and is
  it a setting?
