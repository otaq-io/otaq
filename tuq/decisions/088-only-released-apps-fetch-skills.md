# 088 · Only released apps fetch new skills

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Related:** 058, 068, 076

## Summary

A released Otaq (macOS, Windows, Linux `.deb` and AppImage) fetches new skill releases as decision 068 says. A build
made from source runs the skills it was built with, unless a developer setting turns fetching on.

## Decision

- Builds from source never fetch on their own, so a contributor tests exactly the skills they built with.
- A developer setting turns fetching on, to test a skills release before customers get it.

## Why

A newer release could otherwise silently replace the skills a contributor is testing (gap G18). Fetching in every
build was turned down for that reason.
