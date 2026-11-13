# Re-test 9 · repro and causes of round 7's issues (Windows 11 ARM64 VM, build fae1fd78)

Date: 2026-11-12, 18:24–18:47 VM time. Build: the installed `Otaq_0.1.0_arm64-setup-fae1fd78.exe` (release-fixes
fae1fd78), already running on the normal release-test data (`otaq-reltest\home`, app PID 13936, service PID 22660);
nothing reinstalled or restarted. Issue files: `issues-round7/02-…` to `10-…` (01 is NEW-12's). Screenshots
`shots-retest-9/r9-*.jpg`.

Privacy: no screenshot shows the Windows user name (terminal paths read `~\otaq-reltest\…`), a login, email or token.
The clipboard was set once (`r9 paste check`, Set-Clipboard + Get-Clipboard read back in the same exec) for the Ctrl+V
check; nothing was pasted anywhere else. No code changed, nothing committed or pushed. The other Otaq (`~\otaq-qbox`,
PIDs 8380/18580) and the NEW-12 agent's processes and folders were not touched; I used the GUI and short CLI/DB
commands only.

## Summary
| # | Issue | Reproduced | Cause (confidence) |
|---|---|---|---|
| 02 | NEW-10 Ctrl+V pastes nothing; NEW-e1 phantom unsent text | **yes** both | raw 0x16 → `filled_by_program` sets `multi_line`; Claude's bare `›` reads `Blank` → `keys && multi_line` = unsent (repro + code). 04d9543e fixes the text-clipboard case only |
| 03 | NEW-f1 Something else + ↵ sends twice | **yes** (↵ in field: toast; Send click: none; option + ↵: none) | textarea's ↵ handler and the options div's handler both call `onSend` (guard checks `HTMLInputElement` only) (repro + code). Same guard lets digits typed in the field pick options (repro) |
| 04 | Picks/ticks reset after a tab switch | **yes** (single and several answers) | Overview unmounts on tab switch; picks are its `useState` (repro + code) |
| 05 | NEW-c1 long question, Send below fold | **yes** (~36 CSS px) | cap fixed at half the sheet body, ignoring the options' height (repro + code) |
| 06 | NEW-b2 ask in Review shown nowhere | **yes** (+ `--send-back Dev`) | not `pr_flow` (table empty); Review status isn't `LIVE`, so Asks you never sets and the question is only shown for Asks you (repro + DB + code) |
| 07 | NEW-b4 Ctrl+J opens empty Session | **yes** (NOTE-18 start failed, HOOK-4 delivery stopped) | strip target sends every non-review, non-question item to `lands_on` = Agent tab (repro + code) |
| 08 | NEW-b5 nothing-to-merge reads Merged | **yes** (sheet pill + subtitle, `otaq list`) | only the card reads `done_without_merge`; pill, subtitle and CLI word the bare Merged status (repro + DB + code) |
| 09 | NEW-c2 merge-into offers Spec review | **yes** (NOTE-26 → NOTE-20) | `can_take` accepts any `WaitingForYou` host (repro + code) |
| 10 | Switcher lists ended twice | **yes** (shop) | `open` = not closed, `recent` = not running; ended-by-restart are both (repro + DB + code) |

## Running log
- 18:24 State as fixes2 left it: hooksproj board, HOOK-13 sheet open. Wrote `otaq-reltest\r9q.js` (read-only
  node:sqlite query helper).
- 18:29 Created **HOOK-14** "R9 question box" (`otaq create -P HOOK --stage Dev=Architect`, description: test of the
  question box, reply "noted" to answers, no edits/commits/stage commands).
- 18:30 NEW-b2: `pr_flows` count 0. `OTAQ_TICKET=HOOK-13 otaq ask human --option Yes --option No` → accepted; DB row
  open, `pr_flows` still empty, HOOK-13 `state ready_for_review`; sheet unchanged (`r9-b2-review-ask-not-shown-overview.jpg`).
  `--send-back Dev` → accepted, stored open with `send_back_stage Dev`, replaced the first; `otaq show HOOK-13`
  "Ready for you (Review)", no question. Closed it in the DB (`answered_via='closed'`).
- 18:32 Created **NOTE-26** "R9 c2 merge-into host" (spec stages skipped, Dev = Architect). Started HOOK-14 and NOTE-26
  from their cards.
- 18:33 NEW-b4: notes board, Ctrl+J → NOTE-18 (start failed) on Session "No session …"; Ctrl+J → NOTE-20 on Overview;
  Ctrl+J → HOOK-4 (Delivery stopped) on Session, empty terminal.
- 18:34 NEW-b5: SHOP-39 sheet: pill "Merged", subtitle "Merged", card "Done · nothing to merge". DB and `otaq list`
  below.
- 18:35–18:38 Question box on HOOK-14 (questions by CLI): single answer reset (Beta → Changes → back → Alpha);
  NEW-f1: Something else "Go on" + ↵ → delivered + "The answer to HOOK-14 wasn't sent · HOOK-14 isn't waiting for an
  answer"; Something else + typed `V2` → "2" picked Beta and closed the field; Something else "Go on" + Send answer
  click → no toast (after closing the first toast); Beta + ↵ → no toast. Several answers (`r8c-q-multi.txt`): ticked 2,3 →
  Session → back → 1,3; sent.
- 18:39 NEW-c1: `r8c-q-long.txt` + three noted options on HOOK-14 → Send below the fold; one notch → in view. Sent option 3.
- 18:39–18:41 NEW-10/e1: clipboard `r9 paste check` (read back) → HOOK-14 terminal, bare `›` → Ctrl+V → nothing.
  New question → Send answer → "HOOK-14 has unsent text in its terminal" → Clear it and send. Claude then showed its dim
  suggestion; Ctrl+V again → next question's Send answer went through with no dialog (Placeholder read).
  (An accidental click opened HOOK-14's description editor; Esc discarded it and closed the sheet; `otaq show` confirmed
  the description unchanged.)
- 18:42 Switcher (shop, Noyan panel ▾): ended-by-restart conversations listed in the top list and RECENTLY ENDED.
- 18:42–18:44 NOTE-26: Validate's Codex Checker asked to run three commands (`git diff HEAD^ HEAD -- README.md`,
  `git rev-parse --short HEAD`, `otaq check pass`), answered "y" each; passed → Code review. Approve → chooser →
  Merge into another ticket… → picker lists NOTE-20 (Spec review). Cancelled both.
- 18:46 HOOK-14: "1 message waiting · Deliver now" (Otaq's stage reminder held: the bare `›` was back after the second
  Ctrl+V) → Deliver now → the unsent-text dialog again → Clear it and send → delivered; HOOK-14 Idle.

## DB and CLI evidence
- `pr_flows`: empty before and after the Review asks.
- `human_questions` (HOOK-13): `R9 b2: asked from Review…` answered_via `replaced`; `R9 b2: send it back to Dev? (test)`
  `send_back_stage Dev`, open until closed by hand. No open questions left anywhere.
- `tickets`: SHOP-39/43/44, NOTE-24 `state merged, outcome done_without_merge, nothing_to_merge` set; HOOK-12 `merged,
  done_without_merge`, `nothing_to_merge NULL`. `otaq list`: all "Done Merged". NOTE-18 `start_interrupted`
  (`start_problem_step worktree`); HOOK-4 `ready_for_review`, `auto_merge_stop push_not_landed`; NOTE-20
  `ready_for_review`, `stage spec-review`, branch `otaq/NOTE-20`.
- `project_agents` (shop): doubled rows are `ended_by_restart` with `closed_at NULL` (Test step, no work; Add Security
  stage; Suggest shop flow; Whole dollar README; …); `ended` ones (Add Docs stage, Security review stage) are closed.
- Service log: one `needs-you answer ticket=HOOK-14 delivery=Sent` per answer (the refused second request isn't logged).

## Not done
- 04d9543e not built or tried in the VM; its effect on NEW-e1 is from its diff.
- NEW-f1 in Noyan's panel / Edit flow's box (same component; expected the same).
- The stale-question effect of NEW-b2 after a Send back (code only).

## Setup changes and state left
| Change | State now |
|---|---|
| HOOK-14 "R9 question box" (hooksproj, Dev = Architect) | **Dev · Idle**, its Claude session live; no files changed, no commits; eight test questions asked and answered |
| NOTE-26 "R9 c2 merge-into host" (notes, specs skipped) | **Code review · Ready for you**; branch `otaq/NOTE-26` with one commit (7882b8b, README line "R9 merge-into host."); not merged; Checker passed |
| HOOK-13 | unchanged (Review, Ready for you); the two Review test questions are replaced / closed (`answered_via='closed'`) |
| Settings | none changed |
| Clipboard | `r9 paste check` |
| `otaq-reltest\r9q.js` | left (read-only query helper, no secrets) |
| The app | **running** fae1fd78 on the release-test data (app 13936, service 22660), hooksproj board on screen |
| `~\otaq-qbox` Otaq, NEW-12 agent's work | untouched |
