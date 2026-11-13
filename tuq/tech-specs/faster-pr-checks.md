# Faster pull request checks (OTAQ-6)

How the GitHub Actions checks get from 64–72 minutes per pull request to about 15. Builds on no product decision:
Product spec settled nothing (CI only, no change to what Otaq does). The user settled two points here (2026-11-13):

1. **Free runners, and speed up the slow tests too** (target ~15 min per PR run). No paid larger runners: the
   repository is public, so GitHub's standard runners cost nothing; larger runners are billed per minute.
2. **macOS stays local for now.** No macOS job in `checks.yml`; macOS still only builds in `release.yml`.

Files: `.github/workflows/checks.yml`, `.github/actions/setup/action.yml`, `scripts/check.ps1`, `scripts/check.sh`
(comment only), `.config/nextest.toml` (comment only), `crates/otaq-core/src/store/` (a test-only module),
`crates/otaq-core/src/service/testing.rs`, `crates/otaq-core/tests/suite/common/mod.rs`,
`crates/otaq-core/tests/suite/migrations.rs`, and the slow tests named in "Step 4".

## Where the time goes (run 43389661581 on main, 2026-11-13)

The pull request waits for the approve job, which needs every check job; the Linux jobs end in 2–12 minutes, so the
wait is the **Windows job**, which runs all of `check.ps1` in one job, one step after another:

| Windows step | Time |
|---|---|
| setup (Rust, nextest, Node, caches) | ~1.5 min |
| npm ci, `types` (builds otaq-desktop's tests: 3 min 47 s), tsc, eslint, prettier, vitest, page | ~7 min |
| fmt, clippy | ~2 min |
| nextest build | 5 min |
| nextest run: 3540 tests, 11,500 s of test time on 4 cores | **48 min** |

The same tests take ~3,500 s of test time on Linux: Windows is 3.3× slower overall, but very unevenly:

- The **migration tests**, which only run SQL on a database file (no process, no git), are **25–40× slower**
  (migration_0090: 108 s against 4.0 s; migration_0084: 26.8 s against 0.7 s). `generated_types_are_fresh`, which
  writes files, is 50× slower (25.5 s against 0.5 s). That points at **file I/O on the runner** (the temporary
  folder on `C:`, Defender scanning every new file, SQLite's flushes), not at the tests' logic.
- The median **otaq-core suite** test takes 7.4 s on Windows against 1.2 s on Linux. Every one of them starts from
  `Store::open_in` on a new data folder, which applies all 98 migrations to a new database file (and `TempRepos`
  runs ~5 `git` processes per repository). On Linux a whole migration takes ~0.7 s inside a migration test, so the
  98 migrations are most of the 1.2 s; on Windows, with the file I/O above, most of the 7.4 s.
- Tests that spawn agents or `git` repeatedly (agent_launches, deliveries, otaq-cli's agent tests at ~12 s each)
  are 4–7× slower: process start-up on Windows.

Raw numbers: `$OTAQ_RESOURCE_DIR/win-times.txt` and `lin-times.txt` (seconds, binary, test) on the maintainer's
machine; they come from the job logs of the run above (`gh run view --job <id> --log`).

## Decision 1: Windows runs the same jobs as Linux, in parallel, with the tests in shares

`checks.yml` gets one job definition per **group** of the check script, each a matrix over the two systems, instead
of four Linux jobs and one serial Windows job:

```yaml
# checks.yml (shape, not the final file)
  frontend:
    name: Checks (${{ matrix.os.name }}) – frontend
    strategy: { fail-fast: false, matrix: { os: [ {name: Linux, runner: ubuntu-latest}, {name: Windows, runner: windows-latest} ] } }
    runs-on: ${{ matrix.os.runner }}
    steps:
      - uses: actions/checkout@…
      - uses: ./.github/actions/setup
      - run: scripts/check.sh frontend                                         # Linux
      - run: powershell -ExecutionPolicy Bypass -File scripts\check.ps1 frontend  # Windows
  lint:     # the same, group "lint"
  test:
    name: Checks (${{ matrix.name }}) – tests ${{ matrix.share }}/${{ matrix.shares }}
    strategy:
      fail-fast: false
      matrix:
        include:    # the one list of shares; CHECK_TEST_PARTITION is built from it, never written by hand
          - { name: Linux,   runner: ubuntu-latest,  share: 1, shares: 2, groups: test doctest }
          - { name: Linux,   runner: ubuntu-latest,  share: 2, shares: 2, groups: test }
          - { name: Windows, runner: windows-latest, share: 1, shares: 4, groups: test doctest }
          - …       # Windows 2/4 … 4/4, groups: test
    env:
      CHECK_TEST_PARTITION: hash:${{ matrix.share }}/${{ matrix.shares }}
```

- **`check.ps1` takes the same groups as `check.sh`**: `frontend`, `lint`, `test`, `doctest`, no arguments meaning
  all of them in order, and `CHECK_TEST_PARTITION` passed to nextest as `--partition`. Run locally with no arguments
  it does exactly what it does today. The conpty step runs only when `test` or `doctest` is asked for; the Node check
  only with `frontend` (as in `check.sh`).
- **Windows keeps every group, frontend and fmt included.** The frontend steps are not platform-neutral in practice:
  run 43471761914 failed only on Windows, in vitest (`components\Floating.tsx`: a path with a backslash). As parallel
  jobs they add no wait; fmt is seconds inside the lint job. So the user's "Windows repeats what Linux runs" is
  solved by running it at the same time, not by dropping it, and both systems keep one definition of a green run.
- **Windows test shares: 4 to start** (`hash:N/4`). Dev sets the final number from a measured run, the smallest
  count where the slowest Windows test job ends within ~15 minutes, after Decisions 2 and 3 (which shrink the test
  time). The count lives only in the matrix's `include` list.
- **Each share builds its own test programs from the Rust cache**, as the Linux shares do, and only share 1 saves the
  cache. One job per system and group saves; the others restore it.
- **The setup action serves both systems.** `.github/actions/setup` gets `if: runner.os == 'Linux'` on the
  apt-get step and the Windows-only steps (Decision 2) under `runner.os == 'Windows'`; the Windows job's own copy of
  the setup steps in `checks.yml` goes. One setup for every job.
- The approve job needs `[frontend, lint, test]` (every matrix entry); its approval text stays "Checks (Linux and
  Windows) passed". No ruleset names these jobs as required checks (checked 2026-11-13: the "Otaq Ruleset" has a
required_status_checks rule with an empty list), so renaming them breaks nothing. `release.yml` calls
  `checks.yml` unchanged and gets the same speed-up.
- `timeout-minutes` for every Windows job drops from 120 to 45: a job that runs that long is a hang, not a slow test
  (nextest still kills a single test after 5 minutes).

### Considered

- **Build once, run the shares from a nextest archive** (`cargo nextest archive` in a build job, the shares download
  it and run `--archive-file … --partition`). Saves each share's ~5 min build, but the shares then wait for the
  build job, and Windows debug test programs are hundreds of MB to upload and download per share. The total wait is
  about the same and the workflow gets a second way to run tests that only CI uses. Lost; revisit if the build, not
  the tests, becomes the longest part of a share.
- **Drop frontend and fmt from Windows** (the description's question). Lost: Windows caught a real Windows-only
  frontend bug this week, and in parallel jobs they cost no waiting.
- **A Windows-only list of groups in `check.ps1`** (say `ci-windows`). Lost: two ways to say what a check run is.

### Cost of more jobs

A run now has 2 + 2 + 2 + 4 = 10 jobs instead of 5. The organization's free plan runs 20 jobs at once; when Otaq
opens several pull requests together, later runs queue. Pushing a new commit cancels a pull request's older run
(the `concurrency` group stays), which keeps the queue short. Measured, not designed around.

## Decision 2: the Windows runner's files go on a Dev Drive

In the setup action, Windows only: create a **Dev Drive** (a ReFS volume, which Defender scans asynchronously, made
for build and test folders) with `samypr100/setup-dev-drive` (pinned by commit, as every action here), and put on it:

- `TMP` and `TEMP`, so every `tempfile::tempdir()` of the tests (data folders, databases, git repositories, worktrees)
  is created there;
- the cargo target folder (`CARGO_TARGET_DIR`), with `Swatinem/rust-cache`'s `workspaces` pointing at it so the
  cache still restores and saves it.

This is the fix for the 25–50× file-I/O slowdown; it changes no test. Dev compares one Windows share with and without
it before going on, and keeps it only if it helps; if Dev Drive can't be created on the runner image, the fallback is
the same variables pointing at a folder on the workspace drive plus a Defender exclusion
(`Add-MpPreference -ExclusionPath`) for it.

Also measured, kept only if it shortens the Windows build: `CARGO_PROFILE_DEV_DEBUG=line-tables-only` and
`CARGO_PROFILE_TEST_DEBUG=line-tables-only` in the Windows jobs' env (smaller PDBs, faster links; backtraces keep
file and line).

### Considered

- **Bigger (paid) Windows runners**: the user chose free runners.
- **Turning Defender's real-time protection off** (`Set-MpPreference -DisableRealtimeMonitoring`): works on some
  images, refused by tamper protection on others; the Dev Drive is the supported way to the same effect.

## Decision 3: tests start from a ready-made database, not from 98 migrations

A new test-only module, `otaq_core::store::testing`, public like `git::testing` and `service::testing`:

```rust
// service/testing.rs: TestService::start_in, and tests/suite/common/mod.rs: Fixture::new
let data = DataDir::new(home.path().join("home"));
store::testing::seed(&data);                 // copies a migrated otaq.db into the data folder
let store = Store::open_in(&data).await?;     // finds it up to date: no migration runs
```

- `seed` copies a **template database**: a file made once by `Store::open` itself (so it has every migration, the
  version record, and the shipped agents of the first start, exactly as a real first start leaves them), with its
  WAL checkpointed (`pool.close()`), then copied.
- The template lives in the system's temporary folder, `otaq-test-store/<key>.db`, where `<key>` hashes the
  migrations' checksums (`MIGRATOR.iter()`) and `versions::RUNNING_VERSION`, so a changed or added migration makes a
  new template. nextest runs each test in its own process, so the template is shared between processes: the first
  test to need it builds it in a file of its own (a unique name beside it) and renames it into place; a test that
  loses the race uses the winner's file. Every builder makes the same content, so the race is harmless.
- `seed` is the only writer of the template; `Store::open` stays the only code that migrates. Production code doesn't
  change: `Store::open` on an up-to-date database already does nothing.
- Tests that are **about** a first start, migrations or the shipped agents' first apply (`migrations.rs`, `store.rs`,
  the first-start tests in `service.rs`, `updates.rs`) don't call `seed` and keep opening a new database. Dev greps
  `Store::open` and the fixtures' callers and lists which tests opt out in the module's doc comment.
- `migrations.rs`'s own helper `database_at` (which builds a database "as an older Otaq left it") opens its pool with
  `synchronous(SqliteSynchronous::Off)`: a test database that never outlives the test needs no flushes. It changes
  only that helper's pool; the later `Store::open` in each test keeps the real settings.

Also measured after this: `TempRepos::repo` (a `git init` and a commit per repository, ~5 processes). If it is still a
large part of a typical test on Windows, it copies a template repository the same way; otherwise it stays.

### Considered

- **An in-memory database in tests** (`sqlite::memory:`): fastest, but the service, the CLI and the desktop open the
  database by path from the data folder, and the WAL, the `before-<version>` copy and the cross-process access are
  part of what the tests check. Lost.
- **Faster migrations in production** (`synchronous=OFF` while a fresh database migrates): speeds up tests too, but
  changes how a user's database is written, for a CI gain. Lost.
- **A template built by a nextest setup script** (`[scripts.setup]`): would build it once per run up front, but ties
  the tests to nextest and leaves `cargo test` of one test broken without it. Lost to the build-on-first-use above.

## Decision 4: the slowest tests themselves, after the three above

Only after Decisions 1–3 are in and measured, Dev takes the Windows test list from the latest run (the `PASS [ …s]`
lines of each share's log) and looks at every test still **over 20 s on Windows**, slowest first. Today's candidates:
`deliveries::the_bytes_typed_for_each_tool` (67 s), `agent_launches::every_resume_passes_the_sessions_own_model`
(63 s), `agent_launches::every_resume_of_a_tool_default_session_passes_no_model` (59 s),
`warm_keep::skill_copies_are_neither_kept_nor_seeded` (47 s), `deliveries::a_paste_or_a_recalled_draft_still_counts_as_unsent_text`
(41 s), `m24_example::the_files_view_reads_the_example` (40 s), `suggest::cancel_stops_it_and_without_a_tool_it_is_refused`
(35 s), and the migration tests if Decision 2 leaves them slow.

Rules for that work:

- **A test keeps asserting exactly what it asserts today.** Allowed: one world/service shared by the cases of a table
  test instead of one per case (each case still its own fresh state where the test needs that), shorter fixed sleeps
  replaced by waiting for the event, fewer repeated agent launches when a case adds nothing new. Not allowed: dropping
  a case, `#[cfg(not(windows))]` to skip a slow test, or a longer timeout.
- A test that cannot get faster without losing what it checks stays as it is; the share count absorbs it.
- No retries and no test groups in `.config/nextest.toml` (its comment explains why; unchanged).

## Out of scope, seen on the way

The three most recent pull request runs failed for reasons of their own, not speed: a Windows-only vitest path bug
(OTAQ-3's new scan test, `components\Floating.tsx`), `api::terminal::tests::attaching_mid_stream_shows_every_line_once_in_order`
on Linux, and `deliveries::a_key_a_closing_prompt_took_leaves_no_unsent_text_and_the_note_goes` on Windows. Faster
shares don't fix these; they belong to their own tickets.

## Order and how it's checked

1. `check.ps1` groups and partition, the matrix in `checks.yml`, the shared setup action (Decision 1). Checked:
   `scripts\check.ps1` with no arguments still runs every step on Windows (the maintainer's Windows VM may be used),
   `scripts\check.ps1 test` with `CHECK_TEST_PARTITION=hash:1/4` runs about a quarter of the tests, `check.sh`
   unchanged; `actionlint` on the workflow if it is installed.
2. Dev Drive and the debug-info settings (Decision 2), measured on a real Actions run.
3. The template database (Decision 3). Checked: the whole suite passes on macOS/Linux and Windows; a test run twice in a
   row and two tests started at once both use one template; adding a migration makes a new one.
4. The slow tests (Decision 4).

Each step's real effect is only visible on GitHub's runners: a run needs the branch pushed (only when the user asks),
then `gh run view <id>` for the job times. The goal the user checks at the end: a pull request's Checks run done in
about 15 minutes, the Windows test shares each within it.
