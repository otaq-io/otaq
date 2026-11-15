# 086 · Security check and Update docs get their own Product spec before release

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 057, 081

## Summary

`tuq:security-check` and `tuq:update-docs` each get a Product spec of their own before the release that brings tuq,
so the Security review and Update the docs ready stages ship with working skills.

## Decision

- Each spec settles what the skill checks or updates, what passes, and what it never does.
- Both are task skills: they never depend on how Otaq works (decision 081).
- The release that brings tuq waits for both.

## Why

The two ready stages name skills that don't exist and that nobody has described (gap G20). Hiding the two stages until
their skills exist was turned down.
