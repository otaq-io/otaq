# 052 · Project settings → Flow never scrolls vertically

- **Date:** 2026-11-13
- **Status:** Accepted
- **Related:** OTAQ-4

## Summary

The Flow page of Project settings never shows a vertical scroll bar, neither the page nor anything inside the Flow
card: the card fits the window's height at every window size Otaq allows (1280 × 800 and up). A horizontal scroll is
fine, so the stage strip keeps its single row from Backlog to Done and scrolls sideways when it is wider than the card.

## Decision

- No vertical scroll bar on Project settings → Flow: not on the page body, not on the stage strip, not on any other
  part of the card (title row and actions, the waiting banner, the "changed by" line).
- This holds at any window size Otaq allows (its minimum is 1280 × 800) and for any number of stages, with or without
  the waiting banner.
- A horizontal scroll is allowed: the stage strip stays one row, left to right, and scrolls sideways when the flow
  is longer than the card. Its look (step boxes, connector lines, Backlog and Done ends) doesn't change.
- Only the Flow page is in scope; other Project settings pages keep their vertical scroll.

## Why

The user saw a vertical scroll bar on this page, which holds a single card that is much shorter than the smallest
window, so a scroll there is only noise. The ticket first said the opposite (no horizontal scroll, vertical fine); the
user corrected it on 2026-11-13: the vertical scroll is the problem and a horizontal one is fine.
