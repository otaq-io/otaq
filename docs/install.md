# Install and set up Otaq

[← README](../README.md)

## What you need

| | |
|---|---|
| **An agent tool** | at least one of [Claude Code](https://docs.anthropic.com/en/docs/claude-code), [Codex CLI](https://github.com/openai/codex), [GitHub Copilot CLI](https://github.com/github/copilot-cli) or [OpenCode 2](https://opencode.ai/v2/docs/) (`npm install -g --allow-scripts=@opencode/cli @opencode/cli`; the `opencode-ai` package is OpenCode 1, which Otaq can't run), installed and signed in |
| **git** | any recent version |
| **OS** | macOS (most tested), Windows 10/11, or Linux |
| **To build** (only from source) | Rust 1.99+ (edition 2024) and Node 24 |
| **Optional** | `gh` (GitHub) or `az` (Azure DevOps) for pull requests |

## Install

Each release on [GitHub Releases](https://github.com/otaq-io/otaq/releases) has a setup file per system. Download
yours and open it like any app:

| System | Setup file | Updates |
|---|---|---|
| macOS (Apple silicon and Intel) | `Otaq_<version>_universal.dmg`: drag Otaq to Applications | by itself |
| Windows 10/11 (x64, arm64) | `Otaq_<version>_<arch>-setup.exe`: installs for your account only, no admin rights | by itself |
| Linux (x64, arm64) | `Otaq_<version>_<arch>.AppImage`: make it executable and run it from anywhere | by itself |
| Debian, Ubuntu (x64, arm64) | `Otaq_<version>_<arch>.deb`: `sudo apt install ./Otaq_<version>_<arch>.deb` | by hand: download the next `.deb` |

Otaq isn't code signed (paid OS signing isn't something it does), so the first open asks you to allow it once:

- **macOS:** the first open says Apple can't check Otaq. Open **System Settings → Privacy & Security**, scroll to
  "Otaq was blocked…" and click **Open Anyway**. It asks for an administrator's name and password: on a standard
  account, an administrator does this once. Otaq runs only from an Applications folder: opened from the `.dmg`
  or from Downloads, its window offers just **Move to Applications**, which moves it (to `/Applications`, or
  `~/Applications` when you can't write there) and opens it from there.
- **Windows:** SmartScreen says it protected your PC: click **More info → Run anyway**. Otaq goes into
  `%LOCALAPPDATA%\Otaq`, with a Start-menu shortcut.
- **Linux:** the status icon needs the AppIndicator library (the `.deb` installs it; for the AppImage,
  `sudo apt install libayatana-appindicator3-1` on Debian and Ubuntu), and stock GNOME shows tray icons only with the
  AppIndicator extension (`gnome-shell-extension-appindicator`, on by default in Ubuntu's desktop). Without either,
  Otaq still runs, without the icon (see [The status icon](#the-status-icon)).

**Updates happen by themselves.** Once a day Otaq asks GitHub for the newest version (one request for a fixed file;
nothing about you or your projects is sent), downloads it in the background and checks its signature. Then Settings
gets a dot and Settings → About says **Otaq 0.2.0 is ready · What's new · Restart now · Skip this version**. Nothing
changes until then: the new version installs when you pick **Restart now** (or **Restart to update** in the status
icon's menu), or the next time you open Otaq. Either way only the window reopens, on the new version, and says
"Updated to Otaq 0.2.0"; your agents keep working. The background service moves to the new version later, when you
pick **Restart now** on its "Service update ready" banner. An update the running service can't work with says so
first ("needs agents restarted (3 working)", with **Restart now · stops 3 agents**): it restarts the app and the
service together, as after a reboot (the agents show Interrupted and Resume all continues them), and it never
installs on its own when you open Otaq. The switch **Check for updates automatically** and the **Check for updates**
button are in Settings → About. The `.deb` doesn't update itself: About says "Otaq 0.2.0 is available · Download".

**Going back to an older version.** Before a new version changes the database's layout, Otaq keeps a copy of it
(`~/.otaq/otaq.db.before-0.2.0`). If you reinstall an older version, its window says the board was last opened by a
newer Otaq and offers **Restore the copy from before 0.2.0**; `otaq restore-database` does the same in a terminal.
Changes made on the newer version since the copy are lost.

## Build and run from source

```sh
git clone https://github.com/otaq-io/otaq.git
cd otaq
cd crates/otaq-desktop && npm ci && cd ../..
scripts/install-local.sh     # macOS: builds Otaq.app and installs ~/Applications/Otaq.app
```

`scripts/install-local.sh` builds the same `Otaq.app` a release has, with Tauri's bundler. It is **built from
source**: it never checks for updates, a release never replaces it, and Settings → About says "Built from source ·
<commit>". Build it again to update. On Windows and Linux, `npm run bundle` in `crates/otaq-desktop` makes the setup
`.exe` or the `.AppImage` and `.deb` the same way; [`scripts/README.md`](../scripts/README.md) says how for each system.

## Your first project

1. Open Otaq and **add a project**: any folder that is a git repository. Otaq suggests a short key (`SHOP`) used in
   ticket names (`SHOP-12`) and picks your base branch from the remote. Adding a project also lets your connected
   agent tools work in that folder (it records the folder as trusted in each one, as accepting the tool's own trust
   prompt would, so the project's hooks and settings run).
2. Optional: make a few named agents in Settings → Agents ([named agents](how-it-works.md#agents-any-tool-any-model)), and set the defaults. In the project's settings: a
   **setup** command run in every new worktree (`pnpm install`), a **run** command for the Run tab
   (`pnpm dev --port $PORT`, or **Add app** for each app of a monorepo), environment variables, and Agent
   instructions (for example "run `cargo test` before you submit"). Leave the run command empty and an agent that
   needs the app can add it.
3. **Create a ticket**, pick its agent or leave the project default, drag it to **Dev**, and watch the agent start.

Agents use your own permission or approval mode for each tool; Otaq never changes it. In a mode that asks before
edits, they will stop on permission prompts and show up as "Needs you"; auto-accept modes let them run unattended.
Otaq pre-approves only its own `otaq` commands (for Codex, through an `otaq.rules` file in `~/.codex`, which Otaq
removes when you disconnect the tool).

## The status icon

While Otaq runs, its icon sits in the menu bar (macOS) or the notification area / tray (Windows, Linux). It shows when
an agent needs you or every project is paused, and its menu lists each project's needs-you items, checks and running
tickets: pick one to open Otaq on it.

- **Closing the window hides it.** Otaq keeps running in the menu bar / tray and agents keep working; the icon's Open
  Otaq (or running `otaq` again) brings the window back. Only one copy of Otaq runs per data folder.
- **Stop agents and quit**, in the icon's menu, stops every agent and terminal, stops the background service, pauses
  all projects so nothing starts on its own, and quits. Next time, Resume continues where each agent was.
- **⌘Q on macOS** quits only the app; agents keep working in the background service. While agents work it asks first:
  keep them working and quit, stop agents and quit, or cancel.
- **Linux without a tray** (stock GNOME without the AppIndicator extension, or without the library): a hidden window
  couldn't be found again, so closing the window acts as Stop agents and quit, asking first while agents work.

## Where things live

- **Data:** `~/.otaq/` (a SQLite database, logs, the service socket). Set `OTAQ_HOME` to use another folder.
- **The `otaq` command for your terminal:** `~/.otaq/bin/otaq`, kept by the app on every start (see
  [Your own terminal](otaq-command.md#your-own-terminal)). Agents use their own copy in `~/.otaq/runtime/<build>/`, so an update
  never changes the version an agent's session runs. Downloaded updates wait in `~/.otaq/updates/`.
- **Worktrees:** `~/.otaq/wt/<repo>/<TICKET>`, outside your repository. Branches are named `otaq/SHOP-12` while
  work is in progress.
- **Warm builds:** a new worktree starts from a copy-on-write clone of the last finished ticket's build output (APFS,
  btrfs, XFS, ReFS), so its first build only redoes what changed.

## Anonymous usage data

When first run ends, Otaq asks once: **"Help improve Otaq"**. Nothing is sent unless you press *Send anonymous data*;
*No thanks* (or Esc) means nothing is ever sent, and Otaq works the same either way. Change it anytime in
Settings → Data → *Send anonymous usage data*.

If you agree, Otaq sends to [Aptabase](https://aptabase.com) (EU region) which features are used and how often, with
the Otaq version and your OS. It never sends ticket text, code, diffs, prompts, project, file or branch names, paths,
or any ID that follows you across launches: each launch gets a new random session, and nothing is stored on disk. The
full list of what counts as a feature is in [`crates/otaq-desktop/src/usage.rs`](../crates/otaq-desktop/src/usage.rs).
Debug builds (`npm run tauri dev`) send nothing at all: the Aptabase App Key is only built into release builds.


## Uninstall Otaq

Quit the Otaq app (from its menu bar / tray icon: closing the window only hides it), then run in a terminal:

```sh
otaq uninstall
```

(With the app open it stops at "Quit Otaq from its menu bar / tray icon first: the app would start the service
again".) It says what it is about
to do and asks before changing anything. Both questions come first; nothing is deleted until you have answered yes
to each, and only `y` or `yes` counts: Enter means no.

1. **Worktrees, if any are left.** Every worktree Otaq still has (a ticket in progress, a helper's, a Done or Closed
   ticket's kept one, one no ticket owns, or a merge worktree a crash left with changes) is listed, with ✎ on those
   with uncommitted changes:

   ```
       SHOP-12 · shop-web · /Users/me/.otaq/wt/shop-web/SHOP-12
     ✎ SHOP-14 · shop-web · /Users/me/.otaq/wt/shop-web/SHOP-14
     ✎ SHOP-14.1 (helper) · shop-web · /Users/me/.otaq/wt/shop-web/SHOP-14.1
   Delete these 3 worktrees? Uncommitted changes in the 2 marked ✎ are lost. Their branches are kept. [y/N]
   ```

   **N** stops with "Nothing changed. Move them on or clear them first (Done, Close, the Cleanup view)". **y**
   removes each one with `git worktree remove`, so your repositories' records stay correct.
2. **The data folder** (`~/.otaq`, or the folder `OTAQ_HOME` names), with its size, and what is still running:

   ```
   Delete ~/.otaq (6.4 GB: 3 projects, 41 tickets, warm builds 5.9 GB)? 2 agents and 1 conversation with Noyan end. [y/N]
   ```

Then it stops the background service (agents still working are interrupted), removes the worktrees, removes
the skill folders an older version of Otaq left in your projects, deletes the data folder
(with `~/.otaq/bin`), removes the `# added by Otaq` line from your shell's file (on Windows, the folder from your
PATH), and prints what it removed and what it kept, for example:

```
Kept on purpose:
  Claude Code, Codex: SHOP and PAY stay trusted
  shop-web: 23 otaq/ branches kept · git branch --list 'otaq/*'
Drag Otaq.app to the Trash
```

Anything it skipped or couldn't remove is listed with its path (and the exit code is non-zero). The last step is
yours, and the summary's last line names it:

- **macOS:** drag Otaq.app to the Trash.
- **Windows:** uninstall Otaq from **Settings → Apps**.
- **Linux:** remove the package (`sudo apt remove otaq`) or delete the AppImage.

Removing only the program (Settings → Apps, `apt remove otaq`, deleting the AppImage) stops the background service
and leaves your worktrees and data; the Windows uninstaller and the `.deb` say to run `otaq uninstall` first.

**Kept on purpose:**

- **Your `otaq/` branches**, merged or not, so committed work is never lost. List them with
  `git branch --list 'otaq/*'` and delete the ones you don't need with `git branch -d`.
- **Folder trust in your agent tools.** Adding a project marked its folder as trusted in Claude Code, Codex and
  Copilot. Otaq doesn't know whether you had trusted the folder yourself before, so it leaves the mark alone; it only
  means the tool won't ask again before working there. To remove one by hand, close the tool, keep a copy of the
  file, and remove only that folder's entry:
  - Claude Code: `hasTrustDialogAccepted` under the folder's entry in `projects`, in `~/.claude.json`
  - Codex: `trust_level = "trusted"` in the folder's `[projects."<folder>"]` table, in `~/.codex/config.toml`
  - Copilot: the folder's line in the `trustedFolders` list, in `~/.copilot/config.json`

**Coming back later?** `otaq uninstall --keep-data` keeps the database (with its `otaq.db.before-<version>` copy)
and the comment reports, so reinstalling brings your projects and tickets back; warm builds, logs, worktrees,
`~/.otaq/bin` and the PATH line still go. Its data-folder question reads as
below (with the same "… end." sentence when agents are running):

```
Delete everything in ~/.otaq but the database and comment reports (5.9 GB)? [y/N]
```

