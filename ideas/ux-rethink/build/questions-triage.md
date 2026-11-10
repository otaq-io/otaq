# Build questions, checked against the decisions (2026-11-09)

Every question in questions.md was checked against DECISIONS.md (UX-1…61), ASSUMPTIONS.md (A1…53), the other `ideas/`
decision folders and main's code (dcc4fef0). Counts: settled 7 topics · answered 16 · build contradicts 13 · not
product 21 · open 56.

## Settled with the maintainer (2026-11-09)

- Done stays a "Done N →" link on the board (UX-12 only fixes the ends in Edit flow).
- Git name and email are asked at the first start and at merge; optional on Tools (no decision; new).
- The status word is "Ready for you" (A11).
- Noyan writes tickets straight to Backlog, no proposal card (UX-22).
- Dead code goes: the permission-warning banner, first_flow / flow_setup_since / OnboardingUseDefault, the sheet's
  unused details / choices / path / token_split, shell.cleanup / shell.open_folder.
- The README video is re-recorded in the Windows VM.
- Ready-stage skills (Security review, Test in the browser, Update the docs): **UX-61 stands.** Otaq ships a skill only
  when a shipped flow uses it; none does, so these use skills you add and the palette shows the usual missing-skill
  warning (the follow-up's quiet line is reverted).

## Answered by a decision (build matches unless noted)

| Question | Decision |
|---|---|
| A hint names its own button | A16 |
| "Waiting for it" read-only | UX-39 |
| Noyan's agent offered for ticket stages | UX-3, O1 |
| Noyan's agent can't be deleted | UX-9, A31 |
| "Allow 1 more round" adds one | UX-60 |
| Message rewritten after a send back; one squashed message | UX-56 |
| Send back to any earlier stage | UX-55 |
| Limit banner "% left" | UX-34 |
| Limits/setup failures aren't start refusals | UX-15, UX-12, no-fallback |
| Running popover rows | A37 (built) |
| "Merge into another ticket…" in the Approve menu | UX-13 |
| Plugin skills drop "Local merge" | UX-13 (done) |
| Eight settings topics | UX-57 |
| Merge check stays in Repository | UX-10 (moving it reverses UX-10) |
| Edit flow's Done panel shows On Approve / Automatically like project settings | UX-25 — **build differs under Either** (flow_board.rs:674): make them match |

## The build contradicts a decision (fix)

1. List puts Backlog after the stages — A40 flow order, UX-12 (list_view.rs:210).
2. Strip's Review chip opens Changes — A20 "In Review it opens on Overview" (needs_you_strip.rs:63).
3. "First approve" recorded only at Merged — UX-20/35/61 key on the approval (review/finish.rs:69); also on Approve &
   open PR.
4. An idle Noyan conversation shows Pause — UX-19 (agents/pause.rs:474): count it only while replying.
5. An agent stage with no skill keeps Save off — no-skill-copies S5 "Otaq's own skill for that stage runs"
   (flow/model.rs:234, flow/check.rs:36, flow_editor.rs:596).
6. "Set the Delivery stage's skill" link goes nowhere; Done panel has no agent/skill — F15, UX-25/56 (api/pr.rs:286,
   pr_page.rs:377, flow_board.rs:181).
7. "Local merge" / "Open PR" on the To deliver card and in toasts — UX-13, A17 (card.rs:2788, approve.rs:389/504,
   review_page.rs:339, api/review.rs:264).
8. `prime --project` always gives Noyan's setting agent's instructions — UX-3/UX-9 (project_agents/prime.rs:82).
9. Stage descriptions end with evidence ("From 41 … runs") — UX-53/59 (plugin/skills/flow/SKILL.md): move evidence to
   "Why these changes".
10. Files says "worktree" for a ticket's copy — UX-30, UX-6 (files_view.rs:667, :1938).
11. Automatic PR delivery with no provider only logs — UX-13 "say what to do", UX-57 (review/delivery.rs:93).
12. List drops the board's lift of needs-you cards — UX-33 says same order and answering never moves a row; **which
    side changes is open** (see Open).
13. "New agent on…" for a conversation whose tool can't run — G16 (no-fallback-agents) offers agents on a tool that
    can run; the orchestrator note said the conversation waits. **Open** (see Open).

## Not product (engineering answer)

Node 24 vs 26 (match .nvmrc) · external_id ignored (fine) · gallery hint never stored · no Reset hints · keep os_dark out
of whole-settings saves · stage templates in otaq-core/flows/stage-templates.yaml · "2nd in line" counts the queue the
limit uses, across projects · 50-row search list fine · clear a stuck "writing" message when its session ends · a
dedicated kind for the writing request · drop FirstComment.report at the next protocol bump · bump the protocol for
first_approve · check the "Review — · round 1" Path glitch · count any delivery session at work · keep users' custom
keys · shot.sh raises the window with osascript · column edge dark-only if noisy · Esc closes the topmost open thing
regardless of focus · identity check before any session (resumes too) · never reuse an existing otaq/<KEY> branch.

## Open (no decision covers it), by user impact, with the recommendation

1. Apply now on a reorder/kind change: agents running in affected stages — **decided 2026-11-09: keep them running**
   (UX-60).
2. "Validator" → "Checker" on existing installs — migrate the stored agent and flows (UX-2).
3. Send-back reason posted as the user's comment — post it as the agent (UX-55).
4. Idle stays in Needs you, turquoise — keep (UX-21).
5. A waiting card sits in the first stage even if it skips it — the first stage it will run (UX-12).
6. Automatic local delivery uses the plain message — keep (UX-56).
7. PR-open card on flows with no Review — the flow's last stage (UX-13/23).
8. Click-mode "To deliver" notifies — yes, under "Ready for your review" (UX-28).
9. Descriptions for existing projects' stored flows — fill stages matching an unchanged shipped preset (UX-53).
10. "Stop here" reads like undecided — takes the ticket out of Needs you (UX-60).
11. Sign in… on a signed-out restart row — bring it back (A49, UX-37).
12. Held-for-identity card's Retry — "Set name and email…".
13. Old "Allow notifications?" banner — ask the OS on any first start, drop the banner (UX-18).
14. "Answer here" leaves no History row — record it (UX-55).
15. ⌘↵ in the full form and review bar — the primary button everywhere (UX-22).
16. Individual notification kinds unreachable — keep the groups (UX-28, A33).
17. Repo-local git identity — global only.
18. "Same as…" with several projects — a picker, latest first (UX-61).
19. Card hints on the List — point at rows too (UX-14).
20. "Rework" on a PR-open card — no; Send back with PR comments covers it.
21. "Ask Noyan about this" — keep starting a new conversation (UX-47).
22. History names the agent on agent rows — record it (A24).
23. Template agents — the nearest same-kind stage's agent.
24. "Write a plan" with an empty Plan — "Write a plan…" in ⋯ (N17).
25. "Otaq can see" chips in Settings → Agents — drop (UX-14).
26. Skip + another agent on a card — one combined line.
27. Edit ticket's layout — same as the new-ticket form (A46).
28. Theme choice on upgrade — keep the user's choice.
29. Clicking the other theme kind switches the mode — as built.
30. Grouped notification wording — "N tickets need you".
31. Human-check notification title — "SHOP-1 is ready for you · <stage>".
32. Prepared title starts with the ticket key — keep.
33. ⇧⌘. resumes a paused project — yes, a toggle.
34. Restart dialog count — all stopped rows in the title, ticked ones on the button (A49).
35. "Restart to update · stops N agents" — keep.
36. Settings view after restart — come back to the Board (UX-7).
37. Changing Delivery with an Edit flow draft open — keep the refusal.
38. Project key on Settings — a read-only line in Repository.
39. Box after ↵ — stay open, cleared.
40. Esc in the box keeps the draft — keep.
41. ⌥↵ new line in the box — keep (UX-22 takes ⇧↵).
42. List rows' waiting mark — match the card's hourglass (A6).
43. Files picker ticket rows — keep until the data exists.
44. Files source bar — "Showing main" (A36).
45. Palette recent items — keep under the suggestions.
46. "Pause <project name>" in the palette — yes.
47. "Not while you're looking" — a fixed sentence.
48. Keyboard list names — the palette's names.
49. Done order — newest first.
50. "Not started" on Backlog cards — keep.
51. Unknown in:/by: word — "No stage named …".
52. ↵ in search keeps the filter — keep; Esc clears.
53. Multi-word names in search — accept dashes and quotes.
54. Undo history lost on close — as built.
55. Full form's primary — already Create & start; keep.
56. Template skill name security-check — goes with the ready-stage skills choice.
57. List lift of needs-you cards (from contradiction 12) — drop the board's lift so both keep one order (UX-33).
58. "New agent on…" when a conversation's tool can't run (from contradiction 13) — G16 vs the "conversation waits"
    note.

## Work in progress (2026-11-10)

Two agents stopped before reporting; their branches are pushed, unverified (check.sh not confirmed):
- `ux-fix-contradictions` (was worktree-agent-a720397fa81ec87b0): contradictions 1–11, the Done panel under Either,
  and UX-61's missing-skill warning back in the palette. One commit, looks complete.
- `ux-cleanup` (was worktree-agent-abe3cc9ea2f488715): the approved dead-code removal (PROTOCOL_VERSION 12) and the
  "Not product" items. 13 commits; left: shell.cleanup still referenced in 2 places, and whether the tools' own
  `permission_warning` is still needed. Then check.sh, review, merge.
Open questions resume at Open 2 ("Validator" → "Checker" on existing installs).
