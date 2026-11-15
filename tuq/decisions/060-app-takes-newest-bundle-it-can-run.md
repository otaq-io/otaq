# 060 · Each app takes the newest skill bundle it can run

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Related:** 058, 066

## Summary

A bundle says the oldest Otaq it works with. Each app uses the newest bundle whose range covers it, and every published bundle stays available, so an older app keeps getting skill fixes until a bundle needs a newer Otaq.

## Decision

- A workflow-skills release carries "needs Otaq ≥ X". The publish command keeps X as before unless the maintainer
  raises it, which they do when a workflow skill starts using something new in `otaq`. A tuq release carries no
  range: tuq's skills never depend on Otaq (decision 081).
- An app outside a bundle's range stays on the last bundle that works with it until the app is updated.

## Why

Skills call `otaq` commands; a skill newer than the app can call one the app lacks. Dropping older apps to the
skills built into them was turned down: customers who don't update would lose every skill fix since their build.
Was S12.
