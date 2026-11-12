# 05 · Approve dialog says "still writing the message…" forever when no writer exists

**Kind:** bug · **From:** re-test NEW-3 (piece 5)

SHOP-10's Approve dialog said "Developer is still writing the message…" and never filled in. The service log shows the
writer was asked once at 12:08 under the old build (that session never finished). It was not asked again, because the
branch head didn't change. No writer session existed.

This came from the old build's data, but it can happen to any user who updates the app with a ticket in Review. The
dialog should not say someone is writing when no session is running; ask again, or offer the plain message.
