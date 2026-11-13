# 03 · Re-test the checks that need Claude Code signed in

**Kind:** re-test still to do · **Release:** must do · **From:** re-test NOT TESTED, piece 3 / failure 8, part of piece 4

Claude Code is signed out in the VM, and a signed-out Claude Code no longer starts at all; it is held instead (that
part passes: `../shots-retest/p03-held-signed-out.jpg`). So these were never seen:
- **Piece 3 / failure 8:** a session that ends on a tool error (e.g. login expired) gives the card "Stopped · login
  expired", then Sign in…, then Resume.
- **Piece 4:** the drawn restart row "Claude Code isn't signed in · sign in first" (needs an agent that ran on Claude
  Code before the restart).
- **Sign in…:** does it open `claude auth login`?
- **notes' Spec first flow:** the Product spec / Tech spec / Milestone plan stages, whose agents run on Claude Code.

Needs the maintainer to sign Claude Code in on the VM (and sign it out again in between, to cause the tool error).
