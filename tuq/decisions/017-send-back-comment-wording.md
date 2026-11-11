# 017 · A send-back comment names each stage once

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

When a stage's agent sends a ticket back to an earlier stage, the ticket's comment reads "Dev sent it back to
Product spec: <reason>".

## Decision

- The comment is "<stage> sent it back to <earlier stage>: <reason>", for example "Dev sent it back to Product spec:
  the spec doesn't say who can open the link".
- Each stage is named once.

## Why

Piece 21 of the release fixes wrote "Dev asked to send … back to Product spec: <reason>", which repeats the stage name
(issue 19). The user chose the short form, which says what happened. "Dev asked to go back to …" and keeping the
built wording were turned down.
