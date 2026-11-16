# 117 · Copilot's removed "Auto · efficiency": new starts wait, resumes go on as balance

- **Date:** 2026-11-16
- **Status:** Accepted

## Summary

Copilot CLI 1.0.112 removed its "efficiency" Auto profile. Otaq stops offering it; an agent saved on it starts nothing
new until the user picks another model, while a conversation that already ran on it resumes on "Auto · balance".

## Decision

- **Not offered:** the model picker lists Copilot's Auto as balance and intelligence only.
- **Saved agents:** an agent on `auto@efficiency` shows "no longer listed", and its new starts are held "Model
  unavailable" until the user picks another model (the existing rule for a model a tool drops; no data change).
- **Resumes:** a conversation that ran on efficiency resumes on balance, so it keeps its history and doesn't stop.
- The release notes say Copilot removed the profile.

## Why

Found in the 0.3 Windows regression test (F-E2-1): Copilot refused `--auto-tier efficiency` and the session showed only
"Stopped by a restart". For resumes, the user chose keeping the conversation over showing Copilot's refusal and
starting from scratch.
