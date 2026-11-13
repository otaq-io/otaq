# 066 · A skill bundle that can't be used is skipped quietly

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 058, 059, 060, 076

## Summary

Otaq uses only a bundle it has fully downloaded, whose signature matches and whose range covers the app. Otherwise it keeps the bundle it has, shows no error and tries again later.

## Decision

- Offline, GitHub down, a cut-off or damaged download, a refused signature, a bundle needing a newer Otaq: the
  current bundle stays.
- With no usable downloaded bundle newer than the app's own skills (a new install offline, for one), the skills
  built into the app run (decision 076).
- Agents never wait for a download.

## Why

Agents must keep working whatever the network does; the user has nothing to do about a failed check. Was S22.
