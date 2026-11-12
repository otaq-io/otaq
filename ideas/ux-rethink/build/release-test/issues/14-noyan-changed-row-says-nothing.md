# 14 · Noyan's CHANGED row doesn't say what changed

**Kind:** missing from the design · **From:** piece 22 (failure 14 passes, partly)

When Noyan made SHOP-16 wait for SHOP-15, the panel showed "CHANGED 1 · ✎ SHOP-16 Retest dep B · Backlog". The canvas
(noyan-01 · 4) draws "SHOP-7 · now waits for SHOP-6 · Undo". The what-changed text and Undo aren't built; the piece's
agent said this needs a protocol change. Also, a ticket that was created and then made to wait shows under both CREATED
and CHANGED.
Evidence: `../shots-retest/p22-changed-row.jpg`.
