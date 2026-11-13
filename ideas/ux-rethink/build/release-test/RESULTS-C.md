# Phase C · Ticket · results (Windows 11 ARM64 VM, build ux-rethink-fixes ef84b175)

Screenshots: vmpal saves them under /Users/nevzattopcu/.claude/projects/-Users-nevzattopcu-Desktop-Workspace-otaq/de3a40a2-9ec6-4414-af3c-524ad89a007e/tool-results/
All agents ran on Codex (Claude Code is not logged in in the VM); every Codex command needed a `y` in the Session tab.

## Sheet header and tabs (UX-4, UX-45, A18–A24)
- PASS · A19 · header: crumb "shop / SHOP-6", title, state pill ("Ready for you", "Needs you · checker", "Checking · round 1 of 3", "Merged"), Move…, ⋯, full window, close; meta "Dev and Validate by Codex · 8 min · started by you" / "In Dev · Checker · started by you 14 min ago"; stepper of the flow's stages (skipped stages greyed with a skip icon on Spec first).
- PASS · UX-4 / A20 · tabs Overview · Session · Changes "1 file" · History. In Review it opens on Overview: WHAT THE AGENT DID, CHECKER ("Check passed · on the first try", Full report), THE TICKET, Dependencies, Comments; the review bar (Send back · to Dev… / "1 file · +1 −1" / Approve & merge into main ▾) stays on Overview, Session and Changes.
- PASS · A23 / UX-5 · Session switch "Agent · Validate · Terminal" (the check part named after the stage; only "Agent · Terminal" before Validate ran; no Run as the project has no run command), "Checker · Codex · tool default", "Click the terminal to type to Checker · SHOP-5", copyable `otaq attach SHOP-5 --stage validate`, "Read-only · this checker has ended" after the check.
- PASS · Changes · "SHOP-5's copy · otaq/SHOP-5 · from main", file list (All files / Changed · 3, A/M marks), "3 files · +3 −2 against main · Validate passed · round 1", Agent summary and Validation report folded on top, Unified/Split, Open file, per-file diffs.
- NOT TESTED · ticket-03 · 2 · line comments in Changes (hovered a line, didn't post one).
- PASS · UX-45 / A24 · History: filters All / Questions / Checks / Sessions, "newest first", plain rows ("Merged into main · you · 13:36", "Finished Dev …", "Checker · Validate · round 1 commented ›", "Round 1 · passed · 0 min · 10k tokens"), and one quiet row per agent session "Session · Validate · Checker 01a1c4fd-… ⧉ · 13:24–13:25 · 10k tokens"; its copy icon put the full id 01a1c4fd-1927-77f3-a1b8-506441ae4cc8 on the clipboard. Per-tool rows "Developer: Developer · Codex · gpt-6-luna · 40k". A "Messages" thread block sits above the filters when the ticket asked something.

## Questions on Overview (A21, UX-55, UX-32)
- PASS · A21 / UX-55 · the agent's send-back question is the first thing on Overview: question text, "1 Send back to Product spec", "2 Answer here… type your answer", "1–2 pick · ↵ sends", Answer in terminal, Send answer; card "Asks you", strip "NOTE-2 a question".
- NOTE · ticket-07 · 2 · the Send back option has no explanation line (canvas: "The Spec writer settles the refund rules; … The skip is removed.").
- NOT TESTED · A21 · a plain multiple-choice question with Recommended preselected (no agent asked one).
- NOT TESTED · gap #14 · "Answer here" path of a send-back question (Send back was used).
- FAIL · A22 / ticket-02 · 2 · a permission prompt shows only "Answer in terminal" on Overview; the command itself (and "Codex is waiting for your answer in its terminal…") is not shown, though the card shows it. Screenshot mcp-vmpal-blob-1794307887376-3yu8ma.jpg.

## Comment vs Tell <agent> now (UX-43, UX-58)
- PASS · UX-58 · comment box: Attach · "Tell Checker now" (Ctrl+↵) · Comment, helper "Comment: the next stage reads it. Tell Checker now: Checker also gets it while it works." The second button names the running agent (Checker as SHOP-5's own Dev agent, Developer, Spec writer) and is gone on a merged ticket (only Comment).
- PASS · UX-58 · a told comment is tagged "You 1:38 pm · told Checker now"; agents got "The user added comment 3. Run 'otaq prime' to read it." once delivered.
- FAIL · UX-58 / UX-43 · with Codex a told comment never reaches the agent when it is idle: the session footer says "Unsent text in the input line · Otaq's note waits until it's empty" while Codex's input holds only its placeholder ("Ask Codex to do anything"), and "N messages waiting · Deliver now" asks "SHOP-5 has unsent text in its terminal" every time (also on Send back). Each delivery needed Deliver now → Clear it and send. Repeated on SHOP-5, NOTE-1, NOTE-2 (phase A's NOTE confirmed). Screenshots mcp-vmpal-blob-1794307753299-9mgi25.jpg, mcp-vmpal-blob-1794308184134-lb4wz2.jpg.

## Attachments on comments (tuq/decisions 001–008)
- PASS · 001/003 · Attach opens the Windows picker; picking 6 files at once adds a chip each (images with thumbnails, notes.txt, sample.zip, exact-20mb.bin "20 MB").
- PASS · 005 · big-21mb.bin refused when added: "big-21mb.bin is 21 MB; the limit is 20 MB. Keep the part that matters, …"; the other chips stay. Exactly 20 MB accepted.
- PASS · 003 · Ctrl+V of a clipboard image adds "Pasted image.png 2 KB" with a thumbnail.
- PASS · 007 · a comment with attachments and no text posts ("You commented · with 5 attachments (4 KB)").
- PASS · 004 / 002 · posted images show as a row of thumbnails; clicking one opens it full size in the app ("shot-blue.png · 1 KB", Close); text and zip are chips with name/size and Open; the right-hand ATTACHMENTS list shows all.
- PASS · 008 · editing the posted comment shows its chips removable plus Attach; removing sample.zip and Save → "You 1:37 pm · edited", History "You edited your comment › Removed sample.zip".
- PASS · 006 / 002 · agents read them: the checker ran `otaq report SHOP-5 2 --attachment 1` (printed notes.txt), the Developer "Viewed image Pasted image.png".
- NOT TESTED · 003 · drag and drop from Explorer; adding a new file while editing a posted comment (Attach is there).

## Edit ticket and the ⋯ menu (A46, UX-11, UX-46)
- PASS · A46 / ticket-06 · 2 · Edit ticket… turns Overview into the form in place: "NOTE-2's agent reads your edit the next time it's idle.", Title, Priority P1–P4, Stages for this ticket (started/skipped stages read-only with a lock, Validate still selectable), What and why, "Start from and merge into · main · locked · the ticket has started", Ctrl+↵ save / esc discard, Cancel / Save. P2 saved → card shows P2.
- NOTE · ticket-06 · 2 · Waits for isn't in the edit form (it stays the separate "+ Waits for…" under it); canvas draws it in the form.
- PASS · A46 · ⋯ menu: Edit ticket…, Ask Noyan about this, Pin to top (→ Unpin), Copy branch name (clipboard "otaq/SHOP-6"), Move to next stage… (disabled "agent is working" while it works; when idle a dialog "The agent ended its turn 11s ago … Note for the review summary … Move to next stage"), Close ticket…, Delete… disabled "Started tickets can only be closed."
- PASS · Close ticket… on a running ticket: "Close NOTE-3 … without merging? · Its agent is running. Closing stops it; the session is not kept. · The branch is kept…" Stop agent & close → Done · not merged.
- FAIL · ticket-06 · 1 · the ⋯ menu has no "Merge into another ticket…" (drawn there; it exists only in the Approve menu). Screenshot mcp-vmpal-blob-1794306924362-ta5zc8.jpg.
- NOTE · the header's Move… lists every stage with what a drop would do and why it's refused ("Only waiting, working and closed tickets go back to Backlog", "NOTE-2 can't move to the next stage now: its agent is in the middle of a turn") — not drawn, stays as today.

## Send back (A24)
- PASS · ticket-03 · 3 · "Send back to Dev · SHOP-5 goes straight back to its agent in Dev (Working) and continues in the same session. THE AGENT WILL RECEIVE · No line comments. · Comment for Dev (optional) · Attach · Posted as a comment for Dev · every later stage reads it · Then Validate runs again before it comes back to you." The same session got "Review sent SHOP-5 back to you with a comment."

## Agent asks to send back to an earlier stage (UX-55, UX-32, UX-31, UX-54)
Setup: notes switched to Spec first (Edit flow → Start from… → Spec first, applied at once, Validate rounds set to 1); NOTE-2 created with Product spec, Tech spec, Milestone plan and Spec review skipped; the Dev agent told (Tell Developer now) to run `otaq ask human --send-back "Product spec" --stdin`.
- PASS · UX-31 · the full form offers "Skip this stage" only on the four skippable stages ("notes's flow lets tickets skip Product spec, Tech spec, Milestone plan and Spec review"); Dev/Validate offer agents only. Card: "⏭ Skips Product spec, Tech spec, Mi…".
- PASS · UX-55 / UX-32 · the agent's ask became the turquoise question above; Send back moved NOTE-2 to Product spec, the card's skip line became "Skips Tech spec, Mil…", History: "Sent back to Product spec · Dev asked: The usage wording is a product question… · you sent it back · no longer skipped", plus a Messages thread (asked / answered by you).
- FAIL · UX-55 / gap #3 · the agent's reason is also posted as the user's comment ("You 2:16 pm · Dev asked to send NOTE-2 back to Product spec: …"); the decision says the reason is the agent's, in History. Screenshot mcp-vmpal-blob-1794309382415-x2uqwb.jpg.
- NOTE (environment) · Product spec runs on the Spec writer's tool, Claude Code (not logged in), so it went "Idle" at once; Move to next stage… took it on to Dev (skipped stages passed). The Dev agent then ended with `otaq stage done` ("README already has the test command"): NOTE-2 is "Done · nothing to merge" (approve-options A2b PASS).

## Checker fail, rounds and Decide (UX-60, one-check-agent)
- PASS · UX-60 · with rounds 1, a failed check stops in Validate: card "Validate failed 1 times · decide" + Decide, strip "NOTE-3 decide", pill "Check failed 1 time", Overview "Give it another round" / "Answer the agent…". Decide: "VALIDATE FAILED 1 TIME IN A ROW · Allow 1 more round (Back to Dev with Checker's notes, once more.) · Send it on to Code review (You'll see Checker's last report there.) · Stop here (It waits in Validate until you choose.) · Rounds for Validate are set in Edit flow (1)."
- PASS · Stop here leaves it waiting; Allow 1 more round → Dev "Fixing · sent back by Validate".
- FAIL · UX-60 vs one-check-agent O5 · "Send it on to Code review" is offered, then refused with two toasts "NOTE-3 can't be moved on · Validate can't be skipped: the flow doesn't mark it skippable". Either the option shouldn't show on a non-skippable check, or it should work. Screenshot mcp-vmpal-blob-1794309914155-59a0tl.jpg.
- FAIL · UX-60 / flow-03 · 5 · wording "Validate failed 1 times · decide" (plural with 1). Screenshot mcp-vmpal-blob-1794309901824-uwnbnd.jpg.
- PASS · a check that ended without a verdict: "Validate gave no result · decide" + Retry validation (reran it).

## Approve & merge, prepared message, merge check (UX-42, UX-56)
- PASS · UX-56 · Approve & merge opens at once: "Merge SHOP-6 into main · Squashed into one commit… · Runs node test.js first when main has moved. · Commit message (prefilled) · Developer is still writing the message. Merge now with this one, edit it, or wait… · The worktree for SHOP-6 is removed unless it has uncommitted changes · esc cancel · Ctrl+↵ merge". Merged SHOP-6 (8da7275), SHOP-7 (Ctrl+↵, e91c0ee), SHOP-5 (bb0c70b); progress dialog "Fetch main… / Rebase onto main / Squash into main / Start waiting tickets…"; toast "SHOP-6 merged into main · Squashed as 8da7275".
- FAIL · UX-56 · the prepared message never arrived in 5 Approve dialogs (always "Developer is still writing…"). For SHOP-5 the message writer was a separate Codex session ("SHOP-5 is waiting in Review. You are its delivery's agent…") whose permission prompts are shown nowhere; it was still running after the merge: Running "2 running · SHOP-5 Fix the word tiny in README · Developer · 17m" while SHOP-5 is Done/Merged, its Done card shows a CX agent mark, and %OTAQ_HOME%\wt\shop\SHOP-5 is left as a non-git folder. Screenshots mcp-vmpal-blob-1794309964992-yg6i6i.jpg, mcp-vmpal-blob-1794310150277-pzbun7.jpg.
- PASS · UX-56 / UX-24 · merge check: with project Settings → Repository → Merge check set (notes: a script failing on a flag file), Approve & merge on NOTE-1 after main moved ran it first, merged nothing, sent NOTE-1 back to Dev by itself: card "Merge check failed · fixing", Overview "Merge check failed on main · Merge check '…mc.cmd' failed on the result rebased onto main (exit 1). Fix it, then submit again. · Nothing was merged into main. NOTE-1 went back to its agent…", log attached as NOTE-1.log. After the flag was removed and the agent resubmitted, it returned via Validate to Review.
- FAIL · ticket-05b · the sheet pill for a failed merge check says "Merge conflict · fixing" (the card says "Merge check failed · fixing"). Screenshot mcp-vmpal-blob-1794307669957-j55cge.jpg.
- FAIL (phase A NOTE confirmed) · A20 · WHAT THE AGENT DID keeps the first round's handoff after a send-back/restack (SHOP-5 Review shows "…at cc02715" while the checker reports b2070c4/aadc6f1). Screenshot mcp-vmpal-blob-1794308594256-pf1om2.jpg.
- NOTE · the plain message is built from the last handoff, so after my "no change needed, submit again" send-back the default commit body read "No change needed; resubmitting the existing README change…".

## Approve ▾ other ways to finish (UX-13, approve-options)
- PASS · menu: Approve & merge into main (Squash-merges SHOP-5 into main.) · Approve & open PR (disabled "PR provider not detected" without origin; with a GitHub origin "Pushes the branch and opens a pull request on GitHub instead.") · "Your flow delivers either way. Change it at the Done end of Edit flow." · Merge into another ticket… (disabled "No other ticket of this base waits in review, to be delivered or with a PR open") · Done, nothing to merge (disabled "SHOP-5 has commits: Done, don't merge keeps them out of main.") · Done, don't merge.
- PASS · approve-options A2 · Done, don't merge: "NOTE-1's 2 commits won't be merged; its branch is kept · Nothing lands on main. NOTE-1 finishes as Done and counts as merged…" → toast "Done · NOTE-1 · Not merged; it counts as merged and its branch keeps the commits"; Done list "Done · not merged".
- NOT TESTED · Done, nothing to merge from the menu (enabled case) and Merge into another ticket… (enabled case); the agent's stage done gave "Done · nothing to merge" (NOTE-2).

## PR delivery without gh (UX-57, UX-25)
Setup: a fake origin https://github.com/example-org/notes.git added to notes (removed again afterwards); nothing pushed.
- PASS · UX-25 · Repository shows Delivery "Merge into main / Pull request / Either · On Approve / Automatically" (notes was already Either) with "This is the flow's setting: changing it here changes the flow, and the flow file."
- PASS · UX-57 · Repository: "Pull requests through · GitHub · example-org/notes · gh isn't installed · Set up…" (without origin: "None detected · This repository has no origin remote").
- PASS · UX-57 · Approve & open PR dialog: "Open a pull request for NOTE-1 · GitHub isn't set up on this computer · notes delivers pull requests through GitHub. Add the GitHub provider and sign in once; every project can use it. · Set up GitHub…", Branch otaq/NOTE-1 ("Valid name · not taken on origin"), Title, Description (prefilled, "Developer is still writing the title and description…"), "main · on GitHub · example-org/notes", "Or merge into main instead", Cancel, Push & open PR. Set up GitHub… opens Settings → Git & pull requests (git row, GitHub "uses gh · gh isn't installed · winget install GitHub.cli · Install", Azure DevOps likewise). "Or merge into main instead" switched to the merge dialog.
- NOT TESTED · that Push & open PR is disabled (not clicked; no push attempted).
- KNOWN · gap #32 · the PR title starts with the key ("NOTE-1 Fix the typo libary in README"); canvas has no key.

## Conflicts going back on their own (UX-24, UX-44)
- PASS · UX-44 · SHOP-5 waited in Review while SHOP-6 (same README line) was merged; when SHOP-7 merged, the test merge found the conflict and SHOP-5 went back to Dev by itself: card "Conflicts with main · fixing", Overview "⚠ Conflicts with main · 1 file (README.md)" + Open agent session; the agent ran the restack skill, resolved, and it came back through Validate to Review.
- PASS · UX-24 · a merge-time conflict (main moved again with a conflicting test commit): "Rebase onto main stopped — conflict · main moved since SHOP-5 was built… It was aborted; main and your own checkout are unchanged. · README.md · SHOP-5 went back to its agent, which rebases onto main… Or resolve it yourself in the worktree." (Close); it went back without a click, was resolved and then merged (bb0c70b, README "A small shop library for unit tests. Requires Node 24…").
- FAIL · ticket-05b · 2 · the Overview banner only says "Conflicts with main · 1 file (README.md)": no "what moved (SHOP-6 changed README.md)", no "Nothing was merged … returns to Review when done", no "1 of 3" count as drawn. Screenshot mcp-vmpal-blob-1794308173405-x81wue.jpg.
- NOTE · after the conflict the meta line said "In Dev · Developer" although SHOP-5's own Dev agent is Checker (the session header says Checker).
- NOT TESTED · UX-24 third conflict in a row ("Keeps conflicting with main", red, waits) — too costly.

## Plan & helpers (A43)
- NOT TESTED · no ticket made a plan.

## Counts
PASS 33 · FAIL 10 · KNOWN 1 · NOT TESTED 8 · NOTE 6

## State left for phase D
- Otaq running on screen, maximized, on shop's board. "1 running" is the orphaned SHOP-5 delivery-message Codex session (see FAIL above; left on purpose as evidence, it does nothing).
- shop (Otaq default flow, merge check now `node test.js`): main = bb0c70b SHOP-5 · 270f9ec (test setup "Say unit tests in README") · e91c0ee SHOP-7 · 4fa97d6 (test setup CHANGELOG.md) · 8da7275 SHOP-6 · e77aea2 … All tickets Done: SHOP-1 not merged, SHOP-2…7 merged. Backlog/Dev/Validate/Review empty.
- notes: flow switched to **Spec first** (Product spec → Tech spec → Milestone plan → Spec review → Dev → Validate (rounds **1**) → Code review; delivery Either, on Approve). Merge check `node test.js`. No origin remote (added then removed). main = 84406d5 (test setup CHANGELOG.md) on a4cc069. NOTE-1 Done · not merged, NOTE-2 Done · nothing to merge, NOTE-3 Done · not merged (closed). Nothing in progress.
- Settings changed: Validate rounds on notes = 1; merge checks on both projects. Agents unchanged (Developer/Checker/Noyan on Codex; Spec writer/Architect/Milestone planner still on Claude Code, which can't run here).
- Files in the VM: %USERPROFILE%\otaq-reltest\attach\ (test attachments), %USERPROFILE%\otaq-reltest\mc.cmd (unused now); failcheck.flag removed.

## Extra check (lead, after phase D): user report "Send back with only a pasted text can't be sent"
- PASS · tuq 007 · SHOP-10 in Review → Send back to Dev → pasted 5,509 characters into the empty comment: it became a
  "Pasted text · 5 KB" chip with the box empty; "Send back to Dev" stayed on; sending moved SHOP-10 back to Dev and
  posted comment 3 "Report: Pasted text · 5 KB, 60 lines"; `otaq report SHOP-10 3` prints all 60 lines. (The user's
  report matches v0.1.0, whose `needsLine` rule refused a pasted chip with no typed line.)
- NOTE · the Send back comment is now "(optional)": the button is on even with nothing in the box.
