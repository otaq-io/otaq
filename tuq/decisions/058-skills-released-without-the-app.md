# 058 · Skills reach customers when they change, without an Otaq release

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Related:** 057, 059, 066

## Summary

Otaq fetches newer skill bundles by itself, so a skill fix reaches customers without a new version of the app. The skills built into the app are the fallback whenever no newer bundle can be used.

## Decision

- The maintainer releases skills on their own, without bumping Otaq's version.
- The skills a build ships (decision 057) run whenever no newer bundle is usable (decision 066).

## Why

The maintainer wants to fix and improve skills as soon as they change, not wait for an app release. Taking the skills
only at build time, so they update with the app's daily update, was turned down for that reason. Was S10.
