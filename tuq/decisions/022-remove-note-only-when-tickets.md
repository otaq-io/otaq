# 022 · "Removing waits until no ticket is in it" shows only when the stage has tickets

- **Date:** 2026-11-11
- **Status:** Accepted

## Summary

In Edit flow's stage panel, the note beside Remove stage shows only while tickets are in that stage. With none, the
stage is removed at once and the note isn't shown; Remove stage stays at the panel's foot either way.

## Why

The Windows re-test of release-issues saw the note on stages with no tickets, where it isn't true.
