# 079 · Every set of Otaq's skills has its OpenCode version

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 072, 076, 073

## Summary

Whatever Otaq's skills an OpenCode session runs on (the app's built-in ones or a released tag), it finds them under
their OpenCode names (`otaq-validate`, `tuq-product-spec`, decision 072).

## Decision

- The built-in skills and every release have the OpenCode version.

## Why

Without it, an OpenCode session on built-in skills (a new install offline, a build from source) would be told to load
`otaq-validate`, which wouldn't exist, and stop (gap G5). It follows from decisions 072 and 076, so nothing was asked.
