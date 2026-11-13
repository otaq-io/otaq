# 047 · One ticket per scope; its work is split into pieces, never into tickets

- **Date:** 2026-11-12
- **Status:** Accepted

## Summary

Noyan files one ticket for one scope: one goal the user wants delivered (a feature, a milestone, a fix), however big
it is. The flow's stages split it into pieces on that ticket, built by helpers in one tree and checked by the
ticket's agent as they merge. No agent splits a ticket's work into several tickets.

## Decision

- **Noyan:** one ticket per scope, even when the user lists its parts. Several tickets only for several scopes (goals
  the user would name apart) or a tracker import (one per issue). When unsure, it proposes one ticket and asks.
- **The user asks for several tickets for one scope:** Noyan asks once, one ticket built in pieces or separate
  tickets, then does what the user chooses. It skips the question when the user already shows they know the trade-off.
- **Noyan reads the flow only to explain:** it names the stage that will split the work ("I recommend one ticket: the
  Milestone plan stage will plan it and split it into pieces"). The one-ticket rule is the same in every flow. When no
  stage in the flow says it splits work into pieces (for example a replaced build skill), Noyan recommends neither.
- **The planning stage never suggests another ticket:** everything the ticket holds, even an unrelated goal, becomes
  pieces of it. Small pieces keep each agent's context small.
- **Follow-ups stay allowed:** any ticket agent may file a follow-up ticket in Backlog for something new it discovers
  outside what the ticket asks for, never to split the ticket's own work.
- A skill change only: the app enforces nothing new.

## Why

Tickets Noyan split from one feature waited on each other's merges, had no orchestrator to check them together, and
relied on Noyan's ticket text alone for the checks. One ticket with pieces keeps the work in one tree under one agent
in charge. Agents get confused when their context grows, so the work is split, but into pieces, each helper
responsible for its own. Refines the 2026-11-04 Noyan intake agreement ("several tickets only for clearly separate
deliverables"), whose "separate" was too vague.
