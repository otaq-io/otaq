# 17 · Probably the same newest-first bug in `parked_from`

**Kind:** suspected bug, unconfirmed · **From:** piece 19's agent

Piece 19 fixed "What the agent did" showing an older round instead of the latest one. Its agent noticed the same
ordering pattern in `parked_from` (`crates/otaq-desktop/src/api/board.rs:437`) and left it alone because it was out of
scope. Check whether a parked ticket names the wrong stage it was parked from after more than one round.
