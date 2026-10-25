# Checks on macOS, Windows and Linux

> References such as "decision 0075", "d0072", "SDD §5.5", ticket numbers (T-0215), requirement IDs (UIT-02)
> and design frames (`crewboard.pen`) point to the maintainer's design notes, which are not part of this repository.

Every commit must pass the same steps on every platform (decision 0056, SDD section 23.2). First the frontend steps
of decision 0075 (point 7), from `crates/otaq-desktop`, where the page lives:

1. `node`: the Node version check, against `crates/otaq-desktop/.nvmrc` (24); the script stops with
   "Node 24 is required (found v22.3.0)" on any other version
2. `npm-ci`: `npm ci`, the locked packages
3. `types`: `cargo test -p otaq-desktop generated_types_are_fresh`, the `ts-rs` freshness check (it names the stale
   files in `ui/src/generated/`)
4. `tsc`: `npm run typecheck` (`tsc --noEmit`)
5. `eslint`: `npm run lint` (`--max-warnings 0`)
6. `prettier`: `npm run format:check`
7. `vitest`: `npm run test`
8. `page`: `npm run build:page` (Vite builds the page into `ui/dist`)

then the Rust steps:

9. `fmt`: `cargo fmt --all --check`
10. `clippy`: `cargo clippy --workspace --all-targets -- -D warnings`
11. `test`: `cargo nextest run --workspace` (each test in its own process, in parallel, no retries, and a test still
    running after 5 minutes is killed and fails; settings in `.config/nextest.toml`)
12. `doctest`: `cargo test --workspace --doc` (the doc examples, which nextest doesn't run)

The test step needs **cargo-nextest**: `brew install cargo-nextest` on the Mac, `cargo install cargo-nextest --locked`
anywhere. Without it the scripts stop before building anything and say so.

Each crate's integration tests are one test program, `tests/suite/` (`main.rs` names one module per file), so the
crate is linked once rather than once per file. To run one file, for example
`crates/otaq-core/tests/suite/naming_literals.rs`: `cargo nextest run -p otaq-core -E 'test(/^naming_literals::/)'`
(or `cargo test -p otaq-core --test suite naming_literals::`). A new test file goes in `tests/suite/` with a
`mod <name>;` line in `main.rs`; helpers shared by several files go in `common/` (`crate::common`).

| Platform | Run | Where |
|---|---|---|
| macOS | `scripts/check.sh` | the developer's Mac |
| Linux | `scripts/check.sh` (unchanged), on a copy | the local Ubuntu VM |
| Windows | `powershell -ExecutionPolicy Bypass -File scripts\check.ps1` | the local Windows VM |

Both scripts print each step's name and stop at the first failing step with a non-zero exit. GitHub Actions runs
`scripts/check.sh` on Linux for every pull request and every push to `main` (`.github/workflows/checks.yml`, d0056);
macOS and Windows join it later, so until then they're checked locally.

`check.sh` runs every step when it's given no arguments. Given group names, it runs only those: `frontend` (steps
1–8), `lint` (9–10), `test` (11) and `doctest` (12), for example `scripts/check.sh lint test`. `CHECK_TEST_PARTITION`
runs one share of the tests (`CHECK_TEST_PARTITION=hash:1/2 scripts/check.sh test`). GitHub Actions uses both to run
the steps as four parallel jobs: the frontend steps, fmt and clippy, and the tests in two shares, the doc examples
with the first. Before a commit, run it with no arguments.

**Disk space on the Mac first.** The VMs' disk images live on the Mac and grow with every build inside them (a
full check with its own target folder adds tens of GB per VM), and they don't shrink when the VM deletes files. Before
a VM run, check the Mac's free space (`df -h /System/Volumes/Data`) and keep well over 40 GB free; on 2026-10-13 two
VM checks side by side filled the Mac's disk, the Ubuntu VM froze in `cargo test --workspace` and quit. Run one VM at
a time, and delete a VM's target folder when its checks are done.

The VMs run in VMPal on the Mac and see the repository as a shared folder. Commits are made on the Mac only. Each VM
builds into a folder on its own disk (`CARGO_TARGET_DIR`), never the repository's `target/`, so its builds don't
collide with the Mac's. Each VM also runs the checks on its own copy of the repository, never in the shared folder:
`npm ci` replaces `crates/otaq-desktop/node_modules`, and the Mac's copy holds macOS builds of some packages
(esbuild, Rollup, the Tauri CLI).

## Rust

All three machines use the same toolchain as the Mac: **Rust 1.99.0** with `rustfmt` and `clippy`
(`rustup` installs it; the workspace asks for `rust-version = 1.99`).

## Node

All three machines also need **Node 24** (LTS) with its npm, for the frontend steps (decision 0075; the version is
pinned in `crates/otaq-desktop/.nvmrc` and `package.json`'s `engines`). Nothing npm-related is installed
globally: the Tauri CLI is a dev dependency. On the Mac, Node 24 comes from nvm (`nvm install 24`, then `nvm use` in
`crates/otaq-desktop`, or put `~/.nvm/versions/node/v24.*/bin` first on `PATH` for the run).

## Linux VM (Ubuntu 26.04, aarch64)

Set up on 2026-10-11, starting from a fresh install with nothing added; updated on 2026-10-13 (T-0246, no snapshot:
a running Ubuntu VM can't be snapshotted, and the user approved working without one). Each step says how to undo it.

1. Build tools and the system libraries Tauri builds against (WebKitGTK 4.1, GTK 3, the tray library, D-Bus), as
   root:

   ```sh
   sudo apt-get update
   sudo apt-get install -y build-essential pkg-config git curl wget file \
     libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
     libssl-dev libdbus-1-dev libnotify-bin
   ```

   `libnotify-bin` gives `notify-send`, which Otaq's OS notifications use on Linux (Ubuntu's desktop has it
   already).

   Done on 2026-10-13 with these packages, without errors (WebKitGTK 2.52.6). Undo: `sudo apt-get remove` the
   packages of the second line, then `sudo apt-get autoremove`. The GPUI app's system libraries installed in M11
   (the list in `git show b7498ca:scripts/README.md`) are no longer needed and can be removed the same way; they were
   left in place.
2. Rust, as the normal user:

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
     | sh -s -- -y --default-toolchain 1.99.0 --profile minimal -c rustfmt -c clippy
   ```
3. Node 24, as the normal user, with nvm (it installs into the home folder):

   ```sh
   curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
   . ~/.nvm/nvm.sh && nvm install 24
   ```

   Done on 2026-10-13 (nvm 0.40.3, Node v24.21.0). Undo: `rm -rf ~/.nvm` and delete the nvm lines the installer
   added to the end of `~/.bashrc`. The test build's leftovers in the home folder (`~/node24`, `~/m14-sysroot`,
   `~/m14-debs`, `~/m14tb`, `~/m14tb-target`) aren't used any more and can be deleted.
4. Run the checks on a copy in the home folder, with a target folder there too (the app itself needs a desktop
   session to open its window: Wayland or X11):

   ```sh
   rsync -a --delete --exclude target --exclude .claude --exclude node_modules --exclude ui/dist \
     /media/VMPal/software-factory/ $HOME/cb-src/
   cd $HOME/cb-src
   . ~/.nvm/nvm.sh && nvm use 24
   export CARGO_TARGET_DIR=$HOME/cb-target PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=4
   scripts/check.sh
   ```

   Run `rsync` again after every change on the Mac. Never commit from the copy. `CARGO_BUILD_JOBS=4` keeps the
   builds within the VM's 16 GB. To check a committed branch instead of the Mac's working tree, clone it from the
   shared folder: `git clone --branch <branch> /media/VMPal/software-factory ~/cb-src`. Undo: delete `~/cb-src` and
   `~/cb-target`.

## Windows VM (Windows 11 Pro 24H2, ARM64)

Set up on 2026-10-11, starting from a fresh install with only Git for Windows added; Node added on 2026-10-13
(T-0246), after a snapshot ("Before T-0246 (Node 24 via winget)").

1. **Git for Windows** (already there): `winget install Git.Git`. Besides `git`, the tests use its Unix tools
   (`sh`, `cat`) in place of `claude`, as on macOS and Linux; `check.ps1` adds Git's `usr\bin` to the end of PATH for
   the run. Claude Code on Windows needs Git for Windows too.
2. **Visual Studio 2022 Build Tools** with the C++ tools for ARM64 and the Windows 11 SDK, from an elevated
   PowerShell:

   ```powershell
   $override = '--quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools ' +
     '--add Microsoft.VisualStudio.Component.VC.Tools.ARM64 ' +
     '--add Microsoft.VisualStudio.Component.Windows11SDK.26100 --includeRecommended'
   winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --silent `
     --accept-source-agreements --accept-package-agreements --disable-interactivity --override $override
   ```

   (On an x64 machine use `Microsoft.VisualStudio.Component.VC.Tools.x86.x64` instead of the ARM64 component.)
3. **Rust**, as the normal user (no elevation):

   ```powershell
   Invoke-WebRequest -UseBasicParsing `
     https://static.rust-lang.org/rustup/dist/aarch64-pc-windows-msvc/rustup-init.exe -OutFile $env:TEMP\rustup-init.exe
   & $env:TEMP\rustup-init.exe -y --default-toolchain 1.99.0 --profile minimal -c rustfmt -c clippy
   ```
4. **Node 24**, the Windows ARM64 installer (`node-v24.*-arm64.msi`) from nodejs.org, or with winget:
   `winget install --id OpenJS.NodeJS.LTS --exact` (check that `node --version` prints `v24.*`; once a newer LTS
   is out, take the v24 installer instead). Open a new PowerShell afterwards so `node` and `npm` are on PATH.
   Done on 2026-10-13 from an elevated PowerShell, `winget install --id OpenJS.NodeJS.LTS --exact --scope machine`:
   winget's LTS package was still v24 (v24.20.0, arm64). Undo: `winget uninstall --id OpenJS.NodeJS.LTS --exact`.
   Git for Windows checks text files out with CRLF line endings by default (`core.autocrlf`), which Prettier's
   format check rejects in every file; the repository's `.gitattributes` keeps LF in every checkout.
5. **Run the checks on a copy on the VM's own disk.** The VMPal shared folder (`Z:\software-factory`) can't be
   canonicalized by Windows ("The volume does not contain a recognized file system", os error 1005), which
   `sqlx::migrate!` needs at build time, so the repository is mirrored to the VM's disk first (without the Mac's
   `node_modules`, which `npm ci` installs anew):

   ```powershell
   robocopy Z:\software-factory $env:USERPROFILE\cb-src /MIR /XD target .claude node_modules dist /XF .DS_Store
   $env:CARGO_TARGET_DIR = "$env:USERPROFILE\cb-target"
   cd $env:USERPROFILE\cb-src
   powershell -ExecutionPolicy Bypass -File scripts\check.ps1
   ```

   Run `robocopy` again after every change on the Mac. Never commit from the copy. To check a committed branch
   instead of the Mac's working tree, fetch it into the copy: `git fetch Z:/software-factory <branch>`, then
   `git checkout -f --detach FETCH_HEAD`. Undo: delete `%USERPROFILE%\cb-src` and `%USERPROFILE%\cb-target`.

PowerShell's default execution policy blocks unsigned scripts, hence `-ExecutionPolicy Bypass` for this one run
(the machine's policy stays as it is).

Optional, done in the VM: some tests listen on all network interfaces, and Windows then asks once per test
program whether to allow it through the firewall (the tests pass either way; the prompts only pile up on the
screen). An elevated `Set-NetFirewallProfile -All -NotifyOnListen False` turns those prompts off.

To look at the app in the VM (the hand checks), build it in the copy with `cargo build --release -p otaq-cli` (the
service the app starts sits next to it) and, in `crates\otaq-desktop`, `npm run tauri build` (the page goes inside
the binary), then start `%USERPROFILE%\cb-target\release\otaq-app.exe` from the signed-in desktop, with
`OTAQ_HOME` set to a temporary folder.
A program started by a remote command (VMPal's exec, ssh) runs off the visible desktop; a scheduled task that runs
in the signed-in session puts it on screen. On 2026-10-13 (T-0246) a `.cmd` setting `OTAQ_HOME` and
`OTAQ_KEY_LOG=1` and starting the app was run with `schtasks /create /tn cb-t0246-app /tr <the .cmd> /sc once
/st 23:59 /it /f`, then `schtasks /run /tn cb-t0246-app`. Undo: `schtasks /delete /tn cb-t0246-app /f`. After a
restart Windows waits at the sign-in screen (automatic sign-in is off), and nothing can be run in it until someone
signs in.

## Warm builds and project variables (for people using Otaq)

Decisions 0082, 0085, 0086 and 0087. This section is about using Otaq on any project, so apart from the marked
examples it names no language or build tool.

**Warm builds.** Each ticket still gets its own worktree, but a new worktree doesn't have to build from nothing.
A finished build (the worktree's gitignored files: what your setup installed and your builds made, but not the
project's env files) goes into the project's build stock, `<data folder>/warm/<repo-name>` (for example
`~/.otaq/warm/shop-web`, next to `wt/`), with the commit it was built at and the modified time of each tracked file.
A new worktree starts from the stock's nearest build: the tracked files unchanged since that build's commit get back
the times they had where the build was made, and the ones changed since get the current time, so your first build
only redoes what differs (also a file committed after the build). The setup log of each ticket says where its
worktree started from.

- **The switch:** project Settings → "Start new worktrees from a warm build", on by default, on every disk. Turn it
  off for a project whose build output doesn't survive being moved to another folder (a stack that writes absolute
  paths into it, such as a Python `.venv`); turning it off also clears the stock (after a confirmation). While it is
  off, a build never goes into the stock: it is deleted at Review and at Done or Close, and each new worktree builds
  from nothing.
- **Clear:** Cleanup lists each project's stock under the leftovers, with its size, commit and age, and a **Clear…**
  button, which clears every build of that project. A build being handed to a worktree right then is waited for.

### Ticket environment

Otaq looks after a ticket's build and its own resources at four moments:

- **Prepare** (Start, and a Closed ticket reopened): the worktree takes the stock's nearest build: the ticket's own
  build if it is still there, else one made at an ancestor of the ticket's commit with the fewest changed files,
  else the one nearest to the base branch's tip. Then the env files are copied, the Create command runs (see
  *Ticket resources* below) and your setup command runs, as an update of what is already there. With no build in
  stock, setup builds from nothing.
- **Free at Review:** when a ticket enters Ready for Review (after the validator, if the project has one), its build
  goes into the stock, so a ticket starting next can use it. The worktree, the branch, the ports and the agent's
  session stay where they are; `git status` stays clean, and the env files stay in the worktree (also one inside an
  ignored folder). An Approved ticket (PR open) stays freed too. A Run or Shell still open holds the build until the
  last one closes: Otaq never stops a process you started. Freeing always happens; it is not a setting. If the build
  can't be moved (on Windows, a file in it open in another program), it stays in the worktree and Cleanup lists it
  under "Review builds that couldn’t be freed"; Review goes on as usual.
- **Bring back** (Send back, Fix, Run, Shell): a freed ticket takes the nearest build (its own first), the file times
  are set as above and setup runs; then the agent resumes in the same session and worktree, or the Run or Shell
  starts. When a Run or Shell closes while the ticket is still in Review, the build is freed again.
- **Release** (Done, Close): the worktree goes as before. A build still in it goes into the stock; a freed build has
  nothing left to move. Then the ticket's resources are removed (see *Ticket resources*).

**Which disk.** What counts is the disk under Otaq's data folder, where `wt/` and `warm/` both live, so moving a
build between them never crosses disks.

- **A disk that can clone** (copy-on-write: APFS on macOS, btrfs or XFS on Linux, a Dev Drive on Windows): a new
  worktree gets a clone of the build, which shares its disk space, and the build stays in stock. The stock keeps one
  build per project, and only a newer build replaces it, never an older one. With no build in stock yet, the first
  worktree is cloned from your own checkout's gitignored files, if it has any and is on the same disk. Sizes in
  Cleanup say "up to" because a clone shares its space with what it was cloned from.
- **Any other disk** (NTFS, ext4, exFAT…): the build is handed over by renaming it, which takes a fraction of a
  second, and it leaves the stock. Settings says "This disk can’t copy-on-write; the build is handed over to the
  next ticket." Otaq never falls back to a plain copy or to hard links, which would cost a whole build's disk space
  per ticket, and your own checkout is never a source (its files can't be moved away). If a rename fails (a file of
  the build open in another program, or the build on another disk), that start goes on without it and setup builds
  from nothing; the build stays in stock.
- **On Windows, put the data folder on a Dev Drive** to get clones instead of handovers: a Dev Drive is formatted as
  ReFS, which can copy-on-write. Create one in Settings → System → Storage → Advanced storage settings → Disks &
  volumes → Create Dev Drive (a new partition or a virtual disk file). Then move Otaq's data folder there while no
  ticket has a worktree (nothing in Dev or Ready for Review, and Cleanup empty): quit Otaq, stop its service
  (`otaq daemon stop`), move the folder `%USERPROFILE%\.otaq` to the Dev Drive (for example to `D:\otaq`) and delete
  the `scratch` folder inside it (Otaq makes it again at the next Local merge), run `git worktree prune` in each
  project's checkout, set `OTAQ_HOME` for your account (`setx OTAQ_HOME D:\otaq`) and start Otaq again. With
  `OTAQ_HOME` set, Otaq uses that folder instead of `~/.otaq` (on every platform).

**Stock limit** (on a disk that can't clone). The stock keeps as many builds per project as there are tickets that
could start soon: those in Ready for Dev whose dependencies are done and that wait only for a free slot or the plan
limit. With none, it keeps 1. A ticket waiting for a dependency doesn't count: when that dependency reaches Done, its
build goes into the stock and is the nearest one anyway. The limit is worked out again when a build goes in or is
taken, and when a ticket enters or leaves Ready for Dev. Over the limit, the builds farthest from the base branch's
tip (the most changed files) go first, and among equals the oldest; a build being handed over is never removed. A
removed build is renamed into `warm/<repo-name>.trash` and deleted in the background (deleting a large build can
take many seconds); what is left there is deleted when Otaq starts. The limit is not a setting. A freed ticket's
build counts like any other in the stock, so a ticket in Review or Approved may find its own build gone at Bring
back and take the nearest one instead.

**Setup that updates, not reinstalls.** A setup command that deletes its output before installing (a "clean
install", or a step that empties the build folder first) throws the handed-over build away on every new worktree,
and the first build starts from nothing again. Use the form of your install step that updates what is already there
and doesn't change tracked files. For example with npm: `npm install --no-save`. `npm ci` deletes the handed-over
`node_modules`, and a plain `npm install` rewrites `package-lock.json` even when nothing changed.

**Local data.** Everything gitignored in a worktree goes with its build into the stock and on to another ticket, so
keep data that belongs to one ticket out of it:

- A file database (SQLite and the like) goes in `$OTAQ_RESOURCE_DIR` (see below), not in the worktree. A database
  file left in the worktree would hand that ticket's migrations on to the next ticket.
- Other local data that must stay with its worktree (a local config, uploaded files): add it to the project's env
  files list. Files on that list are copied into each new worktree and never moved with a build.

**Ticket resources.** Project Settings → Ticket resources has three commands for what each ticket needs of its own,
such as a database. Otaq knows no database engine: it only names the resource and runs your commands at the right
moments. All three are empty by default, and with them empty nothing changes.

- **Create command:** runs at Prepare in the new worktree, after the env files and before setup. Otaq records the
  resource's name on the ticket before it runs. A failure stops the start at "creating ticket resources failed", and
  Retry start runs it again. A resource that was created is never created again (Send back, Fix, Retry start).
- **Remove command:** runs at Done and Close in the project's checkout, only for the name Otaq recorded on that
  ticket; your own database's name is in none of the variables. A failure doesn't stop Done or Close: Cleanup lists
  it under "Ticket resources that couldn’t be removed", and its Delete runs Remove again. Nothing retries it on its
  own.
- **Template refresh command:** runs in the project's checkout at Done, after the merge (never at Close), so new
  tickets start from the merged schema. Refreshes of one project run one at a time, and a Create waits while one
  runs (Postgres refuses to copy a template someone is connected to).

Where a project has a Create command (or a ticket has a recorded resource), Otaq sets these in the ticket's commands
(setup, Create, Remove) and sessions (its agent, Run and Shell), and project rows can't override them:

- `OTAQ_DB_NAME`: the ticket's database name, `otaq_<project>_<ticket>` in lowercase with `_` for anything other than
  `a-z` and `0-9`, at most 63 characters; for example `otaq_shop_shop_18` for SHOP-18.
- `OTAQ_DB_TEMPLATE`: the project's template database, for example `otaq_shop_template`.
- `OTAQ_RESOURCE_DIR`: a folder of the ticket's own outside the worktree, `<data folder>/res/<repo-name>/<KEY>`.
  Otaq deletes it after a successful Remove, or at Release when there is no Remove command.

The Template refresh command belongs to no ticket's resource, so it gets only `OTAQ_DB_TEMPLATE` (and `OTAQ_TICKET`,
the ticket just merged).

Examples (only examples; write them for your engine and shell):

- **Postgres:** Create `createdb -T "$OTAQ_DB_TEMPLATE" "$OTAQ_DB_NAME"`. Remove
  `dropdb --if-exists --force "$OTAQ_DB_NAME"`. Template refresh: drop the template, create it, then apply the main
  branch's migrations and seed to it, for example `dropdb --if-exists --force "$OTAQ_DB_TEMPLATE" && createdb
  "$OTAQ_DB_TEMPLATE" && DATABASE_URL="postgres://localhost/$OTAQ_DB_TEMPLATE" <your migrate and seed commands>`.
- **SQLite:** Create copies the template file into the ticket's folder, such as
  `cp db/template.sqlite3 "$OTAQ_RESOURCE_DIR/dev.sqlite3"`. Remove can stay empty, since Otaq deletes the folder.
- **MySQL:** Create `mysql -e "CREATE DATABASE $OTAQ_DB_NAME" && mysqldump "$OTAQ_DB_TEMPLATE" | mysql
  "$OTAQ_DB_NAME"`. Remove `mysql -e "DROP DATABASE IF EXISTS $OTAQ_DB_NAME"`.
- **The connection:** a project variable row `DATABASE_URL = postgres://localhost/${OTAQ_DB_NAME}` gives each
  ticket's sessions and commands its own database (see *Project variables*).

**Schema paths.** Ticket resources → Schema paths takes one path per line, such as `db/migrate/**` or
`migrations/**` (a folder alone also covers the files under it). When two tickets in Review (or Approved) both add
files under the same path, their cards show a warning naming the other ticket, with its files on hover, and the
ticket's History records it. It is only a warning: Otaq changes no file. It usually means the two migrations need an
order once both are merged. A ticket that is abandoned needs no down-migration, because its database is removed and
the template never saw its migration. With no schema paths, Otaq runs no git for this.

**What Otaq can't do here.**

- Create a database without your commands: it knows no engine.
- Give an agent your real data without copying it.
- Isolate a ticket whose connection address is written into the repository instead of read from a variable.
- Give each ticket a database on a hosted plan that allows one database and no `CREATE DATABASE`.
- Run a database server per ticket without something like Docker, which your own Create command would start.

**Project variables.** Project Settings → Environment variables lists `NAME = value` rows that Otaq sets in
everything it starts in that project: ticket agents and their helpers, the validator, Noyan, the Project
terminal, the setup command and the ticket resource commands, Run and Shell, and the git commands it runs in the
project's repository and worktrees (so your git hooks see them too). It is empty by default. For example
`CI = true`, so tools never wait for input or start watch mode. Another one you may want when several agents build
at once: each build takes every core by default, so give each one a share, written for your stack, such as a
variable your build tool reads for its job count set to your cores divided by the number of agents that build at the
same time. A change applies to what starts or resumes after it; running sessions keep the values they started with.
A row with an empty name, a name containing `=`, or a name repeating an earlier row's (on Windows, also one
differing only in case) is marked and not set; Otaq's own variables (`OTAQ_*`, `PORT`) win over a row of the same
name.

A value can use Otaq's own variables as `${OTAQ_…}`, such as `${OTAQ_DB_NAME}`: each one is replaced by its value
where it is set, and by nothing where it isn't (in a git hook, for example). Only that exact form with an `OTAQ_`
name is replaced; anything else, such as `$HOME` or `${HOME}`, is passed on as written.

## Installing your own build (macOS)

`scripts/install-local.sh` builds a release from this checkout and installs it the way users get it (decision 0072),
so the author runs what users run. No admin rights:

- `~/Applications/Otaq.app`: `Contents/MacOS/otaq-app` (the app, the bundle's main program) and
  `Contents/MacOS/otaq` (the command and the background service), the agent tools' plugin folders in
  `Contents/Resources` (`plugin/`: the shipped skills and each tool's hooks plugin), the tent icon, an `Info.plist` with the bundle ID
  `io.otaq.desktop` and the workspace's version, signed ad-hoc (decision 0021, no paid signing);
- `~/.local/bin/otaq`, a link to `Otaq.app/Contents/MacOS/otaq`. When `~/.local/bin` isn't on `PATH` the script says
  so; it never edits shell files.

It needs Node 24 on `PATH` (it runs `npm run tauri build`, which puts the page inside the app, then
`cargo build --release -p otaq-cli`). Before replacing an installed `Otaq.app` it stops the background service of the
data folder in use (`OTAQ_HOME`, or `~/.otaq`) with the old bundle's `otaq` and says so; the app starts it again.
`otaq daemon start` runs the service from the real path of the program, so a start through the link runs it from
the bundle (macOS ties a process to its bundle by that path, which notifications need, decision 0090).

| Option | What it does |
|---|---|
| `--dest <folder>` | installs `<folder>/Otaq.app` instead, without the link (tests, trying a build) |
| `--bundle-id <id>` | another bundle ID, for a check bundle (`io.otaq.desktop.check`): macOS asks for notification permission only once per ID, so a check doesn't use up the real one. A check bundle must not live in a temporary folder (`/tmp`, `/var/folders`, `$TMPDIR`): LaunchServices marks an app there `in-temp-dir`, and macOS's notification service refuses it |
| `--from <folder>` | takes `otaq` and `otaq-app` from `<folder>` instead of building |

The layout, `Info.plist` and signature are checked by `crates/otaq-cli/tests/suite/install_local.rs` (part of the
`test` step, macOS only), on a fake build: copies of `/usr/bin/true` passed with `--from`, so no release build runs
in the checks.

`otaq notify-check` (hidden from `--help`) checks notifications from the bundle: it prints whether it runs inside
`Otaq.app` and its bundle ID; inside, it shows macOS's permission state, asks once if macOS hasn't asked yet, posts
one test notification and waits 60 seconds for a click, printing whether the click reached it. Outside the bundle it
prints "outside Otaq.app" and calls nothing of macOS's notification API.

## Not covered by the checks

The hand checks of each milestone (`tickets/review-M*.md`), for example the window chrome, keyboard shortcuts and
notifications on Windows and Linux, and anything with the real `claude`, `gh` or `az`.

## Third-party licenses

`THIRD-PARTY-LICENSES.md` lists the notices of everything Otaq ships (decision 0071). After changing dependencies, run
`scripts/licenses.sh` on the Mac and commit the regenerated file with the change; `scripts/licenses.sh --check` says
whether it is up to date. It covers the Rust crates (through `cargo-about`) and the page's production npm packages
(`scripts/licenses/npm.mjs`, from `package-lock.json` and the license files of the installed packages, so `npm ci`
first). It needs `cargo-about` (`cargo install cargo-about --locked --features cli`), so it isn't one of the check
steps.
