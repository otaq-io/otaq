# Gap analysis: onboarding + window chrome (Main/Welcome, onb-02-tools, onb-03-project, window-01-title-bar, window-02-search-palette)

Paths are relative to the repo root `/Users/nevzattopcu/Desktop/Workspace/otaq`. UI = `crates/otaq-desktop/ui/src/screens`, VM = `crates/otaq-view/src/view_models`.

## Summary: the 5 biggest gaps by size of work

1. **Window chrome restructure (L, B).** Today the left rail lists every project as a tile with its working and needs-you counts, plus Noyan, Terminals, Cleanup and Settings (`UI/shell/Rail.tsx:1-8`). The project name opens a project menu: Change flow…, Export flow…, Project settings…, Open in Finder, Open a terminal here (`VM/project_menu.rs:84-98`). In the design, the rail is area navigation (Board · Files · Settings, plus Terminal), the project name only switches or adds projects (with a dot for needs you), and "Edit flow" sits at the board's end. Every item in the project menu has to be moved somewhere, and the List view, Export flow and Open in Finder lose their places.
2. **Onboarding reshaped to 3 steps (M, B+D).** Theme and Flow are dropped from Welcome·Theme·Tools·Project·Flow (`UI/onboarding/logic.ts:15`). Welcome becomes an illustration of a miniature board. The Tools step loses the command box, the "Otaq can see" line, the … menu, the starter-agent line and the permission warning, and gh/az move to the project's "Repository" settings. The Flow step (flow editor, an agent proposing a flow, Use this flow / Skip) goes away (`UI/onboarding/Onboarding.tsx:264-277`).
3. **Sign-in without a visible terminal (M, C, possible E).** In the design the row shows "Signing in… finish in your browser". Today Install and Sign in… run the tool's command in a terminal under the row (`UI/onboarding/RowTerminal.tsx:1-4`) because there are prompts to answer: a password, a device code, Homebrew. The design would need a sign-in progress state per tool and some way to handle interactive prompts.
4. **Search becomes a ticket finder (M, B+C).** Today `/` filters the board in place, with chips and filter suggestions that show counts (`UI/board/Search.tsx:1-5`, `VM/search_page.rs:1-17`). The design shows a dropdown of matching tickets with their status (including Done tickets that are off the board), and names the filters only as text. It also uses tokens that don't exist yet: `in:review` and `by:Strong` (`VM/filter.rs:120-145` has `is:`, `has:` and `dep:` only).
5. **Limits and usage chip (M, C).** In the design the amber usage chip appears only when a limit is hit, and the banner has no buttons. Today the chip turns amber at ≤20% left (`VM/activity.rs:32-33`), and the banner offers Resume <tool> plus "Use fallback now" (`VM/limit_banner.rs:5-11`, `UI/notices/LimitLines.tsx:1-5`). Dropping the fallback is required by the maintainer's no-fallback rule anyway, which means core, view and UI work. It is unclear whether Resume is dropped (see the open questions).

Smaller items: the palette's empty state becomes "Suggested" with next-needs-you first (S). In the paused state, Running and Pause leave the title bar and Resume moves into the banner (S). The Running popover loses the agent-limit stepper (S, D). Assorted copy and placement changes (S).

---

## Main.txt: Welcome

Caption: *"teaches the whole model with the board itself: five columns, who acts in each, and the turquoise card. The columns shown are the default flow… One button."*

| Design | App today | Class |
|---|---|---|
| Steps Welcome · Tools · Project (3) | Welcome · Theme · Tools · Project · Flow (`UI/onboarding/logic.ts:15`, `UI/onboarding/Onboarding.tsx:1-9`) | B / D |
| Miniature board: Backlog / Dev / Validate / Review / Done, a line for each column on who acts, the SHOP-1 card going turquoise ("Asks you") | Title, text and 3 points: Real sessions, Needs you, You stay in control (`VM/onboarding_page.rs:132-161`) | A (copy and illustration) |
| Title "Coding agents that follow your process" | "Run coding agents from a board" (`VM/onboarding_page.rs:132`). The README headline already uses the design's wording | A |
| Columns = default flow | `crates/otaq-core/flows/otaq-default.yaml`: Dev, Validate (check, by agent), Review (by you), Done (delivery, `delivers: either`). They match | — |
| "Done: Merged into your branch" | Default delivery is `either` (Local merge or Open PR) | E (minor): the copy hides the PR path |
| "About 2 minutes", button "Get started" | "Takes about a minute", "Choose a theme" (`VM/onboarding_page.rs:160-161`) | A |
| "Otaq lets you know, even when the window is closed" | The "Needs you" point mentions the strip and ⌘J (`VM/onboarding_page.rs:148-152`) | A |

D, dropped:
- The **Theme step** (`UI/onboarding/Onboarding.tsx:202-221`, `VM/onboarding_page.rs:177-183`). No caption says why, so it is probably deliberate simplification. The theme stays in Settings → Appearance.
- The **"Attach from any terminal with `otaq attach`"** point and **"Nothing starts, retries or deletes without a click"**. These are product promises, and nothing in the design replaces them.

## onb-02-tools.txt: Tools

Captions: *"Only what's needed to start: the four agent tools and git. Continue works as soon as one tool is signed in. Optional tools (gh, az) move to the project's Repository settings."* / *"Continue is disabled and the reason is said once, at the top."* / *"Sign in… runs the tool's own sign-in; the row shows progress and the button disappears until it finishes or fails."*

| Design | App today | Class |
|---|---|---|
| Agent rows: Signed in / Installed · not signed in [Sign in…] / Not installed [Install]; header "1 ready" | Same states, with the header "AGENT TOOLS · AT LEAST ONE" and "N connected · M hidden" (`VM/onboarding_page.rs:355-393`, `UI/onboarding/Tools.tsx:1-8`) | A |
| Continue rule: one agent tool ready, plus git | `continue_state` requires the required tools (git) and one connected agent tool (`VM/onboarding.rs:141-166`). Same | — |
| No-tool reason shown once at the top; Continue disabled | Blocking line above the list plus the reason beside Continue (`VM/onboarding_page.rs:386-389`, `VM/onboarding.rs:155-160`) | A |
| "ALSO NEEDED: git 2.46.0" only | "OTHER TOOLS" covers git, gh and az, each with its version, the command to copy, and Install / Sign in… (`VM/onboarding_page.rs:393-425`) | B |
| gh/az move to the **project's Repository settings** | gh/az sit in the global Settings → Tools (`VM/settings_page.rs:572-586`, `ToolView` at `:1895`). Project sections are General, Flow, ProjectNotes, Answers, Commands, TicketResources, PrProvider, RemoveProject (`VM/settings_page.rs:102-117`). No "Repository" section exists; "Repository" is only a row under General (`:985`) | B (rehome under PrProvider), plus a question: gh/az are machine-wide, not per project |
| Sign in… → "Signing in… finish in your browser", no terminal | Terminal under the row (`UI/onboarding/RowTerminal.tsx:1-4`), Hide / Retry / "Not done yet" (`VM/onboarding_page.rs:262-272`) | C: needs a sign-in progress state per tool. **E-risk**: Install via brew/npm and device-code logins need an interactive terminal |
| "You can add more tools later in Settings → Agents" | "…in Settings → Agent tools", plus the starter-agent line "Otaq starts you with one agent… Make others, like Strong or Light, in Settings → Agents" (`VM/onboarding_page.rs:442`). Agent tools and Agents are separate sections (`VM/settings_page.rs:576-577`) | B (are the two sections merged?), D (the starter-agent line) |
| Copy: "with your own sign-in and plan; never installs or signs in without your click" | "uses the tools already on your machine and never updates them…" (`VM/onboarding_page.rs:292-293`) | A |

D, dropped:
- The **Claude Code permission-mode warning** ("With 5–10 agents, prompts will stop agents often…", `VM/onboarding_page.rs:367-378`, raised as a banner after onboarding at `UI/onboarding/logic.ts:80-84`). No caption mentions it, so this looks like an omission.
- **Show / Hide, the install command box, "Otaq can see", the tool's own warning, and the … menu** on each row (`UI/onboarding/Tools.tsx:1-8`). Today the command is visible before you click it; the design hides it. This is probably deliberate ("Only what's needed"), but it weakens the transparency the copy promises.
- The **"Check failed: …"** error state is not shown.
- **Anonymous usage data**, asked once at the end of first run (`UI/onboarding/Onboarding.tsx:161-164`, `UI/onboarding/UsageData.tsx`). Not shown; probably an omission.

## onb-03-project.txt: Project

Captions: *"Name, key and base branch are filled in from the repo; nothing else is asked. The folder line says why it's safe."* / *"The error names the folder and the fix."*

| Design | App today | Class |
|---|---|---|
| Folder, Choose folder…, then Name / Ticket key / Base branch | Same fields. Name is detected (read-only) and the key is editable with a live check (`VM/onboarding_page.rs:566-622`, `UI/onboarding/AddProjectForm.tsx:152-256`) | A |
| Key note "Tickets are named SHOP-1, SHOP-2, …" | "Used in ticket IDs like SHOP-18. **Can't be changed later.**" (`VM/add_project.rs:116-123`) | D: drops the irreversibility warning, probably an omission |
| Base note "Agents start from here; work merges back here." | Base branch dropdown with local and remote branches (`VM/onboarding_page.rs:597-603`) | A |
| Folder line: "Agents work on their own copies of it, so your folder and your current branch are never touched." | "…keeps everything in its own database; nothing is written into the repo." (`VM/onboarding_page.rs:571-575`) | **E**: Local merge fast-forwards the base branch's clean checkout, which is the user's folder when they are on main (`crates/otaq-core/src/git/merge.rs:5,16`). "Never touched" is not true |
| Footnote "Run command, environment and delivery can wait" | "Setup command, run command and env files can be added later…" (`VM/onboarding_page.rs:615-618`) | A |
| Not a git repo: "~/Downloads isn't a git repository. Choose a folder that has one, or run git init in it first." | "Not a git repository" / "{shown} has no .git folder. Pick the repo's root folder, or run git init there first." (`VM/add_project.rs:193-195`) | A |
| Add project → (onboarding ends on the board) | Add project → **Flow step**: flow editor with presets, an agent proposing a flow, Use this flow / Skip (`UI/onboarding/Onboarding.tsx:252-277`, `UI/onboarding/logic.ts:95-105`) | D. Deliberate per the Welcome caption ("default flow the project will start with") |

D, dropped:
- The **folder-trust line** "Adding a project also lets Claude Code, Codex and GitHub Copilot work in this folder (their hooks and settings will run)" and its error "…trust couldn't be recorded…" (`VM/add_project.rs:148-164`). This is a security disclosure, so dropping it looks like an omission.
- The **"already in Otaq"** error ("Switch to it with ⌘N…", `VM/add_project.rs:201-215`) and the generic "Can't add this folder" error are not shown.
- **Drag-and-drop of a folder** (`UI/onboarding/drops.ts`) is not shown.
- If the design's Name is editable, that would be new in first run (A/B). It is unclear from the text.

## window-01-title-bar.txt: Title bar, rail, banners

### 1 · Running: *"Running lists agents and apps, with usage per tool at the foot."*
- Today the Activity button opens a popover (⇧⌘U) that shows the agent-limit stepper, AGENTS RUNNING, USAGE · WHAT'S LEFT, PROCESSES and "Open Terminals" (`UI/shell/ActivityPopover.tsx:1-6`, `UI/session/Processes.tsx:1-2`). The design labels them "4 running", AGENTS, APPS (port, Open ↗, Stop) and USAGE (% · resets). **A**: the renames are Activity → Running and Processes → Apps.
- **D**: the agent-limit stepper (also in Settings → Agent limits, `VM/settings_page.rs:86`), Stop all, and "Open Terminals" are gone from the popover.
- Ambiguity: "85% · resets 3:45 pm" doesn't say used or left. The app always shows **left** (`VM/activity.rs:9-10`).
- The design counts an agent that "asks you" in "4 running". Check that against how the app counts today.

### 2 · Pause: *"Pause asks for the scope: this project or all of them."*
- Matches today: a scope menu with this project first and All projects with counts, no confirmation, ⇧⌘. (`UI/pause/Pause.tsx:1-5`, `VM/pause.rs:1-17`). **A** (copy only).

### 3 · Switching projects: *"The project name only switches projects or adds one. A dot says which project needs you."*
- Today you switch from the rail tiles (`UI/shell/Rail.tsx:1-8`), the ⌘P quick switcher (`UI/shell/QuickSwitcher.tsx:1-3`) and the palette's "Switch to …". The project name opens the project menu (`UI/shell/Header.tsx:1-5`, `VM/project_menu.rs:84-98`). **B**: the switcher becomes a dropdown on the name, and the menu's items have to be rehomed.
- **D**:
  - Each rail tile shows its working and needs-you counts and rings for 10 s when a project newly needs you. The design reduces this to a dot.
  - Export flow… and Open in Finder have no place.
  - Change flow… becomes "Edit flow" at the board's end. Board slice, but it needs confirming.
  - The rail's Cleanup broom and Noyan button aren't visible in this artboard's text. Noyan stays reachable as palette "Ask Noyan ⌘L".
  - A project in Flow setup ("billing-api · Flow setup", `UI/shell/Header.tsx:6-7`) has no design. That's consistent with the Flow step being dropped, but projects added later still go to Flow setup (`UI/onboarding/logic.ts:86-89`).
- The rail is "SH · Board · Files · Settings". Today Board · List · Files is the board toolbar's view switch (`UI/board/Toolbar.tsx:1-3`). **B**: the views move to the rail and **List is dropped** (D?), or lives elsewhere (another slice). Putting Settings in the rail replaces the rail's Settings button (⌘,). Whether Settings means the project's or the global settings isn't clear.

### 4 · A tool hits its limit: *"Only that tool's agents wait. The banner says when it resets; the amber usage chip appears in the title bar only now."*
- The per-tool banner exists ("Claude Code limit reached · Resets 3:45 pm · 4 tickets waiting", `VM/limit_banner.rs:5-11`).
- The design has **no buttons**. Today there are **Resume Claude Code** (manual, never automatic: "Resume is the user's click and never happens by itself", `VM/limit_banner.rs:9-10`) and **Use fallback now (Codex)** (`UI/notices/LimitLines.tsx:1-5`, `VM/limit_banner.rs:40,65`).
- **C (rule-driven)**: the no-fallback rule means removing Use fallback now and the fallback machinery from core, view and UI. That is real work even though the design only *omits* it.
- **Open question / E**: without Resume, do agents continue on their own at reset? That would contradict today's rule. Or is Resume only hidden until the reset time?
- Chip: today it is amber at ≤20% left (`VM/activity.rs:32-33`). The design shows it only at the limit. **B/C (S)**: a threshold change, and the 20% warning is lost (D).

### 5 · Paused: *"One banner with Resume; Running and Pause leave the title bar while nothing runs."*
- Today there is an amber banner that can't be closed ("shop-web is paused.", `VM/banners.rs:33-35,60-62`). Activity reads "5 paused", and the title bar's Pause becomes an accent **Resume** (`VM/pause.rs:64`).
- **B (S)**: Resume moves into the banner, and the Running/Pause controls hide while nothing runs.
- Open: how do you resume "All projects" from this banner, given the scope menu (`VM/pause.rs:14-17`)?

### 6 · Terminal: *"The rail's Terminal opens a shell in the project folder as a drawer under the board (⌃`)."*
- Exists: the Terminals drawer with tabs and +, ⌃`, Project terminals only (`UI/terminals/Drawer.tsx:1-5`), and the rail's Terminals button (`UI/shell/Rail.tsx:5-6`). **A**.

Across all frames:
- "New ticket ⌘N" sits in the header row; today it is in the board toolbar (`UI/board/Toolbar.tsx:1-4`). **A**.
- The search entry reads "Search tickets  /"; today the placeholder is "Search · is:needs-you" (`VM/search_page.rs:17`). **A**.
- A card reads "Dev: Light instead of Strong". If that is a per-ticket agent override, it already exists (README: "per stage, per project or per ticket"), so **A**. If it means a tool or agent substitution, it is an **E** conflict with no-fallback. The wording needs to rule out the fallback reading.

## window-02-search-palette.txt: Search and command palette

### 1 · Search (/): *"Tickets by key or words, with their status; filters are offered as text, not a separate UI."*
- Today `/` focuses the header field, filters the board as you type, turns finished tokens into chips, and suggests filters with counts ("FILTERS"). Typing "cleanup" opens Recovery (`UI/board/Search.tsx:1-5`, `VM/search_page.rs:1-20`).
- The design shows a **TICKETS result list** (key, title, status, including Done SHOP-12) and one text line: "Filters: is:needs-you · is:working · in:review · by:Strong".
- **B**: from board filtering to a jump-to-ticket list, which the palette already does (`VM/palette.rs:1-5`). Open question: does `/` still filter the board?
- **C (S)**: new tokens `in:<column>` and `by:<agent>`. Today `filter.rs` parses `is:`, `has:` and `dep:` (`VM/filter.rs:120-145`).
- **D**: the filter counts, the chips, and the "cleanup" shortcut.

### 2 · Command palette (⌘K): *"Every action by name, with its shortcut; the first suggestion is the next thing that needs you."*
- The palette exists: ⌘K / ⇧⌘P, tickets, projects and actions in one ranked list, `>` for actions only, and actions that can't run greyed with the reason (`UI/palette/Palette.tsx:1-4`, `VM/palette.rs:1-5,22-31`).
- **B (S)**: the empty state is "Recent" items today (`VM/palette.rs:29`). The design shows "SUGGESTED", led by "Next that needs you · SHOP-22 ⌘J" (an existing action, `crates/otaq-view/src/keymap.rs:84-85,193`), then New ticket, Edit flow, Ask Noyan ⌘L, Pause shop-web ⇧⌘., Settings ⌘, and Open a terminal ⌃`.
- **A**: action names change: Change flow… → "Edit flow", "Open / close Noyan" → "Ask Noyan" (`crates/otaq-view/src/keymap.rs:202`), "Open a terminal here" → "Open a terminal".
- **D**: Recent items, "Other projects", the `>` hint, and the "Nothing matches" help aren't shown. They're probably just out of frame.

---

## E: conflicts (collected)

1. **"Your folder and your current branch are never touched"** (onb-03) is false: Local merge fast-forwards the base checkout (`crates/otaq-core/src/git/merge.rs:16`).
2. **Sign-in without a terminal** (onb-02) conflicts with interactive installs and logins: passwords, device codes and Homebrew prompts are answered in the row's terminal today.
3. **Limit banner without Resume** (window-01 #4). If it implies an automatic resume at reset, that contradicts "Resume is the user's click and never happens by itself" (`VM/limit_banner.rs:9-10`).
4. **"Dev: Light instead of Strong"** reads like a substitution. It must not be read as a fallback agent (no-fallback rule).
5. **"Done · Merged into your branch"** on Welcome hides the `delivers: either` PR path of the default flow. A small copy issue.

No conflicts found with: Noyan intake-only (Noyan appears only as "Ask Noyan ⌘L"), one agent per check stage (Validate shows one "Verifier"/checker), "the flow YAML alone defines behaviour", or skill copying. None of these appear in this slice.

## Open product questions

1. Projects added after first run: do they still open Flow setup and the flow editor, or start on the default flow like first run does? (Today: Flow setup, `UI/onboarding/logic.ts:86-89`.)
2. gh/az are machine-wide tools. Should they really live in each project's "Repository" settings? And where is a missing gh/az reported when a ticket opens a PR?
3. Are "Agent tools" and "Agents" merged into one Settings → Agents section? The design says "add more tools later in Settings → Agents".
4. What does the onboarding Sign in… do when the tool asks for a code or password? Is the terminal still reachable?
5. Where do the permission-mode warning, the folder-trust disclosure, the usage-data question and the theme choice go in first run?
6. Does `/` search still filter the board, or only list tickets? Do results include Done/archived tickets of the current project only, or of all projects?
7. Limit banner: is Resume removed, hidden until the reset time, or automatic? Is the ≤20% usage warning dropped?
8. Paused banner: how is "Resume all projects" offered, and what does the title bar show for another project that is still running?
9. Rail: is List view dropped? Does "Settings" in the rail mean the project's settings or the global ones? Where do Cleanup, Noyan and the update dot go?
10. Project switcher: is ⌘P kept, and do the per-project counts and needs-you ring survive anywhere?
11. Usage % in Running: used or left?
