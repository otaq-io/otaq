# 099 · Skill updates can be turned off in Settings

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Supersedes:** 088
- **Related:** 058, 068, 069, 076

## Summary

Settings gets "Update Otaq's skills automatically", on by default, beside the app's own update setting. Turned off,
Otaq stops looking for new skill releases and keeps the skills it has.

## Decision

- On (the default): Otaq checks for skill releases as decision 068 says.
- Off: no checks; new sessions keep using the newest release Otaq already has, or the app's built-in skills when it
  has none newer (decision 076). Turning it on again checks at once.
- The setting is per machine, for every project. About still shows which skills run (decision 069).
- In a released app it is on by default; in a build from source it is off by default, and turning it on is how a
  developer tests a skills release (replacing 088's separate developer setting).

## Why

A released app always took new skills (decision 088), so a team with change-control rules, or someone in the middle of
careful work, couldn't hold agent behaviour still. A shared per-project pin was turned down as more to build and to
explain for now; keeping 088 as it was left no way out.
