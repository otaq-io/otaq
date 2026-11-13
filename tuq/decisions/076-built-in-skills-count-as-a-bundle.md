# 076 · The app's built-in skills count as a bundle

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 058, 060, 066, 067

## Summary

The skills built into an app count as one more bundle, dated by the commit they were built from. Otaq uses the newest
usable one of all of them, so a new app never goes back to skills older than its own.

## Decision

- Bundles, the built-in skills among them, are ordered by the commit their skills come from; the newest whose range
  covers the app and whose signature matches wins (decisions 060, 066).
- A downloaded bundle older than the app's own skills is never used by that app.
- A bundle says only the oldest Otaq it works with; there is no "newest Otaq" limit.

## Why

Without an order, a new app could switch to a bundle published before its release, missing what the new app needs
(gap G2). An upper limit per bundle was turned down: this rule already keeps an old bundle off a newer app, and it
would be one more thing to set at every publish.
