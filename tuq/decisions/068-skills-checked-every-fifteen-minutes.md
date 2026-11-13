# 068 · Otaq checks for new skills at service start, then every 15 minutes

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 058, 066

## Summary

Otaq's background service asks for the newest bundle when it starts and every 15 minutes after. A published fix reaches every running Otaq within about 15 minutes, and the next new session uses it.

## Decision

- One small request to the skills repository for its tags (decision 084), sending nothing about the user.
- A session never waits for the check.

## Why

Otaq has no server to push updates, so it asks. Every 5 minutes was turned down as more traffic for little gain,
and checking before each new session because a start would wait on the network. Was S24.
