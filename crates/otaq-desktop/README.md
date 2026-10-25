# otaq-desktop

The Otaq desktop app on Tauri 2: a window whose page is React with TypeScript, built with Vite (decisions 0074,
0075; SDD §5.5). Its binary is `otaq-app`, the name the GPUI app had (T-0245), so `otaq` with no arguments
opens it and the release archive keeps its layout (d0072).

> References such as "decision 0075", "d0072", "SDD §5.5", ticket numbers (T-0215), requirement IDs (UIT-02)
> and design frames (`crewboard.pen`) point to the maintainer's design notes, which are not part of this repository.

## Where things are

The frontend sits inside this crate (the implementation choice SDD §5.5 leaves open), so one folder holds the whole app
and every `npm` command runs from here:

| Path                                                                                                      | What                                                 |
| --------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| `Cargo.toml`, `build.rs`, `src/`                                                                          | the Rust side                                        |
| `src/door.rs`                                                                                             | the only Rust module that names Tauri (UIT-02)       |
| `src/window.rs`                                                                                           | the window: title bar, minimum size, its kept place  |
| `tauri.conf.json`, `capabilities/`, `icons/`                                                              | Tauri's config: the CSP, the window's permissions    |
| `package.json`, `package-lock.json`, `.nvmrc`                                                             | the npm packages and the Node version (24)           |
| `tsconfig.json`, `tsconfig.node.json`, `eslint.config.ts`, `types/`, `.prettierrc.json`, `vite.config.ts` | the TypeScript rules (UIT-08) and the build          |
| `ui/index.html`, `ui/src/`                                                                                | the page                                             |
| `ui/src/tauri.ts`                                                                                         | the only TypeScript file that names Tauri (UIT-02)   |
| `ui/src/generated/`                                                                                       | the types shared with Rust, generated with `ts-rs`   |
| `ui/src/app/`                                                                                             | the frame, the registries, the shared selection      |
| `ui/src/components/`                                                                                      | the shared components every screen draws with        |
| `ui/src/cards/`                                                                                           | the Ticket Card                                      |
| `ui/src/screens/`                                                                                         | one folder per screen, and the screens index         |
| `ui/src/terminal/`, `src/terminal.rs`                                                                     | the terminal component every terminal screen uses    |
| `src/api/`                                                                                                | the request table and one file per area              |
| `ui/dist/`                                                                                                | the built page (git-ignored), Tauri's `frontendDist` |

The app icons in `icons/` come from the design's _App Icon 96_ (Command tent, d0076): exported from `crewboard.pen`, put
on a 1024 px canvas (macOS: an 824 px body; Windows and Linux: a tighter crop) and run through `npm run tauri icon`.

## Commands

Use Node 24 (`nvm use` in this folder reads `.nvmrc`). Nothing is installed globally; the Tauri CLI is the
`@tauri-apps/cli` dev dependency.

```sh
npm ci                  # install the locked packages
npm run tauri dev       # the app with the Vite dev server (live reload)
npm run tauri build     # a release build with the page inside the binary: target/release/otaq-app
npm run check           # typecheck, lint, format:check, test (each also runs on its own)
```

## How a screen plugs in

The page is a fixed frame (`ui/src/app/Frame.tsx`, _02 · App shell → Window anatomy_) whose parts are filled by
screens. A screen touches only its own folders plus **one line in the screens index** and, if it calls the service,
**one line in the request table**. Everything it can fill is listed, typed, in the registries (`ui/src/app/registry/`):

| Registry                          | Ids (fixed in)                                                                                                             | A screen registers                                 |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| `app.slots`                       | `slots.ts`: title bar ends, header, header search, header pause, rail, banners, agent panel, board, Ask bar, sheet, toasts | the component that fills the slot                  |
| `app.overlays`                    | `overlays.ts`: every dialog, popover and drawer, with what it is opened with                                               | its overlay's component (gets `params`, `close`)   |
| `app.sheetTabs`                   | the generated `SheetTab` (Rust `sheet::SheetTab`), in the order of `sheet.ts`                                              | its tab's body (gets the open `ticket`)            |
| `app.sheetSections`               | `sheet.ts`: Overview's sections (Stack) and the open PR's box under its top                                                | its section (gets the open `ticket`)               |
| `app.settings.global`, `.project` | `settings.ts`: the global and the project sections, in nav order                                                           | its section (a project section gets the `project`) |
| `app.shortcuts`                   | the generated `KeyAction` (Rust keymap)                                                                                    | a handler per action, `app.shortcuts.on(...)`      |

Each id has one owner: registering one twice throws. The ids are unions, so a `switch` over them without a `default`
is checked by ESLint's `switch-exhaustiveness-check`. A new overlay id, settings section or slot is one line in its
registry file; a new sheet tab or key action comes from Rust (regenerate the types).

**1. The screen's folder.** `ui/src/screens/<area>/index.tsx` exports one `Screen`, a function that registers its
parts; its components and logic sit next to it. `ui/src/screens/_sample/` is the template:

```tsx
import type { Screen } from "../../app/registry";

export const strip: Screen = (app) => {
  app.overlays.register("resume_all", { Component: ResumeAll });
  app.shortcuts.on("next_needs_you_item", next);
};
```

**2. One line in the screens index**, `ui/src/screens/index.ts`: `export { strip } from "./strip";`. The page runs
every screen exported there once at start.

**3. One line in the request table**, if it calls the service: a file `src/api/<area>.rs` with
`pub async fn request(app, method, params)` matching its own full method names (`"strip.view" => view(app,
params).await`) and ending with `_ => Err(unknown(method))`, plus its name in the `areas!` list of `src/api/mod.rs`.
The page calls it with `request("strip.view", params)` from `ui/src/tauri.ts`. Its reply types get the `TS` derive and
one line in `export_all` (below).

**Shared state.** Interaction state screens share lives in two small stores; everything else comes from the service
(`useServiceView` in `ui/src/service.ts`) or stays inside the component (UIT-04):

- `ui/src/app/selection.ts`: the project on screen, the ticket open in the sheet and its tab, the focused terminal.
  Read it with `useSelection()`; change it with `selectProject`, `openTicket`, `closeTicket`, `showTab`,
  `focusTerminal`. The sheet slot shows while a ticket is open.
- `ui/src/app/overlays.ts`: the open overlays, newest last. Any screen opens any overlay by id: `openOverlay("resume_all")`,
  `openOverlay("edit_ticket", { ticket: "SHOP-22" })`, `toggleOverlay`, `closeOverlay`. An overlay that isn't
  registered yet draws nothing. `modalOpen()` says whether a modal one holds the app's shortcuts back (`modal.rs`).

**Shortcuts.** T-0215's dispatcher turns a key into a `KeyAction` and calls `app.shortcuts.run(action)`; the newest
handler for that action runs. A handler returns `false` to leave the key to the one registered before it (an open
overlay's Esc before the sheet's). A component whose key acts on it while mounted (Find in a focused terminal) uses
`useShortcut(name, handler)` instead of registering at start.

**Keys.** The dispatcher (`ui/src/app/keys.ts`, installed by `App`) takes the §21.3 map from Rust (`keymap.map`, built
from `otaq-view`'s keymap), so no TypeScript file lists a shortcut. While the selection's `focusedTerminal` is set,
every key goes to the terminal except the app's (⌘ on macOS, Ctrl+Shift on Windows and Linux); a terminal component
calls `focusTerminal(session)` on focus and `focusTerminal(null)` on blur. Leave terminal focus blurs it when no screen
handles it. Outside a terminal, bound keys that no handler takes, and keys the webview would act on (reload, print,
find, zoom, devtools, caret browsing, history), are swallowed; a focused text field keeps Enter, Backspace and Esc. The
app started with `OTAQ_KEY_LOG=1` shows the last keys and where each went, bottom right.

**Testing a screen.** `registerScreens({ strip })` gives fresh registries with only that screen; `registered(app)`
lists what it filled (see `ui/src/app/registry/registry.test.ts`). The frame renders without a DOM through
`react-dom/server` (`ui/src/app/frame.test.ts`).

## The terminal component

`ui/src/terminal/Terminal.tsx` is the one terminal of the app (T-0216, UIT-05, SES-09): Session's agent, check,
Run and Shell, Project terminals and Project agents all show a session with it. It is **xterm.js 6** with its WebGL, fit, search,
web-links and Unicode 11 add-ons, inside the frame (it needs the registries' context for its Find shortcut):

```tsx
import { Terminal, type TerminalHandle } from "../../terminal/Terminal";

const terminal = useRef<TerminalHandle>(null);
<Terminal
  session={session.id} // another id attaches anew
  label="SHOP-19 · agent terminal" // its accessible name
  readOnly={false} // true: output only, nothing typed is sent (Run tab)
  onState={setTerminalState} // phase, focused, scrolledUp, newLines, fontSize: for the header and hint strips
  ref={terminal} // terminal.current?.focus() / .openFind() / .jumpToLatest() / .changeFontSize(±1)
/>;
```

It fills the box its host gives it: the find bar, the terminal in the theme's terminal colours, the "142 new lines"
pill while scrolled up, and the "Reconnecting… agents keep running" strip. The strips around it (session state, path,
attach command, the hint line under it) are the host's, drawn from `onState` (`TerminalState` in `state.ts`).

- **Output.** It attaches over its own door channel (`attachTerminal` in `ui/src/tauri.ts` → `attach_terminal` in
  `src/door.rs` → `terminal::attach`). Rust subscribes to the service's events before it asks for the attach, sends the
  stored scrollback (`replay`), then live output cut to start exactly where the shown output ends (`live`), so no byte
  shows twice or goes missing; and the `phase` (`attaching`, `live`, `reconnecting`, `ended`, `no_session`). After a
  reconnect, or when live output went missing, Rust attaches again and the terminal redraws from the new replay. It
  detaches on unmount (`terminal.detach`).
- **Input.** What is typed or pasted goes out with `terminal.input`, one request at a time and in order with the
  resizes (`createSender` in `input.ts`). The terminal's own answers to the program (cursor reports, colours, mouse
  reports) are sent as replies, so they don't touch the service's unsent-input flag (d0048); answers to questions in a
  replay aren't sent at all. Input is off unless the phase is `live`, so while reconnecting the scrollback stays and
  nothing is typed (SVC-04).
- **Dropped files** (d0099, T-0388). Each terminal registers its well in `dropTargets` (`drops.ts`) while mounted. The
  door forwards every file drop as `files_dropped` (`src/drops.rs`: the paths, the paste quoted as Terminal.app does,
  and the point in CSS pixels); the page looks up the element at that point: a typeable, live terminal gets the paste
  through `term.paste` (bracketed, as ⌘V) and the keyboard, one that doesn't take typing ignores it, anywhere else Add
  project opens with the first path.
- **Focus.** On focus it calls `focusTerminal(session)` and tells the service (`terminal.focus`, passed on for an
  agent's session only, ATT-03); on blur `focusTerminal(null)`. Keys go to xterm.js untouched except the app's own
  (T-0215) and the terminal's own from the Rust map (`keymap.map`'s `terminal_keys`): copy and paste on Ctrl+Shift+C /
  V on Windows and Linux (on macOS the Edit menu's ⌘C / ⌘V reach xterm.js), ⌘+ / ⌘− (Ctrl+Shift+= / −) for this
  terminal's font size until the setting changes, and ⇧↵, which types ESC + CR (as ⌥↵) instead of xterm.js's `\r`, so
  it starts a new line in the agent's prompt instead of sending it (OT-5). ⌘F / Ctrl+Shift+F opens the find bar
  (`useShortcut`).
- **Settings.** `terminal.settings` gives the font size (8–32), the font stack and the renderer, asked again on
  `settings_changed`. The font stack (d0117 point 5) is the Terminal font (Geist Mono by default, or when the chosen one
  is no longer installed), then every installed Nerd Font for prompt icons, then `monospace`; the installed families are
  read once per start with fontdb (`src/fonts.rs`). A font change reaches every open terminal at once, scrollback kept,
  and its glyph atlas is cleared. The
  renderer (`terminal_renderer` in the global settings: WebGL or DOM; DOM by default on Linux, WebGL on macOS and
  Windows, Q37) picks xterm.js's renderer for every terminal; only a WebGL context that fails to start falls back to
  DOM, because a broken driver that draws garbage raises no error. Its row in Settings → Appearance is T-0234's (Q38).
- **Theme.** The colours are `terminalColors(...)` (T-0213), read again whenever the frame swaps `data-theme`.
  Selected text is drawn in `term-fg` on the `selection` tint, focused or not (d0117 point 2).
- **Drawing** (d0117, T-0481; `drawing.ts`). Options of the terminal itself, so the WebGL and DOM renderers draw alike:
  a contrast floor of 4.5 (`minimumContrastRatio`, WCAG AA: xterm.js shifts a text colour too close to its background
  at draw time, the theme's values stay), Option-drag always selects (`macOptionClickForcesSelection`), and character
  widths by Unicode 11 as the tools count them, set before anything is written, the replay included.
- **Links** (d0117 points 7 and 9, T-0484; `links.ts`). Every link opens with ⌘-click (Ctrl-click on Windows and
  Linux) only; a plain click or drag selects. One gate wraps every link provider: it opens a link only on that gesture
  (`isOpenGesture`), and underlines it, with the hint "⌘-click to open", only while the key is held. Plain `http(s)`
  links come from the web-links add-on, links programs hide behind text (OSC 8) from xterm.js's own provider (wrapped
  in place, `gateHiddenLinks`), which keeps http and https links (their address shows as a tooltip); `file:` links
  wait for T-0485, other schemes are no link. Web links open in the system browser (`terminal.open_link`).
- **Copying from programs** (d0117 point 7, T-0510; `clipboard.ts`). xterm.js's clipboard add-on reads OSC 52; Otaq's
  provider (`otaqClipboard`) puts a copy (Claude Code, vim, tmux) on the clipboard (`navigator.clipboard.writeText`),
  never from the replay, and ignores a clear. It reads nothing: a program asking for the clipboard gets an empty
  reply. The add-on bundles `js-base64` (BSD-3-Clause, accepted in d0117 as highlight.js was in d0078).

## Components

The shared components (_crewboard.pen → 01 · Components_, T-0217) are in `ui/src/components/`; a screen imports them
from the folder, never from a file inside it: `import { Button, ConfirmDialog, StatusPill } from "../../components";`.
They are drawn with the theme's CSS variables only (`components.css`) and take typed props: what a view model says
comes in as its generated type (`PillView`, `PriorityView`, `MenuItem`, `ThemePickerSection`), and the component keeps
only the moment of interaction (UIT-04). Their keyboard and placement rules are plain functions in `logic.ts`, tested
without a DOM.

| Component                                                       | Props, briefly                                                                       |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `StatusPill`, `MetaChip`, `CountBadge`, `Kbd`                   | a `PillView` (`status.rs`); icon + label + tone; count + tone; the keys' text        |
| `Button`, `IconButton`                                          | label, variant, icon, `compact`, `disabled`, `loading` (ignores presses), `onClick`  |
| `PriorityChip`, `PriorityLabel`                                 | a `PriorityView` (`priority.rs`)                                                     |
| `Tabs`, `Segmented`, `Toggle`                                   | items with icon, label, indicator; options; on/off. ←/→ move between them            |
| `Field`, `TextInput`, `TextArea`, `Select`, `Checkbox`, `Radio` | label, hint, error (`fields.rs` says which shows); `Select` takes `MenuItem`s        |
| `Menu`, `AnchoredMenu`, `Popover`, `Tooltip`                    | `MenuItem`s (`menu.rs` rules: arrows skip disabled rows and wrap); an anchor         |
| `Dialog`, `ConfirmDialog`                                       | title, body, confirm label, `destructive` (↵ = Cancel); traps focus, Esc cancels     |
| `Banner`, `Toast`, `StackList`, `StackRow`                      | tone, icon, title, text, actions; the Stack list's row shell                         |
| `ThemePicker`, `useThemePicker`, `pickTheme`                    | the picker's sections (`theme.picker`); a click applies and saves (`theme.pick`)     |
| `OtaqMark`                                                      | `size` (px), optional `label`; the Command tent (d0076), thicker strokes below 24 px |

Any screen asks for a confirmation without drawing one: `openOverlay("confirm", { title, body, confirmLabel,
destructive, onConfirm })`; the components screen (`ui/src/screens/components/`) draws it with `ConfirmDialog`.
Menus, popovers and tooltips are fixed-position layers inside their anchor's subtree, so a scrolling parent never cuts
them and the anchor's theme applies.

**A new dropdown, menu or popover** uses `Select`, `AnchoredMenu` or `Popover`. One that can't places itself with
`useFloatingStyle(anchor, floating, align, onClose)`: passing `onClose` is what closes it on a press outside it and on
a scroll behind it. A list that opens in place (not floating) closes with `usePressOutside(box, null, onClose)`. Never
close a layer only on its blur: a list that never had the keyboard never loses it, and a press on the header's drag
region moves no focus, so it stays open (`Floating.test.ts`, `screens/settings/dropdowns.test.ts`).

**The gallery.** Start the app with `OTAQ_GALLERY=1` (`OTAQ_GALLERY=1 npm run tauri dev`) and the design
gallery opens over it: every foundation and component next to each other, the token swatches of Otaq Light and
Dark, the Status Legend, the keyboard map, the theme picker (it saves) and a palette switch over the 15 themes (it
doesn't). Esc or its close button leaves it. A new shared component gets a place in it
(`ui/src/screens/components/Gallery.tsx`).

## Cards

The ticket card (_24 · The two views_; the one-line card of BRD-03 since T-0460) is in `ui/src/cards/`:
`import { TicketCard, stateOfLook } from "../../cards";`. It draws a `TicketCardView`, which
`otaq_view::view_models::card_page` builds from the card view models with every text already worded: the key, the
title, one status line (its words and its dot's colour, `card::status_line`), who and when, the coloured edge of a card
that needs you and its one action (Answer, Reply, Review, Decide…); the key row's hover has the full status and what
the card has no room for. A Check stage with several check agents adds a row per check agent (`CheckRound.tsx`,
`check_agents.rs`); a build stage's pieces (d0133) add "Plan · 4 pieces" under the line and the PIECES rows, one per
piece with its name, title and status (`Pieces.tsx`, `pieces.rs`). The pill, the model, the tokens and the answer
options are the ticket's sheet's. The board passes the moment of interaction as a `CardState` (selected, chain
highlighted or dimmed, dragging, hovered). `look.ts` turns each union of the view (tone, palette, gallery look) into
theme colours with an exhaustive switch; a variant added in Rust fails the linter and
`cards.test.ts` until it has a drawing. The gallery shows every fixture of `card_fixtures.rs`
(`gallery.cards`) at the card width and at 1280.

## The ticket sheet

The ticket opens as a sheet (_25 · The ticket view_, SHT-01, SHT-02, T-0464): `ui/src/screens/sheet/` fills the sheet
slot, served by `src/api/sheet.rs`, whose `sheet.view` carries the sheet's header, details line, flow path and tabs
(`sheet`, worded by `otaq-view`'s `sheet.rs`) beside the ticket's action bar, "…" menu, answer options, queued notice,
conflict strip and Start interrupted callout (Overview's top, `ui/src/screens/overview/NeedsYou.tsx`). The ticket's
actions are the sheet's (`sheet/actions.tsx`): buttons that talk to the service go back through `sheet.action` (Start,
Retry start, Resume, Reopen, Mark merged, Open PR in browser), which shows a refusal as a toast; the rest open a tab
or an overlay (`sheet/actionRoutes.ts`, `actionRoute` / `menuRoute`). Move… asks `sheet.move_menu` (the drag's
columns) and drops like a drag. The sheet keeps only its full page state (`sheetExpanded`), open menus and the picked
answer. The selection's `tab` names what the sheet shows, the generated `SheetContent` (Rust `sheet::Content`): a tab
(`overview`, `files`, `activity`, `lead`) or a side of Session's switch (`agent`, `check`, `run`, `shell`); `null` is
the tab it opens on. Each sheet tab lists the contents it holds and the one it lands on.

- **A sheet tab:** register its body (`app.sheetTabs.register("session", …)`) from its own screen; it gets
  `{ ticket }`, and `useSheet()` gives the open view, its actions and the selected content.
- **An Overview section:** register it (`app.sheetSections.register("stack", …)`); it gets `{ ticket }` and draws in
  Overview.
- **Edit:** the menu's Edit ticket shows Overview and adds a `ticketEditRequests` entry (`ui/src/app/selection.ts`)
  Overview answers by opening its form.
- **Dialogs opened from the sheet:** `close_ticket` and `submit_for_review` are the sheet's; `local_merge`, `send_back`,
  `send_back_pr`, `start_from_scratch`, `open_pr` and `delete_ticket` are opened with `{ ticket }`, each drawn by its
  own screen.

## Session

The sheet's Session (canvas `s25-06`, SHT-04, T-0466) is `ui/src/screens/session/`: the switch **Agent · <check's agent>
· Run · Shell** (`SheetView.sheet`'s Session `parts`, worded by `otaq-view`'s `session_tab.rs`; the check's side shows
once a check stage ran, named after its agent) leads the line of the side shown. The agent's and the check's terminal
are served by `src/api/terminal_tab.rs`: `terminal_tab.view` gives the agent session to attach (the running one, else
the newest) and the check's (`check`), the line ("Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-22",
"Typing goes to Rust developer"), the strip's texts (`$PORT`, the attach command, "1 message waiting · Deliver now",
font size and find tooltips), the "Instructions: shop-web" link, the end marker of an ended session and, for an
Interrupted ticket whose session is gone, "Before the restart" (REC-02). The state label and the hint line depend on
the terminal's phase and focus, which only the page knows, so the view carries one `strips` entry per phase and focus,
worded by `otaq-view`'s `terminal_strip.rs`, and the terminal picks the one it reports (`terminalLogic.ts`). Deliver
now goes through `terminal_tab.deliver_now`, which answers like `sheet.answer` (done, or the unsent-text question).
Run (`run.tab`) leads with "pnpm dev · in SHOP-22’s worktree · :4022" and Open URL; Shell is `run.shell`'s.

- **Focus requests:** a `terminalFocusRequests` entry for the shown ticket (a notification click, the bar's Open
  terminal / Answer in terminal) focuses the shown side's terminal once it is live, for at most five seconds; the
  caller picks the side with the selection (`agent`, `check`).

The sheet's Activity (SHT-05) is `ui/src/screens/activity/`: `sections.history`'s rows oldest first, each with its
kind, under the filters All · Comments · Stages · Description (`history.rs`).

## The Needs you strip and ⌘J

The Needs you strip (_24 · The board_, NYS-01…NYS-03, d0114, T-0459) is `ui/src/screens/strip/`, served by
`src/api/strip.rs` and worded by `otaq-view`'s `needs_you_strip.rs`. The Board and the List each render
`<NeedsYouStrip />` under their toolbar; it shows only the open project's items, one chip each, in the order of the
service's queue (`needs_you.list`, d0016's groups, unchanged). Requests: `strip.view` (the chips, the count, Next),
`strip.next` (where ⌘J goes: the next item here, else the next project with items), `strip.resume_all_confirm` and
`strip.resume_all` (Resume all's confirmation and run).

- **Overlays:** `resume_all` (Resume all's confirmation with its tickets, also opened by the "sessions were
  interrupted" banner).
- **⌘J** (`next_needs_you_item`) works from anywhere, a focused terminal included: the next item opens with its terminal
  focused (`requestTerminalFocus`); after the open project's last item it switches to the next project with items. It
  never moves on by itself (ATT-07).

## Terminals

The drawer (_17 · Project terminals_, T-0242, T-0331) is `ui/src/screens/terminals/`, served by `src/api/drawer.rs`. It
holds Project terminals only (d0087; Project agents live in the agent panel, d0113): `drawer.view` gives the shown project's tabs
(terminals by number, from `otaq-view`'s `project_terminal.rs`, each with its hint and ended line) and the tab to show.
Opening a tab goes through Rust, which keeps each project's last-used tab: `drawer.last_terminal` (the rail's Terminals,
⇧⌘T) and `drawer.new` (+, a new terminal) each answer the tab to show, which the page passes to `requestTab`
(`logic.ts`). The drawer is the `terminals` overlay, fixed across the bottom of the window; while it is open it pads the
frame so the board ends above it.

## Banners and toasts

The banner area and the toasts (_crewboard.pen → 11 · Banners, toasts & OS notifications_, T-0231) are kept on the
Rust side, in `src/notices.rs`: which banners show (at most two rows, one per situation), their wording and buttons,
the toasts and their time-outs. The page draws `notices.view` and each `notices` event (`ui/src/screens/notices/`).

- **A toast from a screen:** `request("notices.toast", { level, title, text, link, ticket })` (a `NewToast`), with
  wording from a view model or the service's reply. `link` is a `copy` link (`Copy command`) or an `open` link to a
  ticket; `ticket` makes the toast's title open that ticket.
- **A toast for a service event:** one function in `EVENT_TOASTS` (`src/notices.rs`) per event kind, turning the
  event into a `NewToast` or passing.
- **Restart required:** `onlyRestartAllowed(useStore(noticesStore))` from the notices screen says when everything
  except Restart is disabled.
- **Notification clicks** bring the window forward and land in the selection store: the project and the ticket on its
  Session tab, plus a `terminalFocusRequests` entry (`ui/src/app/selection.ts`) Session answers by focusing the
  terminal; a grouped notification goes to Needs you (the strip's next item, as ⌘J).

## View preferences

How a project's board looks (Board or List, Lanes, Group by, Fold empty columns, Hide empty stages, Dependencies, the
folded columns, lanes and sections) is kept by the service in `ProjectSettings.view` (SHB-05), never in the page.
`board.view` reads it and sends it with the board (`prefs`, and the `toolbar` worded from it,
`otaq_view::view_models::view_prefs`); the page changes one thing at a time with `board.set_view` (a `ViewChange`,
stored with `project_view.set`) and asks for the board again. The page keeps only where each column was scrolled
(`screens/board/prefs.ts`).

## Search, the card menu and deleting

The board screen (`ui/src/screens/board/`, T-0220, T-0221) also fills the `header_search` slot: the header draws that
field in place of its search entry. The filter is kept per project while the app runs (`filter.ts`) and sent with
`board.view` and `board.search`; Rust parses the tokens and words the field. The card's right-click menu asks
`board.menu` for its items and `board.menu_choose` for what a choice does: Pin, Unpin and the priorities are sent at
once, the others open their dialog by overlay id (`delete_ticket`, `close_ticket`).
**Every place that deletes a ticket** (the card menu, the sheet's "…" menu, the Project agent's summary) opens the one
delete confirmation: `openOverlay("delete_ticket", { ticket })`.

## Settings

Settings (⌘,, the `settings` overlay, T-0234) lives in `ui/src/screens/settings/`. `settings.view` sends the nav and
the open page (global or one project) as sections of rows, each row worded with its control
(`otaq_view::view_models::settings_page`); `settings.edit` takes one `field` and its value, applies it with that
module's rules and answers with the fresh view. The screen registers one component per section id it draws
(`ui/src/app/registry/settings.ts`); a later section is added in `settings_page.rs` and registered under its id the same
way (T-0240: `validation`, `project_agent_instructions`). The slow parts load on their own: `settings.tools`,
`settings.provider`, `settings.branches`.

## Shared types

The types the page reads are generated from the Rust structs with `ts-rs` (decision 0075), one file per type in
`ui/src/generated/`, and committed; nothing else goes in that folder. After changing an exported Rust type, regenerate
them all with:

```sh
cargo test -p otaq-desktop regenerate_types -- --ignored
```

`cargo test` (and so `scripts/check.sh`) fails, naming the stale files, when the committed files differ. A new type gets
the `TS` derive (with serde) in its own crate and one line in `export_all` in `src/generated.rs`.

A plain `cargo build` builds the development flavour, which loads the page from the Vite dev server
(`http://localhost:5199`); only the Tauri CLI's build turns on the `custom-protocol` feature that puts `ui/dist` into the
binary. That is also why `cargo clippy` and `cargo test` work on a fresh checkout without `ui/dist`.
