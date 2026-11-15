# 087 · A Reproduce stage runs before Dev

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 065, 081, 083

## Summary

A Reproduce stage, running `tuq:reproduce`, comes before Dev: an agent reproduces a reported bug and shows the
failing check (a test, a request, exact steps) before anyone fixes it. Dev then shows that the same check passes after
the fix.

## Decision

- `tuq:reproduce` is a tuq task skill (decision 081): it never depends on how Otaq works, and works in a plain
  terminal too.
- A ticket that isn't a bug (a feature, a docs change) passes the stage at once with a short note.
- It is a ready stage in Edit flow's + palette, placed before Dev. Otaq default and the other shipped flows don't
  include it: new users see no extra column.
- Dev's comment says that the check Reproduce showed failing now passes.

## Why

A fix nobody saw fail can't be shown to work. The user chose a visible stage before Dev over making it part of how Dev
works, and over a skill Dev runs itself.
