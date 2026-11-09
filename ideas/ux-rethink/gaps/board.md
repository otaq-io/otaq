# Board slice: design vs app (board-01 … board-05)

Paths are relative to the repo root. `card.rs` = `crates/otaq-view/src/view_models/card.rs`; `ui/` = `crates/otaq-desktop/ui/src/`.

## Summary: the 5 biggest gaps, by size of work

1. **In-place flow editor on the board, with a much smaller stage model** (board-04). **L.** Today it's a 3-part overlay (`ui/screens/flow_editor/FlowEditor.tsx`, 1694 lines; `flow_editor.rs`, 5527 lines). The design drops skill per stage, `column: with-previous`, stage options (other agents or skip), editable rounds, several check agents, the Delivery stage, "Made for <project>" proposals, Import, the Project agent column and the animation. It adds one new field: per-stage **Instructions**.
2. **What a flow change does to tickets** (board-04 f2/f5). **L.** The design says "New tickets use it already" and "Tickets in it move to the stage before". Today a removal or reorder makes new tickets wait at the span's edge until it is quiet (`crates/otaq-core/src/flow/apply.rs:14-22`). Apply now's default pick is the *next* stage (`apply_now.rs:12-14`). Delivering the design means per-ticket flow versions, a different default, and a decision on whether Apply now goes.
3. **Live "what the agent is doing" on every card** ("writing tests", "editing 4 files", "running the tests"). **M–L.** `ToolEvent` has no tool-use or activity event (`crates/otaq-core/src/agent_tools/events.rs:17-96`). A line exists only for Otaq's own planning skills (`planning.rs:81`). This needs a new event in all four adapters, plus a summariser and throttling.
4. **First-run path** (board-01). **M.** It needs four things:
   - the Flow onboarding step removed;
   - a starter ticket card in Backlog ("Create & start / Not now", time and cost line);
   - three one-time coach hints (first question, first send-back, first review) with stored "seen" flags;
   - "Pause"/"N running" shown only while something runs.

   None of the starter ticket or the hints exists today.
5. **Board shell restructure** (board-01/02/05). **M**, mostly UI and view models:
   - the Ready for Dev column goes; waiting cards sit at the top of the first stage, with a place in line;
   - Backlog cards get Start;
   - edges are recoloured: red for "broke and waits";
   - the strip is hidden when empty and regrouped;
   - Board/Files/Settings/New ticket move into the rail, and List becomes a layout switch;
   - New ticket and the Ask bar merge into one "New ticket, or ask Noyan…" bar;
   - "Edit flow" is a slot after the last column.

Smaller items:
- per-stage instructions: **S–M**, C;
- dependencies as one "Waits for" direction, a hover line and a toast: **S–M**;
- "found N problems" needs a count in the verdict: **S**, C;
- Spec first preset content differs: **S**.

---

## board-01 · A new user's first ticket

| # | Design | App today | Class |
|---|---|---|---|
| 1 | Board shows right after Project, on the default flow. | Onboarding has a 5th step, **Flow** (FlowEditorPage, `onboarding.rs:18-25`, `ui/screens/onboarding/Onboarding.tsx:1-8`). | **D**, deliberate per the caption ("Right after Project"). The flow-proposal-at-setup feature (README "have an agent propose one") loses its entry point. |
| 1 | Column subtitles: "Not started", "An agent writes the code", "A checker tests it", "You review and approve", "Merged". | Only Ready for Dev has a hint ("Starts when ready…", `board_page.rs:313`). Who runs a column shows on hovering its title (`ui/screens/board/Column.tsx:391-400`). | **A** |
| 1 | No **Ready for Dev** column: Backlog → Dev → Validate → Review → Done. | Ready for Dev is a fixed column (`flow_columns.rs:24-26,40-41`). | **B** (see board-02) |
| 1 | "Edit flow" sits at the end of the columns. | Opened from a column header's ⋯ menu (`Column.tsx:381`), the project menu "Change flow…" (`ui/screens/shell/projectMenu.ts:11`) and Settings (`ui/screens/settings/flowActions.ts`). | **B** |
| 1 | A prefilled first ticket ("Tidy the README"), editable, with "Takes about 5 minutes and uses your Claude Code plan", **Create & start / Not now**. | Empty board with a New ticket button only (`empty_board.rs:31-39`, `ui/screens/board/EmptyBoard.tsx`). | **C** (S–M): a starter-ticket template, a "shown/dismissed" flag, and the tool name for the plan line. The time/cost estimate has no source; it would be fixed copy. |
| 1 | No strip, no Pause, no Ask bar until something runs. | The strip always renders, with "Needs you · 0 here" (`needs_you_strip.rs:421-423`). Pause is always in the title bar (`ui/screens/pause/Pause.tsx`). The Ask bar floats over the board (`ui/screens/ask_bar/AskBar.tsx`). | **B** |
| 2 | Create & start moves the card to Dev and says "Working · reading the README", with the tool name "Claude Code" and a time. | The start sequence exists. There is no live activity text (see summary item 3). The card names nobody while there is only one agent or tool (`card_page.rs:40-43`, d0086). | **C** for the activity text; **A** for showing the tool. |
| 2 | "1 running" and Pause appear in the title bar. | Header shows Activity (agents and processes across projects, ⇧⌘U popover; `ui/screens/shell/Header.tsx:1-10`). | **B**: Activity becomes a count. The popover's content (processes, usage warning) isn't shown in the design: **D**? |
| 2 | OS notification prompt plus Otaq's one-line "Turn on notifications · OK". | The ask is set pending at the first session start; Otaq shows its own line before the OS request (`crates/otaq-core/src/attention/os_permission.rs:5-9`). | **A** |
| 3 | First question: the card turns turquoise, the strip appears, the rail tile gets a dot. One-time hint "An agent needs you … ⌘J … Got it". | The turquoise edge, strip chip, rail dot and ⌘J exist (`card.rs:1838`, `needs_you_strip.rs`, `ui/screens/shell/Rail.tsx`). There are no coach hints: `store/settings.rs` has no flags for them. | **A** for the states; **C** (S) for the hint and its stored flag. |
| 4 | First send-back by the checker: "Fixing · Validate found 1 problem / Sent back by the checker", and a one-time hint. | The sent-back line reads "Sent back · N comments · round N" (`card.rs:2807`). Validator findings are free text (`validation/mod.rs:1784`), so they have no count. | **C** (S): a problem count in the verdict, or drop the number. **C** (S) for the hint. |
| 5 | Review in magenta, "Ready for you · passed Validate", button Review. The hint says "Approve merges it into main; Send back returns it…". | Review edge and Review action exist (`card.rs:1838-1855`, `1861-1880`). The default flow's Delivery is `delivers: either`, `mode: click` (`crates/otaq-core/flows/otaq-default.yaml:29-35`), so approving offers Local merge **or** PR (`ui/screens/board/ApproveChoice.tsx`). | **A** for the card. **E**: the hint's "Approve merges it into main" is wrong for the default flow, which offers PR too. |

Open questions:
- Where does a new user choose or propose a flow now that the Flow step is gone? Is the agent proposal ("Made for <project>") dropped for good?
- Is the starter ticket shown for every new project, or only the first? Where does the "about 5 minutes" figure come from?
- Do the hints show once per install or once per project? Is there a way to reset them?

## board-02 · An ordinary day

| Design | App today | Class |
|---|---|---|
| Each card has at most 4 lines plus one action. Tokens, rounds, ports and models live in the sheet. | The card is already the one-line BRD-03 card (`ui/cards/TicketCard.tsx:1-8`). It can still grow: piece rows (`TicketCard.tsx:225`), check-agent rows (`:224`), "Blocks SHOP-20" (`:226-231`), "Plan · 4 pieces" (`:232`), origin "by Noyan · …" (`:233`), and a "Serving :4020" flag (`card.rs:1952`). | **D** if those go. No caption covers pieces, multiple check rows or the origin line: likely an **omission**. The ports move is deliberate. |
| Strip groups: "Needs you" (asks **and** start failed), "Your review", Next ⌘J. | Groups are Needs you, **Interrupted/Stopped** (with Resume all), and **Checks** (`crates/otaq-core/src/board/needs_you.rs:41-70`, `needs_you_strip.rs:370-396`). | **B**: start failed and interrupted fold into Needs you. **D**: "Resume all" (`ui/screens/strip/ResumeAll.tsx`) isn't shown. |
| A waiting card sits **at the top of the first stage** and says why ("Waits for SHOP-27 to merge"). | Waiting-to-start cards live in the Ready for Dev column, priority-ordered (`flow_columns.rs:9-12`; d0065). | **B** (M): the column merges into the first stage. Ordering inside Dev needs a rule. |
| Own-agent choice as one muted line: "Dev: Light instead of Strong". | The footer agent part says "Light · set by you" / "picked by …" (`card.rs:499-506`). | **A** |
| Agents called **Strong / Light / Verifier**. | The shipped agents are Developer and Validator (`crates/otaq-core/flows/otaq-default.yaml:1-7`). | **A** if illustrative. If it means shipped tiered agents: **C** (S). |
| Red edge on "start failed", with Retry. | Red is used only for a closed parent (`card.rs:1845`). Stopped states get turquoise or none. | **A** (recolour), but it changes what turquoise means. |
| Review column shows "Ready for you" cards and the "PR #481 open" card together, with no sections. | Review has READY FOR REVIEW / APPROVED sections, each foldable with its own drop target (`Column.tsx:391-400`, `flow_columns.rs:28-31`). | **B/D**: the Approved section and its drop target go. Dragging to Approved means "approve". |
| Done is a full column; cards read "Merged into main · today" or "Done · not merged · yesterday". | Done exists and folds to a "Done 4 →" line. | **A** |
| "New ticket, or ask Noyan…" ⌘N is one bar at the bottom. | New ticket (⌘N, quick-create at the top of Backlog) and the Ask bar (⌘L, opens the Project agent panel) are separate (`AskBar.tsx`, `Rail.tsx`). | **B** (M): one input must decide whether you are creating or asking. That is ambiguous. |
| The rail holds Board, Files, Settings and New ticket. | The toolbar has Board · List · Files tabs, Group by, Dependencies On hover/Always, Fold empty columns, All tickets and New ticket (`ui/screens/board/Toolbar.tsx:1-6`). The rail has projects, Add project, the Project agent, Terminals, Cleanup and Settings. | **B**. **D**: the Dependencies Always mode, Fold empty columns, the All tickets filter, and Terminals/Cleanup in the rail are not shown. |

Open questions:
- In what order do cards sit inside Dev: waiting cards first (this board) or needs-you first (board-05's List)?
- Where do Interrupted cards go in the strip, and what replaces Resume all?
- When is "New ticket, or ask Noyan" a ticket and when is it a question?

## board-03 · The card, every state

| State in design | App today (`card.rs` unless noted) | Class |
|---|---|---|
| Not started + **Start** | A calm Backlog card has no line and no action (`card_page.rs:36-37`). Start exists as ⌘D and on List rows (`list_view.rs:11-13`). | **B** |
| Working · writing tests | "Working" only (`card.rs:1135`). | **C** (summary item 3) |
| Checking · running the tests (agent "Verifier") | "Validating/Checking · round 1 of 3" (`card.rs:1371`). | **A** for the wording; the activity part is **C**. Rounds leave the card (deliberate). |
| Sent back: "Fixing · Validate found 2 problems" | "Sent back · round 2" (`card.rs:2807`). | **C** (S) for the count |
| Asks you · question → **Answer** | "Asks you: …" → **Reply** (`card.rs:845`, `1882-1887`). | **A** |
| Permission: "Asks you · allow 'npm install'?" → **Allow…** | "Needs you: allow …" (`card.rs:812-813`). The action opens the sheet. | **A**. Is "Allow…" an approve-from-card? Ambiguous. |
| Waits for a ticket | "Blocked · waits for SHOP-18 to merge", amber with a link icon (`card.rs:1738-1748`). | **A** |
| Waits for a free agent · 2nd in line | "Waiting for a slot" (`queue.rs:196`). | **A** plus a small **C**: the place in line comes from the order (S). |
| Own agent "Dev: Light instead of Strong" | Footer agent part (`card.rs:499`). | **A** |
| Urgent P1 chip | PriorityChip (`TicketCard.tsx:216`). | same |
| Your review: magenta, Review | `edge()` Review tone (`card.rs:1854`). | same |
| PR open · waiting for merge | "PR #481 open" (`card.rs:1163`). | **A** |
| Can't merge · conflicts with main → **Resolve** (red) | "Conflicts with main", or "Conflict · fixing" (the agent restacks), "Conflicts ×3" turquoise (`card.rs:1017,1563`). | **A/B**. What does Resolve do? Restarting the fix agent exists. |
| Start failed → Retry (red) | `RetryStart` (`card.rs:887`). | **A** (recolour) |
| Interrupted "Stopped by a restart" → Resume (red) | `Interrupted` → Resume (`card.rs:861`). | **A** (recolour) |
| Paused · resumes where it was | `paused_part` (`card.rs:1792-1825`). | **A** |
| Merged / Done · not merged | yes | **A** |

**D: states the app has that the design doesn't draw.** All are real statuses (`crates/otaq-core/src/domain/status.rs`), and none is covered by a caption, so these look like **omissions**:
- **check out of rounds**, "Decide" (`card.rs:1862-1868`), though board-04's panel promises "After 3 tries it waits for you";
- **Idle**, waiting for input;
- **Working in background**;
- **plan limit stop**;
- **Held**, base diverged;
- **Parent closed**;
- **Resume failed** (Start from scratch);
- **Checker needs you** and **Check no result**;
- **Merging · merge check** (with Stop);
- **Merged into another ticket**;
- **Parked · has work**;
- hand-off pending (**Deliver now**);
- **To deliver**: "Ready to merge" / "Ready for PR" clicks (`card.rs:1365-1369`).

The hover status card (tokens, branch; `TicketCard.tsx:33-48`) and the quick actions (open terminal, open panel; `:181-204`) are also not shown.

**E.**
- The app still has **Use fallback** card actions (`card.rs:349-351`, `2664`). The design has none, which matches the no-fallback rule, so the app is the side that conflicts.
- The app supports **several check agents per check** with per-agent rows (`card_page.rs:51-53`, d0130). The design shows one checker, which matches the "one agent per check stage" rule. Again the app conflicts.

Open questions:
- Does red now mean "stopped/broken, waiting for you", leaving turquoise only for asks?
- Does "Allow…" grant the permission from the card, or open the sheet?
- What does "Resolve" run on a conflict?

## board-04 · Edit flow, in place

| # | Design | App today | Class |
|---|---|---|---|
| 1 | The board becomes the editor: cards dim, headers become stages (name, what happens, agent). Drag to reorder, × to remove, + to add. Backlog and Done are locked. | A separate overlay: Start from column, step list with "Watch it" animation, details panel, Project agent terminal (`FlowEditor.tsx:1-13`, `AgentColumn` `:1225`). Steps reorder by grip or Alt+↑/↓. | **B** (L, UI) |
| 1 | Only Backlog and Done are fixed. | The board also has a fixed **Ready for Dev**. The **Delivery** is a real stage (`model.rs:76-84`); the default flow puts it in Review's column (`otaq-default.yaml:29-35`). | **D/E**: the design has no Delivery stage. Where do delivery mode (click/automatic) and target (local/PR/either) live? Only presets imply them (f4: "Solo autopilot", "Team PR"). |
| 2 | Stage panel: Name; What happens (does work / checks / you check); one Agent ("Any of your agents"); **Instructions · optional**; "If the check fails, goes back to <stage>"; "After 3 tries it waits for you"; Remove. | Name, kind, agent; **Skill** with Pick a skill… / Ask the agent to write it (`FlowEditor.tsx:511`, `crates/otaq-core/src/flow/write_skill.rs`); Fails go to (`:898`); **several check agents** (`:805`); **Rounds 1–5** (`:871`); options and skip; column; why line (`model.rs:48-73`). | **C**: per-stage `instructions` would be a new YAML field, sent via prime or launch (S–M). **D**: the skill field, rounds editor, options/skip, column, why, and Ask-to-write a skill. The caption gives no reason, so it looks deliberate simplification but is unstated. **A**: Fails go to. |
| 2 | "Remove stage: tickets in it move to the stage before". | Removal waits for a quiet span. Apply now offers a pick per ticket, defaulting to the **next** stage (`apply_now.rs:12-14`, `331`). | **E/C**: different rule. It also contradicts f5's "waits". |
| 3 | "+" between columns adds a stage and opens its panel (example: read-only security check). | "+ between steps on hover", Add a stage, templates (`FlowEditor.tsx:5-6,644`). | **A/B**. "Read only" is only an instruction; the app has no enforcement (agent permission modes are the tool's). |
| 4 | Start from: Otaq default, Spec first ("Spec → your check → Dev → Validate → Review"), Solo autopilot, Team PR. Replaces the stages between Backlog and Done; nothing changes until Save. | The same four presets (`crates/otaq-core/src/flow/presets.rs:78-103`). Plus **Made for <project>** (agent proposals per tool), **Import a flow…** and Export (`flow_editor.rs:6-7`). | **A** for the presets. **D**: agent proposal, import and export (README feature). **E** (S): the real Spec first is Product spec → Tech spec → Milestone plan → … (`crates/otaq-core/flows/spec-first.yaml`), not "Spec → your check → Dev". |
| 5 | "Flow saved · Applies when SHOP-26 leaves Validate. **New tickets use it already.**" | A waiting change exists (d0108/d0115; `apply.rs:14-22`). But **meanwhile no ticket enters the span**: tickets wait at its edge, "Waiting for the flow change" (`apply.rs:20,55`). A board swap holds all starts. | **E/C** (L): "new tickets use it already" needs tickets on different flow versions at once. **D**: Apply now and Cancel change aren't shown. |

Rule check:
- "A check stage has one agent": the design matches; the app allows several.
- "Flow YAML alone defines behaviour":
  - per-stage instructions keep this rule only if they are stored in the YAML;
  - the fixed "after 3 tries" and the delivery hidden in presets are still in the YAML, but the editor can no longer show or change them.
- "No skill copies": the design removes the skill picker and Ask-to-write a skill, which fits. The app still writes and imports skills (`write_skill.rs`; `agents/mod.rs` "importing a skill folder").

Open questions:
- How does a stage know *what* to do without a skill? Is the kind enough (work-ticket / validate / open-pr implied)? What happens to Spec first's skills?
- Is "3 tries" fixed or still editable?
- Where are Delivery (local merge / PR / either; click or automatic) and `with-previous` columns edited?
- Can a ticket still choose another agent or skip a stage ("Dev: Light instead of Strong")? If options leave the editor, which agents may a ticket pick?
- What happens to tickets in a removed stage: they move back (f2), or the change waits (f5)?

## board-05 · List, and what waits for what

| # | Design | App today | Class |
|---|---|---|---|
| 1 | List is a **layout switch** next to the views, not a fourth view. Rows grouped by stage in flow order, "what needs you first" (Dev, Validate, Review, Backlog, Done · 3 folded). | List is a toolbar tab like Board and Files (`Toolbar.tsx:1-3`). Group by is Stage, Status or Agent (`crates/otaq-view/src/view_models/view_prefs.rs:31-35`). Columns: TICKET · WHERE IN THE FLOW (a track) · WHAT'S HAPPENING · WHO (`list_view.rs:1-9`). Backlog is first, then Ready. | **B**. **D**: Group by Status/Agent, the flow track and the header row. The group order changes: Backlog moves after Review. |
| 1 | Inside Dev, needs-you rows come first and the waiting row last. | Rows keep the board's order (`list_view.rs:7-9`: "answering a ticket changes its row's edge and words, not its place"). | **B**, against an earlier explicit choice. |
| 2 | Hovering a card dims the rest and adds "SHOP-30 waits for it" to the hovered card. | The chain highlight dims non-members, draws arrows and shows a tooltip (`ui/screens/board/ChainLayer.tsx:1-6`, `chain.ts`). "Blocks SHOP-20" is always on the card (`TicketCard.tsx:226`). | **A/B**: the always-on Blocks line moves to hover only. **D**: arrows and the Always mode. |
| 3 | Overview has **WAITS FOR** with search (it lists a Done parent too). "It starts by itself once these are merged." The header reads "Waiting · In Dev · waits for SHOP-27 · started by you". | DEPENDENCIES has **BLOCKED BY** and **BLOCKS**, an Add picker that asks the direction first, and loop checks (`ui/screens/dependencies/Dependencies.tsx:1-6`). | **B**. **D**: adding links from the parent side ("Blocks"). It also has to say whether a merged parent stays listed. |
| 4 | Dragging a waiting ticket ahead is refused with a **toast**: "can't go to Validate yet · it waits for SHOP-27 to merge, then starts by itself". | A modal **BlockedDialog** with Keep it waiting; to start, remove the dependency (`ui/screens/board/BlockedDialog.tsx:1-5`). | **A/B** |

Open questions:
- Does a Backlog ticket with "Waits for" start by itself when its parents merge, or only after Start (today: only once queued)?
- Which order wins inside a stage: waiting first (board-02) or needs-you first (List)? Does the Board reorder too?
- Is "Blocks" (setting a link from the parent's side) gone?
