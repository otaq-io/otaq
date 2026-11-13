# 08 · Small views still name the first stage for a held ticket that skips it

**Kind:** bug · **Release:** can wait · **From:** piece 27's hand-back (round 4)

## What happens
- Piece 27 moved a held ticket to the column of the stage it starts at, on the board and the List view. Views that
  build plain `Columns::of` without the card's agents (the palette, the sheet's column id, dependencies, the filter)
  still name the flow's first agent stage for such a ticket.

## Evidence
Screenshots in `../shots-retest-5/`: none.

## Where to start
crates/otaq-view/src/view_models/flow_columns.rs (`Columns::with_starts`), board.rs `columns(input)`, and the callers
of `Columns::of`.

## Open questions
- none.
