# 085 · Skills have their own signing key, replaced by an app update if it leaks

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Related:** 059, 076, 084

## Summary

Skill releases are signed with a key of their own, never the app-update key. If the skills key leaks, an app update,
signed with the untouched app key, brings a new skills key and refuses tags signed by the old one.

## Decision

- Two keys on the maintainer's Mac: the app-update key and the skills key. The app carries the skills key's public
  half.
- A tag's signature covers its message (version, Otaq range, note) and all its files (decision 084); an app never goes
  below the skills built into it (decision 076).
- After a leak: an app update carries the new skills key and refuses every tag signed by the old one; auto-updating
  apps are safe again within a day. Until then the leak is not otherwise contained.

## Why

Skills are instructions every customer's agents follow, so a leaked key would let someone sign skills for all of them
(gap G14). Using the app-update key for skills too was turned down: one leak would then compromise both, leaving no
trusted way to replace it.
