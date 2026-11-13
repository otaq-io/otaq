# 07 · Ticket stays "Working · in the background" while Codex sits idle

**Kind:** bug · **From:** re-test NEW-5

SHOP-13, in Update the docs with the `retest-docs` skill:
- The card stayed "Working · in the background", and Session said "Waiting on its background work".
- Meanwhile Codex sat idle at its prompt after answering. The skill told it to finish, but it never called
  `otaq stage next`.

The user is never asked to act, so the ticket can sit there indefinitely. It should read as idle / needing the user.
Evidence: `../shots-retest/p07-nobom-session.jpg`.
