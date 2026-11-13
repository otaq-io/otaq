# 032 · A hook's changes to the landing commit land, and Otaq says so

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 029

## Summary

When a hook (a formatter, for example) changes files in the commit Otaq is landing on main, the commit lands with the
change, and Otaq says so: "Your pre-commit hook changed 2 files", with Show changes, in the merged message and the
ticket's History.

## Decision

- Applies when a hook changes the content of the commit Approve & merge makes, so main differs from the branch the user
  reviewed.
- Otaq doesn't stop or ask: the commit lands as the hook left it.
- The merged message and the ticket's History say which hook changed how many files ("Your pre-commit hook changed 2
  files"), with **Show changes**, the difference between what the user reviewed and what landed.
- The Merge check doesn't rerun because of it.
- When no hook changed anything, nothing extra is shown.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/06`) found a reformatting pre-commit adding lines
to main that the user never reviewed, with no word from Otaq. Formatting hooks are common and are what the team
wants, so stopping on every formatted merge would cost a second approval each time. Saying so keeps the user
informed. Turned down: stopping to ask (Merge anyway / Send back) and rerunning the Merge check before landing (slower
on every formatted merge).
