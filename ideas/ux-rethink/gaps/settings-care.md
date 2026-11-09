# Gap analysis: Settings and Care (restarts, cleanup, menu bar, quitting, updates)

Slice: settings-01-app, settings-02-project, settings-03-appearance-keyboard-about, care-01-restart-cleanup,
care-02-menu-bar-quit-update. Paths: `VM` = `crates/otaq-view/src/view_models/`, `UI` = `crates/otaq-desktop/ui/src/screens/`.

## Summary: the 5 biggest gaps by work

1. **Project settings become a Settings tab of the project, on one scrolling page with new cards** (Flow strip, Repository, Run table, Workspace, Notes, Remove). Today they are pages inside the global Settings modal (`UI/settings/Settings.tsx:254`, `VM/settings_page.rs:958`). Mostly B (moving and regrouping), plus C for "Deliver as" (it has to write the flow) and for env values read from `.env.local`. **M–L**
2. **Agents page merges Agent tools + Agents + the Noyan picker; the agent editor gets Save/Cancel.** Today these are 2 topics plus a role on each agent (`VM/agent_tools_page.rs:179`, `VM/agents.rs:608`, `VM/agent_page.rs:530`). Most of the removals (skills, fallbacks) are already agreed in `ideas/no-skill-copies` S4 and `ideas/no-fallback-agents`. New: Noyan's agent as a global picker instead of a role, and a draft/save model for saved agents. **M**
3. **Notifications shrink to 3 coarse toggles, and one of them is new: "A ticket is ready for your review".** That conflicts with today's rule "Nothing else ever notifies" (`VM/settings_page.rs:756`). "Not while you're looking at that ticket" becomes a toggle (today it is always on, `otaq-core/src/attention/focus.rs`). C in core attention. **M**
4. **Appearance follows the system by default, with one light and one dark theme.** Today it is one stored theme, and the OS is only read before a theme is saved (`UI/../theme/theme.ts:13`). The onboarding Theme step goes (`VM/onboarding.rs:28`). C: a new setting and a live OS listener. **S–M**
5. **The restart recovery becomes a dialog on open, not a banner plus a sheet** (`VM/after_restart.rs:436`, `VM/banners.rs:38`). **Limits & usage** merges two topics, the cap is always on, back-pressure goes, and plan usage is shown in Settings (`VM/settings_page.rs:694-750`, `VM/limits.rs:179`, `VM/activity.rs`). B with small C. **S–M each**

Main conflicts:
- "Deliver as" in project Settings vs a flow file defining the Delivery.
- The review-ready notification vs "blocking items only".
- The menu bar's "Restart to update · stops 4 agents" vs the banner's "working agents continue".
- Settings shows usage as % used; Activity's rule is % **left**.

---

## settings-01-app (App settings: Agents, Limits & usage, Notifications, Data)

**Nav.** The design has 7 topics: Agents, Limits & usage, Notifications, Appearance, Keyboard, Data, About. Projects leave the nav. Today there are 10 topics plus every project (`VM/settings_page.rs:521` DRAWN, `:543` nav; `UI/settings/Settings.tsx:241-265`).
- B: Agent tools merges into Agents; Agent limits and Usage merge; the "Tools" topic (git/gh/node checks, `VM/settings_page.rs:900`, `tools_intro` `VM/onboarding_page.rs:290`) **disappears**. That is D, and no caption mentions it, so it looks like an omission. Ask where git/gh/Node checks live now. Possibly onboarding (onb-02-tools).
- B: projects leave the nav ("Project settings live in each project's Settings tab").
- Open: is App Settings still a modal over the board (it is drawn over the board) or a page?

### Frame 1 · Agents
- **Tools list** (CC / CX / CP / OC with state and Install). Today: `VM/agent_tools_page.rs:179`, `UI/settings/AgentTools.tsx`. A/B (moved under Agents).
- D, all removed without a caption, so they look like simplification by omission: Sign in…, Hide/Show, Re-check, Disconnect…, "Otaq can see" chips, "Otaq added" setup lines, the permission warning ("5–10 agents, prompts will stop agents"), and How to install commands where Install isn't possible (Needs Node.js). Only "Signed in" and "Not installed" are drawn. Ask how "Found · not signed in", "update needed" and "Hidden" look.
- **Your agents list**: name, tool·model, "where used". Today: `VM/agents.rs:608`, columns Name/Model/Skills/Used in (`:649`). A, and the Skills column drop matches `no-skill-copies` S4.
- D:
  - "Default model for new agents" (`VM/agents.rs:644`)
  - "Fallback per tool" (`:613`); already decided to be removed (no-fallback-agents)
  - came-from line, ⋯ Duplicate / Delete…, "Apply the shipped agents file again" (`:537`)
  - the amber mark ("couldn't run model X", AGT-05)
- **"Light · Not in a flow · picked per ticket"** fits `one-check-agent` O1 (any agent for any stage, per ticket). Fine.
- **"Noyan uses [Strong]. You can switch when you start a conversation."**
  - B: today Noyan is a **role held by one agent**, set on the agent's page (`otaq-core/src/agents/mod.rs:45` Role::ProjectAgent; `VM/agent_page.rs:543` Role field). The design makes it a global picker on the list.
  - C (small): "switch when you start a conversation" means picking an agent per new Noyan conversation. Today only "New agent on …" exists, and only when the tool can't run (`VM/tool_problems.rs:482`, `VM/agent_panel.rs:518`). Kept by no-fallback-agents for that case. A general per-conversation pick is new; the Noyan slice should confirm it.
- E: none. "Your agents (tool + model + instructions)" matches S4.

### Frame 2 · An agent
- Name, Tool, Model ("opus · always the newest"), Instructions, Used in, Delete agent, Cancel/Save. Today: `VM/agent_page.rs:530`, `UI/settings/Agents.tsx`.
- B: **Cancel / Save**. Today a saved agent saves each change as it is made; only New agent and Duplicate use a draft with "Add agent" (`VM/agent_page.rs:5-6`). The whole editor moves to an explicit-save model. Small C in the view (dirty state, leave-without-saving).
- D:
  - **Effort** (`VM/agent_page.rs:509`). An omission? Effort matters for Codex and Claude.
  - **Role** (moved to the Noyan picker)
  - **Skills / Import skill folder / Re-import** (already decided: S4)
  - **"What this agent reads"** (`UI/settings/AgentReads.tsx`, `VM/agent_reads.rs`, kept by F17). Its drop looks like an omission.
- Wording A: "Every ticket this agent works on starts with these." Today: "Edits apply to the next session that starts" (`VM/agents.rs:635`). Keep the "next session" truth: running sessions don't change.
- "Used in: shop-web · Dev, billing-api · Dev, Noyan" exists as `used_in` (`VM/agents.rs`). A.

### Frame 3 · Limits & usage
- **Run at most [5] agents at once, across all projects.**
  - Today: optional caps, **off by default** with a toggle (`VM/settings_page.rs:697-737`, `VM/limits.rs:179-205`), max 1–50.
  - C (small): the cap is always on with default 5 (a settings migration: off becomes 5? or unlimited?).
  - D: the **back-pressure** row "Don't start new tickets while Needs you has N" (`VM/limits.rs:66`) and the "Right now: 3 running · 2 waiting…" line are dropped without a caption.
- "Waits for a free agent" at the top of the first stage. Today "Waiting for a slot" (`VM/queue.rs:8`, Status::WaitingForSlot). A wording.
- **USAGE per tool.**
  - Today plan windows live in the Activity popover (`VM/activity.rs:1-12`). Settings' Usage only has "Token count · Always on" (`VM/settings_page.rs:739`). B: plan windows are repeated in Settings.
  - D: tokens are not shown here; the token row is gone.
  - E (minor): the design shows **"85% of the 5-hour limit"** (used), while Activity's rule is "Percentages are what's **left**" (`VM/activity.rs:9-10`) and amber at ≤20% left. Pick one convention.
  - Not drawn: the "Limits not reported" and "No plan limit · tokens only" states.
- "When a tool hits its limit, its agents stop and wait; other tools keep working. A banner says when it resets." Matches no-fallback-agents. OK.

### Frame 4 · Notifications
- Today (`VM/settings_page.rs:752-790`, `:352`; core `otaq-core/src/attention/notify.rs:50`):
  - a main switch with macOS status ("Allowed in macOS" exists, `:317`)
  - 7 kind checkboxes (permission, question, turn ended, interrupted, validation decision, delivery stopped, tool unavailable)
  - Play a sound
- Design: 3 toggles: asks something / ready for review / something broke; plus "Not while you're looking at that ticket".
  - B: 7 kinds grouped into 2 (asks = permission+question+turn-ended?; broke = interrupted+validation+delivery stopped+tool unavailable). C to store or migrate them as groups.
  - **C + E: "A ticket is ready for your review"** is a new notification kind. It contradicts the current rule "Nothing else ever notifies" (only blocking items, d0090 wording in `VM/settings_page.rs:756`). Needs a product decision.
  - C (small): "Not while you're looking at that ticket" as a **toggle**. Today suppression while focused is always on (`otaq-core/src/attention/focus.rs`, `needs_you.rs:237`).
  - D: master switch and **Play a sound**. Omission?
- Caption "even with the window closed" is true today (tray, first-hide note).

### Frame 5 · Data
- Today (`VM/settings_page.rs:792-850`): Database file and Logs folder (each with Reveal), Cleanup… with a count, **Share usage data** toggle (d0085).
- Design: "Data folder ~/.otaq · Show"; "Cleanup · 3 leftovers · 412 MB · Review…".
  - A: one folder instead of DB + logs. D: the Logs folder is dropped. Omission?
  - C (tiny): the **total size** in the row. Today the row shows the count only; sizes are per item in Cleanup (`VM/cleanup.rs:87`, `:228`).
  - B: Cleanup leaves the rail (caption says so). Today it is the broom on the rail (`UI/shell/Rail.tsx:43-55`, `shell.cleanup`). Deliberate.
  - **D: the "Share usage data" consent toggle** (`VM/settings_page.rs:843`, d0085) is gone. This has to live somewhere (privacy consent). Looks like an omission.

## settings-02-project (a project's Settings tab)

**Placement.** B: a "Settings" tab next to Board / Files in the project header, one scrolling page with a sticky section list (scroll-spy). Today a project page lives in the global Settings modal nav (`UI/settings/Settings.tsx:254-264`, `VM/settings_page.rs:958-1118`) and opens from the project menu "Project settings…" (`UI/shell/Header.tsx:3`). The data and edits stay the same (`settings.view` / `settings.edit` with `project`). **M** (UI shell + page).

| Design card | Today | Class |
|---|---|---|
| **Flow**: stages strip with who works each, "Backlog and Done are fixed", Edit flow | Settings → Flow section: changed-by line, read-only stage table, waiting change, Change flow…, Use a preset…, Copy from another project…, Export (`VM/flow_settings.rs:1-5`, `UI/settings/Flow.tsx`) | A/B. D: preset / copy / export / "changed by" / waiting-change Cancel aren't on this card. Ask whether they live in the flow editor now (board-04 slice). |
| **Repository**: Base branch | General row (`VM/settings_page.rs:992`) | A |
| Repository: **Deliver as: Merge into main / Pull request** ("What Approve does") | Delivery is the **flow's** (`delivers: local/pr/either`, `mode`; `otaq-core/src/flow/model.rs`; FLW-01 note `VM/settings_page.rs:7-9`) | **E/C**: a project setting that changes the Delivery breaks "a flow YAML alone defines behaviour". Either it edits the flow (a new flow version through the editor) or it goes. It also assumes Approve-with-a-click; an automatic Delivery or `either` doesn't fit the binary choice. |
| Repository: Pull request · GitHub · repo · gh signed in | PR provider row (`VM/settings_page.rs:1870`, `Control::Provider`) | A (moved) |
| Repository: Merge check · optional | Commands section, `merge_check_row` (`VM/settings_page.rs:620`) | B (moved) |
| **Run**: apps table App / Command / Open URL / Port (`$OTAQ_PORT_WEB`), + Add app, "Set by Dev on SHOP-27 · Undo" | `app_rows` (`VM/settings_page.rs:1218`), Undo `run.command_undo` (`UI/settings/Settings.tsx:113`) | A (row form becomes a table). The port column is a display of the existing `OTAQ_PORT_<APP>`. "Ask the agent" (RUN-02) isn't drawn: D, omission? |
| **Workspace**: Setup command | Commands (`:1048`) | A |
| Workspace: Environment variables, incl. `STRIPE_KEY •••• (from .env.local)` | `env_vars_row` (`:1345`); `EnvVar{name,value}` only, no masking, no reading env files (`otaq-core/src/board/settings.rs:714`) | C (S): list variables read from copied env files, masked. Or drop. Ask whether that's intended. `$TICKET` vs `$OTAQ_TICKET`: A. |
| Workspace: Files to copy | Env files row (`:1060`) | A |
| Workspace: Per-ticket resources (create / drop script) | Ticket resources section: Create, Remove, **Template refresh**, **Schema paths** (`:1120`) | A + D (template refresh and schema paths dropped, no caption) |
| Workspace: Warm build | `warm_builds_row` (`:1382`) | A |
| **Notes for agents** | Project notes (`:1023`) | A |
| **Remove project** | `remove_dialog` (`:1971`, texts `VM/settings.rs:185`) | A; wording consistent |
| — | **Name** (editable), **Project key**, **Repository path** (`:960-990`) | D: name editing gone; the header shows `~/code/shop-web`. Omission? |
| — | **"Let finished tickets answer questions"** (`:1032`) | D, no caption. Decide. |

## settings-03-appearance-keyboard-about

### Frame 1 · Appearance
- "Follows the system by default (no onboarding step); one light and one dark theme." Theme: Match system / Dark, light pick and dark pick, "+ 11 more themes", Terminal font "Geist Mono · 13".
- Today: one theme "used all the time" (`VM/settings_page.rs:856`). The OS is read only until a theme is stored (`UI/../theme/theme.ts:13`, `theme.active` takes `os_dark`). 15 themes, 5 light and 10 dark (`VM/theme.rs:24-90`).
- **C**: new settings (mode + light theme + dark theme), migrating the stored single theme, and a live `prefers-color-scheme` listener. The tray icon and native window look may need it too. **S–M**
- B/D: the onboarding **Theme step** is removed (`VM/onboarding.rs:28`), deliberate per the caption.
- D: **Theme credits** row (`:860`, licence obligation, MIT credits) is gone from Appearance. Is it still under About's "open-source licences…"? Must stay reachable. D: **Terminal renderer** (WebGL/DOM fallback for blank terminals, `:886`) is gone. Omission; it's a fix-it switch.
- A: terminal font family + size merged into one control (`:869`, `:879`).

### Frame 2 · Keyboard
- "The shortcuts that matter, editable": 8 rows. All exist (`otaq-view/src/keymap.rs:188-220`): ⌘J, ⌘K, `/` filter, ⌘L, ⌘D, ⇧⌘., ⌃`, Ticket tabs.
- Today: every action, grouped, plus the single-letter switch, "Works inside terminals", per-row reset, Reset all (`VM/keyboard_page.rs:1-11`, `UI/settings/Keyboard.tsx`).
- B: trimmed list. D: the single-letter switch, Works inside terminals (Windows/Linux), Reset, and roughly 25 other actions (Approve ⌘↵, Send back ⌘⌫, Files ⇧⌘O, Go to file, Switch project ⌘P, New conversation ⇧⌘L…). Ask whether that's hidden behind "show all" or removed. If removed, users lose rebinding of keys they rely on.
- A: "Start / move to the first stage" vs "Start / move to Dev" (`keymap.rs:211`). The design wording is better with flows.

### Frame 3 · About
- Version, Updates (ready · Restart to update · Check automatically), "otaq in your terminal · Not in your PATH · Add to my shell's PATH", Licence.
- All exist: `VM/about.rs:1-7`, `:157-192`; `VM/updates.rs:441-495`. A.
- D: Copy version info, commit/"Built from source" line, trademark line, View license, Skip this version / What's new. Some are probably just not drawn; keep Copy version info (bug reports) and the trademark line.

## care-01-restart-cleanup

### Frame 1 · Opening after a restart
- One dialog: ticked rows, a tool that can't resume is unticked with the reason, "Not now / Resume 2", a note about SHOP-22 waiting for an answer.
- Today: a red banner "N sessions were interrupted" with **Resume all / Review & resume** (`VM/banners.rs:38,72,174-177`) opening a summary sheet. The sheet already has rows ticked by default except ones whose tool can't run (`VM/after_restart.rs:300`), a "Resume selected skips…" line (`:405`), asking-tickets notes (`:524`), "They stay in Needs you if you close this" (`:549`).
- B: banner + optional sheet becomes a dialog shown on open. "Resume all" without review disappears. **S**
- D:
  - the **Noyan conversations** group (Resume same conversation, `VM/after_restart.rs:179-205`)
  - per-row results (Resumed / Couldn't resume · Retry · Start from scratch)
  - the "Review and Approved untouched · Run and Shell stopped" note (`:531`)
  - the quiet "Reconnected" state
  - Omissions? The Noyan group at least must stay.
- A: "Codex isn't signed in · sign in first" as text vs today's Sign in… button in place of Resume. Keep the button.
- Consistent with no-fallback (it waits; no "resume on another tool").

### Frame 2 · If you close it
- "Stopped cards are red with Resume, and in the Needs you strip."
- Today: card flag "Interrupted" (`VM/card.rs:858`), and the strip has its **own "Interrupted · N" group** with Resume all (`VM/needs_you_strip.rs:4, 386-390`).
- B/A: label "Stopped by a restart"; merge into Needs you. The drawn strip doesn't actually list the stopped SHOP-27, so the design contradicts itself. Ask.

### Frame 3 · Cleanup (Settings → Data)
- Flat list, sizes, nothing ticked, "Delete selected…".
- Today: `VM/cleanup.rs:1-10`, `UI/recovery/Cleanup.tsx`. Same rule (nothing selected, d0018). A.
- D: the **groups** (Sessions, Worktrees, Branches, Ports, **Done tickets with uncommitted changes** warning, Couldn't-free builds, ticket resources that couldn't be removed) and the **warm builds** section with Clear… (`VM/cleanup.rs:7-9`, `:24-50`). The uncommitted-changes warning and the warm builds are omissions to keep.
- B: the entry point moves from the rail to Settings → Data.

## care-02-menu-bar-quit-update

### Frame 1 · Menu bar icon
- Count when something needs you; Needs you list, then "4 agents running · shop-web 3 · billing-api 1", Open Otaq, Pause all, "Restart to update · stops 4 agents", Quit Otaq ⌘Q.
- Today: `VM/status_icon.rs:160-185` (count, Needs you, **Your checks and stopped tickets**, Running one line per ticket, Paused, cap 10, Open Otaq, Pause all / Resume all, Stop agents and quit, Restart to update `:56-79`).
- B: running shown as per-project counts instead of per-ticket lines.
- D: the "Your checks and stopped tickets" list, Resume all, and the Paused lines.
- **E**: "Restart to update · **stops 4 agents**" contradicts frame 4's banner ("working agents continue afterwards") and today's behaviour (a compatible update restarts only the app; only an incompatible one restarts the service, `VM/status_icon.rs:56-57`, `VM/updates.rs:489,523`).
- B: "Quit Otaq" replaces "Stop agents and quit". Presumably it opens the frame-3 question.

### Frame 2 · Closing the window the first time
- "Agents keep working · Otaq is in the menu bar…" once.
- Exists: first-hide note (`crates/otaq-desktop/src/api/window.rs:44-56`, `Request::AppFirstHide`). A.
- Linux without a tray (close = stop and quit, `VM/no_tray.rs`) isn't covered. Fine.

### Frame 3 · Quitting while agents work
- Cancel / Quit and stop agents / **Keep them running** (default).
- Today: `VM/quit.rs:47-63`, "Keep them working and quit" first, "Stop agents and quit", "Cancel". A.
- The design text "Quitting the window keeps them running" muddles close vs quit. Today's text says they keep working "with no window and no icon". Keep today's truth.
- D: the "Otaq didn't quit" failure dialog (`VM/quit.rs:117`). Not drawn, so presumably kept.

### Frame 4 · An update is ready
- A calm banner on the board ("Restart to update; working agents continue afterwards"), never a modal; same row in About.
- Today: a notice at the top of About plus a dot on the rail's Settings button (`VM/updates.rs:1-5`, `UI/shell/Rail.tsx:6`). An app banner exists only for "restart required" (`VM/banners.rs:174`).
- B: a board-level banner for every ready update. **S**. Dismiss / Skip this version isn't drawn.

## Open product questions
1. "Deliver as" in project Settings: does it edit the flow (new version), or is it removed? (E)
2. Review-ready notifications: does the "blocking items only" rule change? Which of the 7 kinds go into "asks" vs "something broke"? Are sound and the master switch kept?
3. Agent limits always on: what do existing users with limits off get? Is back-pressure removed on purpose?
4. Usage: % used or % left? Are tokens no longer shown anywhere in Settings?
5. Noyan: one global agent with a per-conversation switch. Is the per-conversation pick new for every conversation, or only the existing "New agent on…" when the tool can't run?
6. Agent editor: is effort dropped or just not drawn? Is "What this agent reads" (F17) kept? Explicit Save vs today's save-on-change?
7. Agent tools: where do Sign in…, Hide, Re-check, Disconnect, "Otaq can see", and not-signed-in / update-needed states go? Where do the git/gh "Tools" checks go?
8. Keyboard: is the full list and the single-letter switch still reachable?
9. Data: where do the usage-data consent toggle and the Logs folder live?
10. Appearance: Theme credits and the Terminal renderer fallback: kept where?
11. Project page: Name editing, "Let finished tickets answer questions", Template refresh / Schema paths, Run's "Ask the agent", flow presets / copy / export: removed or moved?
12. Restart dialog: Noyan conversations, Retry / Start from scratch results: kept? Does it pop over the board every launch after a restart, or only once?
13. Menu bar: does "Restart to update" stop agents or not? Is "Your checks and stopped tickets" gone?
14. Cleanup: keep the uncommitted-changes warning group and warm builds?
15. Env vars masked "(from .env.local)": a new feature or only illustration?
