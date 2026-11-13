# 11 · post-merge runs in the user's own checkout without a word

**Kind:** missing · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 11, scenario 2

## What happens
- After every merge the fast-forward runs `post-merge` in the user's own repo folder (husky-style "npm install on
  post-merge" means an install there that Otaq doesn't show).

## Evidence
Screenshots in `../shots-hooks/`: The hook log table in ../RESULTS-hooks.md.

## Where to start
`git/merge.rs` (`merge --ff-only` into the user's checkout).

## Open questions for the product spec
- Say it in the merge dialog ("your post-merge hook runs in ~\…") when the repo has one? Show its output?
