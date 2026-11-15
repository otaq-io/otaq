# 095 · A release's date is set by the publish command; a future-dated release waits

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Related:** 066, 067, 069, 076, 085

## Summary

The publish command takes a release's version from the maintainer's date of publishing; nobody types it. An app
ignores a release dated after its own today until that day comes, so a mistaken or forged far-future version can never
beat later fixes.

## Decision

- The version is the publish day (`2026.11.14`, then `.2` for a second release that day, decision 069), set by the
  publish command and never typed or edited.
- An app treats a release dated after its own date as not usable yet (one more case of decision 066): it keeps the
  release it has and uses that one once its date arrives.
- An app whose clock is behind waits for a new release until its clock reaches the release's date; nothing is shown.
- No withdraw is added: decision 067 stands.

## Why

Every app takes the newest usable release (decision 076), so a release dated `2062.11.14` by a typo, a wrong clock or a
leaked key (decision 085) would beat every later fix, and publishing again (decision 067) couldn't undo it. Letting a
release retire earlier versions was turned down as the withdraw path 067 rejected; accepting the risk was turned down
because customers who don't update the app would stay stuck.
