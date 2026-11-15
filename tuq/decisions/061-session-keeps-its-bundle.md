# 061 · A session keeps the skill bundle it started with

- **Date:** 2026-11-14
- **Status:** Superseded by 101
- **Related:** 060, 062

## Summary

An agent session uses the bundle that was newest when it started, for its whole life, resumes included. Only a new session gets a newer bundle.

## Decision

- Resume, a Send back and a restart keep the session's bundle.
- The next stage, a new ticket and a new Noyan conversation get the newest bundle, so a ticket may run its stages
  on different bundles.

## Why

A skill changing under a running session would mix two versions of the same instructions in one conversation. Each
stage is its own session, so a ticket moving to a newer bundle between stages is fine. Was S13.
