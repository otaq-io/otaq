# 03 · Prompts during "Merge check failed · fixing" show no Needs you

**Kind:** bug · **Release:** must fix · **From:** re-test 5 NEW-3

## What happens
- SHOP-32's Dev agent stopped at Codex prompts (`git rebase main`, `git diff …`) while the ticket read "Merge check
  failed · fixing": the pill and card kept "Merge check failed · fixing" and the strip had no Needs you. The prompts
  were only found by opening the session.

## Evidence
Screenshots in `../shots-retest-5/`: `new-fixing-prompt-pill-no-needs-you.jpg`, `new-fixing-prompt-card-no-needs-you.jpg`

## Where to start
The merge-check-fixing status (conflict/merge-check fixing statuses in crates/otaq-core/src/domain and signals'
transition), and whether a prompt signal is ignored in that state.

## Open questions
- Needs you over "fixing", like Working → Needs you everywhere else? (Expected: yes.)
