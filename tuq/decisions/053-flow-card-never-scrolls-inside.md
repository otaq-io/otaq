# 053 · The Flow card never scrolls vertically inside; the page still does

- **Date:** 2026-11-13
- **Status:** Accepted
- **Related:** OTAQ-4, partly supersedes 052

## Summary

Nothing inside the Flow card of Project settings scrolls vertically: not the stage strip, not the title row, the
waiting banner or the "changed by" line. The Project settings page itself keeps its vertical scroll, since it is one
page holding all six cards (Flow, Repository, Run, Workspace, Notes for agents, Remove project) and the scroll is how
the user reaches the cards below Flow.

## Decision

- No vertical scroll bar anywhere inside the Flow card, at any window size Otaq allows (1280 × 800 and up), for any
  number of stages, with or without the waiting banner.
- The page body keeps its vertical scroll, as for every other card.
- The rest of 052 stands: the stage strip stays one row and scrolls sideways when it is wider than the card, its
  look unchanged.

## Why

052 took "Project settings → Flow" for a page of its own holding a single card. In the app, Project settings is one
page scrolling through six cards and the left list only scrolls to a card, so the page's scroll can't go away for
Flow alone. Asked at Tech spec on 2026-11-13, the user chose to keep the page's scroll and make the Flow card itself
never scroll vertically, over turning each section into its own page.
