# 04 · The message-writer request reaches the Dev session after the ticket was sent back

**Kind:** bug · **From:** re-test NEW-2 (piece 5)

When the delivery's agent is the ticket's own Dev session: SHOP-14's merge check failed and sent it back to Dev. After
that, the Dev Codex session still got "SHOP-14 is waiting in Review. You are its delivery's agent: write the message…"
typed in, followed by the restack request. It had been queued behind text left in Codex's input line, and it was sent
once the line was free, although the ticket was no longer in Review.

Expected: a queued writer request is dropped when the ticket leaves Review/To deliver/PR open.
Evidence: `../shots-retest/p05-stale-writer-prompt.jpg`.
