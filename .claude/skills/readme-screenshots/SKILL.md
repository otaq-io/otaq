---
name: readme-screenshots
description: Refresh Otaq's README.md and its screenshots in docs/images/ from the real running app. Sets up a throwaway demo project (its own OTAQ_HOME, a small git repo, tickets, a ticket with a Plan of pieces, real Claude Code agents), stages a board with work in every column, captures the app window on macOS, then updates the README text and images to match. Use this whenever the user wants new or updated README screenshots, says the screenshots or README are out of date, wants images of the app for docs, a release, a launch post or the website, or asks to "show the app" in the README, even if they don't mention the demo project or screenshots by name.
---

# README screenshots for Otaq

Real screenshots beat mockups: the README should show the app as it is, with real agents doing real work. The
catch is that a live board takes some staging, and macOS, the sandbox and Claude Code's own environment each have a
trap. This skill is the route that works; the scripts in `scripts/` hold the fiddly parts.

**Cost and duration:** about 15–20 tickets run real `claude` agents on the user's plan (each takes 1–3 minutes),
and the whole session takes 15–45 minutes. Say so before starting if the user hasn't asked for it explicitly.

`S=.claude/skills/readme-screenshots/scripts` below. Demo data lives in `$OTAQ_DEMO` (default `/tmp/otaqdemo`),
never in `~/.otaq`, so the user's own boards are never touched.

## 1. Preconditions

- **macOS only** (`screencapture`, CoreGraphics). Commands that start the service, open the app, click or capture
  need the sandbox off (`dangerouslyDisableSandbox: true`); without it the agents can't save their conversations and
  `screencapture` can't see the screen.
- **Screen Recording permission** for the terminal that runs Claude Code: `swiftc -O $S/window.swift -o
  /tmp/otaqdemo/bin/window && /tmp/otaqdemo/bin/window --access`. If it says NOT allowed, macOS has just shown its
  prompt: ask the user to allow it in System Settings → Privacy & Security → Screen & System Audio Recording. A
  terminal app may need a restart to pick it up (`claude --continue` resumes). Clicking needs no Accessibility
  permission: the scripts post CoreGraphics events.
- The user's `claude` CLI is logged in; Node (24 per `.nvmrc`; 20 has worked too) and Rust are installed (the app
  builds from this checkout).
- Keep `$OTAQ_DEMO` short: the service socket path must fit in ~104 bytes, so a deep scratchpad folder fails
  with "path must be shorter than SUN_LEN".

## 2. Start the app and seed the demo

```sh
zsh $S/start.sh
```

It builds `otaq` and the app, starts the demo service with a clean environment, seeds the demo on the first run
(`seed.sh`: the `pocket-notes` repo, project NOTE, 13 tickets including "Tags" NOTE-7 and "Notebooks" NOTE-13, each
with a Plan of four pieces; nothing starts yet), then starts Vite and opens the window. Seeding comes before the window because the app
chooses between onboarding and the board once, at launch: if you ever see the onboarding wizard, run `start.sh` again.

Why `start.sh` does what it does, in case something needs adjusting by hand:
- **Clean environment** (`env -i`): a service started from inside Claude Code inherits `CLAUDE_CODE_CHILD_SESSION`
  and passes it to every agent; their terminals then say "Transcript saving is off", Resume fails with
  "conversation not found" and token counts stay at 0. If you see that warning in an agent terminal, the service was
  started wrong: stop it and run `start.sh` again.
- **No `npm run tauri dev`**: its watcher restarts the app whenever any file in the repo changes, and other sessions
  commit while you shoot. The app runs from a fixed debug build against the Vite server instead.
- **The service and the app are built from the same tree**, so no "Service update ready" banner appears in shots.

## 3. Stage the board

The board is worth capturing when every column has something in it and something is visibly happening. Agents on
this demo finish in about a minute, so stage it in waves rather than starting everything at once:

1. Wave 1: `NOTE-1`, `NOTE-2`, `NOTE-4`, `NOTE-5` to Dev; `NOTE-3` to Ready for Dev (it waits for NOTE-1). Wait:
   `zsh $S/wait.sh "Ready for Review" NOTE-1 NOTE-2 NOTE-4 NOTE-5` (about 5 minutes).
2. Merge in two passes, because a ticket only starts once the tickets it waits for are merged:
   `zsh $S/merge.sh NOTE-1`, then `wait.sh "Ready for Review" NOTE-3`, then `merge.sh NOTE-3`. Done fills.
3. Drag `NOTE-7` (Tags) to Dev. Its agent is in charge: it reads the Plan and may start **helpers**, one per piece,
   each in its own worktree; their rows show inside the card ("7.1 merged · 7.2 working") and in the ticket's
   Overview next to the Plan's pieces. Open the ticket and capture its Overview while at least one helper is working and one is merged, as the plan shot. A helper's question
   goes to the agent in charge first; one it passes on shows in the Needs you strip as "NOTE-7 · 7.2 asks". Leave
   NOTE-7 running; it reaches Ready for Review as one ticket when its pieces are merged.
4. Wave 2, right before the board shot: `NOTE-8`, `NOTE-10`, `NOTE-12` and `NOTE-13` (Notebooks) to Dev, `NOTE-9` to
   Ready for Dev. Capture within the next minute or two, while cards still say Working or Validating.

**Approved stays empty**, and that's fine: only the Open PR flow puts cards there, and the demo repo has no remote
(`ticket.move` to `approved` answers "Approve isn't available yet").

If a card shows **Start interrupted**, retry it: `ticket.retry_start {"key":"NOTE-8"}`. `wait.sh` stops early and
names the ticket when that happens.

The requests:

```sh
R=(python3 -I $S/otaq_req.py)                                           # an array: zsh doesn't split strings
"${R[@]}" ticket.move '{"key":"NOTE-1","target":"dev","confirmed":true}'      # also ready_for_dev, backlog, ...
"${R[@]}" ticket.pin '{"key":"NOTE-2"}'
"${R[@]}" ticket.start_from_scratch '{"key":"NOTE-1","confirm":true}'          # after "Resume failed"
python3 $S/otaq_req.py theme otaq-dark                                  # otaq-light, daylight, tokyo-night, dracula, ...
```

"Conflicts with main" on review cards is normal here (the tickets touch the same few files) and looks realistic;
leave it.

## 4. Capture

```sh
zsh $S/shot.sh /tmp/otaqdemo/shots/board.png                       # just capture
zsh $S/shot.sh /tmp/otaqdemo/shots/x.png 1414 70                   # click (1414,70) in the window, then capture
zsh $S/shot.sh /tmp/otaqdemo/shots/x.png scroll 180 600 -15 wait 2 # scroll a column down, wait, capture
```

Coordinates are pixels in an earlier shot of the same window (window-relative; the window is 1440×900 by default
on a 1x display). `shot.sh` first clicks the header's top edge, because the first click on an inactive window only
activates it; at the end it moves the pointer below the window, because a hovered card dims the rest of the board.
Look at each shot with Read before moving on: a click can still miss, a panel may be open, a tooltip may show, and
once in a while macOS hands back a scaled-down window (`shot.sh` warns about the size): just capture again.

Shots the README uses (keep these names so the README's image links stay valid):

| File | What it shows | How |
|---|---|---|
| `board.png` | the hero: Otaq Dark, cards in every column but Approved, something working (NOTE-7's helper rows) | theme `otaq-dark`, all panels closed, right after wave 2 |
| `board-light.png` | the same board in a light theme (Daylight or Otaq Light) | switch theme, capture again |
| `terminal.png` | an agent's Claude Code session, ideally explaining a recorded decision | **Review** on a Ready for Review card; Terminal tab |
| `review.png` | agent summary, a recorded decision, the validation report | same panel, Diff tab, top |
| `diff.png` | the code diff with the file list | expand the panel (⤢ in its header), collapse the three sections above the diff |
| `plan.png` | a ticket's Overview with its Plan: the pieces in order, each with its helper's status | step 3 above |

Useful places in the default 1440×900 window: a panel's close ✕ at (1414, 70), expand ⤢ at (1378, 70); with a
review card's panel open, its tabs sit on one row around y≈235–280 (Diff at x≈911). For `diff.png`, collapse the
summary, decisions and validation sections **bottom-up**, so the ones above don't move before you click them. Recheck
against a fresh shot: layouts change between versions, so treat these as hints, not truth.

Things to keep out of shots: the "Service update ready" banner, "Transcript saving is off" in a terminal, personal
paths outside `/tmp/otaqdemo`. The header's plan meter ("5h 5% · week 35%") shows the user's real usage; mention it
when you present the shots.

## 5. Update the README

Copy the picks into `docs/images/` with the names above, then read `README.md` against the shots and the current code:

- Every caption and sentence should describe what the image actually shows (ticket keys, panel names, button labels).
- Check claims against the code, not memory: CLI commands (`target/debug/otaq --help`), themes
  (`crates/otaq-view/src/theme.rs`), setup steps (`crates/otaq-desktop/README.md`). Features change between shoots.
- Link only to files git tracks. `decisions/`, `DESIGN.md`, `SDD.md`, `design-briefs/`, `tickets/` and `prompts/` are
  the maintainer's local notes and are not in the repository. `zsh $S/check_links.sh` checks every link and image.
- Images are ~250 KB each as captured; that's fine for six or so.

## 6. Clean up and hand over

`zsh $S/stop.sh` stops the demo's agents, the app and Vite, so nothing keeps spending the user's plan. It keeps
`/tmp/otaqdemo` for a reshoot; `rm -rf /tmp/otaqdemo` removes it.

The demo stays out of `~/.otaq`, but not entirely out of Claude Code's own config: adding the project records
`/private/tmp/otaqdemo/pocket-notes` as a trusted folder in `~/.claude.json`, and every agent run adds Claude Code's
usual per-folder entries there. They're harmless; mention them when you hand over rather than editing that file,
which every running `claude` (including this session) rewrites.

Tell the user which shots you took and what changed in the README, and flag anything odd you noticed in the app while
shooting: this is a real end-to-end run, and it finds real bugs. Commit or push only when the user asks.
