# 023 · An undone CHANGED row says it was undone

- **Date:** 2026-11-11
- **Status:** Accepted

## Summary

After Undo on one of Noyan's CHANGED rows, the row stays and says the change was undone ("Undone · SHOP-7 no longer
waits for SHOP-6"), instead of keeping its old text without Undo.

## Why

Release-issues piece 14 kept the row's text after Undo, which reads as if the change still held. The user chose to
have the row say it was undone.
