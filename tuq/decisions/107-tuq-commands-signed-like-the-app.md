# 107 · tuq's commands are signed as the app is; real certificates come later for both

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 077, 106

## Summary

`tuq` and `tuq-plan` are signed exactly as Otaq's app is today: ad-hoc on macOS, not at all on Windows. Signing both
with real certificates (an Apple Developer ID with notarization, a Windows code-signing certificate) is a later
decision, taken for the app and tuq together.

## Decision

- Each tuq release's commands get the same signing as the app of the time; when the app moves to real certificates,
  tuq's commands move with it.
- Until then, a machine that blocks unsigned programs (AppLocker, WDAC, Defender rules, Gatekeeper) may block tuq's
  commands as it may block the app; that is accepted for this release.

## Why

The app itself is ad-hoc signed on macOS (decision 0021 of the old log) and unsigned on Windows, so signing only tuq's
commands with real certificates would protect nothing on a machine that already blocks the app. Buying certificates
now was turned down for this release; shipping tuq unsigned whatever the app does was turned down so the two never
drift apart.
