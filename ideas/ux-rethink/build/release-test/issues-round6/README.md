# Round 6 · release blockers left after re-test 6

Left open by the Windows re-test of release-fixes d0a165b5 (`../RESULTS-retest-6.md`, screenshots `../shots-retest-6/`).
Logged 2026-11-11. Each issue's cause was found before planning (read-only investigations; 01 and 05 confirmed on real
Codex in the VM). Decisions 044–046 settle the product questions; 010 and 042 are kept.

## Must fix (release blockers)
- [01](01-codex-skill-alone-not-sent.md) · A skill Otaq types on its own into Codex isn't sent (Push to PR blocked)
- [02](02-skill-turn-prompt-in-review-hidden.md) · A prompt during a delivery skill turn shows nowhere (+ NEW-6 wording)
- [03](03-answer-session-invisible.md) · An answer session can't be seen or reached
- [04](04-answer-session-ends-unanswered.md) · A question whose answer session ended unanswered waits forever
- [05](05-ctrl-c-codex-selection-lost.md) · In a Codex terminal on Windows, a Shift+drag selection is lost before Ctrl+C

## Known issues (after release)
- NEW-3 · Every "Ask the agent to write it" starts its own Noyan conversation, so decision 043 never applies.
- NOTE-3 · Dragging a start-failed card over its own Dev column says "can't go to Dev yet · It starts by itself…".
- NOTE-4 · Edit flow → Dev doesn't say "Removing waits…" while a start-failed ticket shows in Dev.
- NOTE-5 · Stopping two skill writes can leave an empty `.agents\skills`.
- With the terminal focused, the sheet's Asks-you answer box is hidden (round 5 piece 6's note).
- macOS: Option-drag + Cmd+C in Codex loses the selection the same way as 05 (decision 042 keeps macOS unchanged).
- A Ctrl+C in a Windows Terminal-tab PowerShell during `Start-Sleep` didn't interrupt it (seen once, not investigated).
  Re-test 7 (NEW-6): `ping -t` isn't stopped either (2 of 2).

### Added after re-test 7 (release-fixes f080c8f5, `../RESULTS-retest-7.md`)
- R7 NEW-1 · `otaq attach` from Windows Terminal types the terminal's colour-query replies into the session (the Esc in
  them can decline an open prompt); Ctrl+] doesn't detach. The app's Session tab reaches every session.
- R7 NEW-2 · A second question to a finished ticket that already answered one: the resumed Codex answers the old
  question's id and is refused; the question then goes to Needs you. Likely fix: put the question id in the request.
- R7 NEW-3 · Cancel on the PR dialog while the skill waits at a prompt leaves the turn running unseen (ticket reads
  Ready for you; the skill is later refused "no Open PR in progress").
- R7 NEW-5 · A here-string command is named only "$ @' …" on the card and in the PR dialog.
- R7 NEW-7 · An answer the user gives is typed into the asker as "SHOP-35 answered your question"; Codex took it as a
  question to answer.
- R7 5(a) · While its question waits on an answer session, the asking Codex ticket reads "Idle" (its turn ended after
  the stage reminder), not "asked … · waiting".
- R7 NEW-4 rest (round 6 piece 6 fixed the core case) · If the agent limit is full when a merge conflict hands the
  ticket back, it goes to Queued and a writer prompt still open stays calm until the agent's next signal.
- R7 NEW-8 · After a Push to PR dialog closed with a proposal pending, the next Push to PR shows "Push skill is
  preparing…" forever: Codex ends its turn ("Otaq still shows commit … awaiting confirmation") and the dialog never
  notices the turn ended without a proposal; only Cancel works. Same family as NEW-3.
- R7 5(b) · The question's Needs you item opens the asker's Session tab; Answer and Resume are only in History →
  Messages.

## Not tried yet
Round 5 piece 7's login expiring mid-session and resuming after sign-in (the held "signed out" + Sign in… passed in
re-test 7), check 1's Claude Code side (Claude Code isn't signed in in the VM), the real gh/az PR paths.
