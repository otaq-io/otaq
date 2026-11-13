# Plan · faster-pr-checks

- **Ticket:** OTAQ-6
- **Date:** 2026-11-13
- **Decisions:** none (CI only; Product spec settled nothing)
- **Tech spec:** tuq/tech-specs/faster-pr-checks.md
- **Orchestrator prompt:** none (the ticket's agent builds from OTAQ-6's Plan)

<!-- tuq:plan:start -->

The work follows tuq/tech-specs/faster-pr-checks.md (the user chose free runners plus faster slow tests, ~15 min per
PR run; macOS stays local). Two independent tracks, side by side: the CI track (check.ps1 learns check.sh's groups and
partition (1), then checks.yml runs every group as a Linux/Windows matrix with the Windows tests in 4 shares and one
shared setup action (2), then the Dev Drive on Windows (5)), and the test-fixture track (the template database (3),
then the fixtures that use it (4)). The slow-test work and the final Windows share count (6) wait for both, because
they are measured on a run that has all of the above. Pieces 2, 5 and 6 can only be measured on GitHub's runners:
the branch is pushed only when the user asks (ask with `otaq ask human`, never push unasked).

## Pieces

1. check.ps1 takes check.sh's groups and test partition — waits for nothing
   Spec: tuq/tech-specs/faster-pr-checks.md, Decision 1 (first two bullets). scripts/check.ps1 (102 lines) today
   runs every step in order with no arguments. Make it take the same groups as scripts/check.sh: `frontend`, `lint`,
   `test`, `doctest`, any of them as arguments, none meaning all in order (exactly today's run), an unknown group
   refused with the same message shape as check.sh ("unknown group '…' (frontend, lint, test, doctest)"). Copy
   check.sh's logic (its `wants` function, lines ~12–30, and CHECK_TEST_PARTITION → `--partition`, lines ~100–110):
   the Node version check only with frontend, the conpty step and the nextest requirement only with test/doctest,
   `cargo nextest run --workspace --partition $env:CHECK_TEST_PARTITION` when that variable is set. Update both
   scripts' header comments so they describe the same groups. No workflow change here.
   Check: on Windows (the maintainer's Windows VM may be used, see memory windows-vm-test-setup), `scripts\check.ps1`
   with no arguments still runs every step; `scripts\check.ps1 lint` runs only fmt+clippy; `$env:CHECK_TEST_PARTITION
   ='hash:1/4'; scripts\check.ps1 test` runs about a quarter of the tests. check.sh's behaviour is unchanged.
2. checks.yml runs every group on Linux and Windows in parallel — waits for 1
   Spec: tuq/tech-specs/faster-pr-checks.md, Decision 1 (the yaml shape, every bullet, and "Cost of more jobs").
   .github/workflows/checks.yml: replace the four Linux jobs and the one serial Windows job with three job
   definitions, `frontend`, `lint`, `test`, each a matrix over Linux (ubuntu-latest, scripts/check.sh <group>) and
   Windows (windows-latest, `powershell -ExecutionPolicy Bypass -File scripts\check.ps1 <group>`), fail-fast false.
   `test`'s matrix is one `include` list: Linux share 1/2 (groups test doctest), 2/2 (test); Windows 1/4 (test
   doctest), 2/4–4/4 (test); CHECK_TEST_PARTITION is built as hash:${{ matrix.share }}/${{ matrix.shares }}, never
   written by hand. Job names "Checks (<OS>) – frontend / lint / tests N/M". Only share 1 of each system (and one job
   per system and group) saves the Rust cache; the others restore. timeout-minutes 45 for Windows jobs (was 120).
   The approve job needs [frontend, lint, test]; its text stays "Checks (Linux and Windows) passed". Keep the
   concurrency group (a new push cancels the older run). release.yml calls checks.yml via workflow_call: keep its
   inputs/outputs working, don't edit release.yml unless it must.
   .github/actions/setup/action.yml serves both systems: the apt-get step gets `if: runner.os == 'Linux'`, the
   Windows job's own setup steps move into it under `runner.os == 'Windows'`, and checks.yml's Windows copy goes.
   No ruleset names these jobs as required checks (checked 2026-11-13), so renaming is safe.
   Check: `actionlint` on the workflows if installed (else a careful read); a real run needs a push, only when the
   user asks.
3. Template database: store::testing::seed — waits for nothing
   Spec: tuq/tech-specs/faster-pr-checks.md, Decision 3 (code sketch, first four bullets, "Considered").
   Add a public test-only module otaq_core::store::testing (crates/otaq-core/src/store/testing.rs, declared in
   store/mod.rs, public like git::testing and service::testing) with `seed(&DataDir)`: copies a template otaq.db into
   the data folder so a following `Store::open_in` finds it up to date and migrates nothing. The template is made by
   `Store::open` itself on a scratch data folder (every migration, the version record, the shipped agents of a first
   start), its pool closed so the WAL is checkpointed, then kept at <system temp>/otaq-test-store/<key>.db, where key
   hashes every migration's checksum (MIGRATOR.iter()) and versions::RUNNING_VERSION. nextest runs each test in its
   own process: build into a uniquely named file beside it and rename into place; a loser of the race uses the
   winner's file. Store::open stays the only code that migrates; production code doesn't change.
   Tests in the module: seed then Store::open_in runs no migration and gives the same schema/version as a fresh open;
   two seeds at once (threads or processes) end with one valid template; a different key (fake migration list or
   version) makes a new file. Don't switch any fixture over here (piece 4).
4. The test fixtures start from the template — waits for 3
   Spec: tuq/tech-specs/faster-pr-checks.md, Decision 3 (bullets 5–6). Call store::testing::seed(&data) (piece 3)
   before Store::open_in in crates/otaq-core/src/service/testing.rs (TestService::start_in, the DataDir made near
   line ~629) and crates/otaq-core/tests/suite/common/mod.rs (Fixture::new, line ~29). Tests that are about a first
   start, migrations or the shipped agents' first apply must keep a new database: migrations.rs, store.rs, the
   first-start tests in service.rs, updates.rs (grep Store::open and the fixtures' callers; give those an opt-out,
   e.g. a fixture constructor that skips seed) and list them in store/testing.rs's doc comment. Also otaq-cli's and
   otaq-desktop's test helpers if they open a fresh store the same way (grep Store::open in crates/*/tests and
   #[cfg(test)] code). In crates/otaq-core/tests/suite/migrations.rs, `database_at` (line 19) opens its pool with
   `synchronous(SqliteSynchronous::Off)`; the later Store::open in each test keeps the real settings.
   Check: the whole workspace suite passes (scripts/check.sh test); note the suite's test time before and after.
   Optional, only if measured worth it: TempRepos::repo copying a template repository (spec, "Also measured").
5. Windows build and test files on a Dev Drive — waits for 2
   Spec: tuq/tech-specs/faster-pr-checks.md, Decision 2. In .github/actions/setup/action.yml, Windows only: create a
   Dev Drive with samypr100/setup-dev-drive (pinned by commit SHA like every action here) and set TMP, TEMP and
   CARGO_TARGET_DIR onto it, with Swatinem/rust-cache's `workspaces` pointing at that target dir so the cache still
   restores and saves. Fallback if the runner can't make a Dev Drive: the same variables on a workspace-drive folder
   plus `Add-MpPreference -ExclusionPath` for it. Also try CARGO_PROFILE_DEV_DEBUG and CARGO_PROFILE_TEST_DEBUG =
   line-tables-only in the Windows jobs' env; keep each change only if a measured run shows it helps (compare one
   Windows share's build and test time with and without; gh run view <id> / --job <id> --log). Needs pushed runs:
   ask the user before each push.
6. The slowest Windows tests, then the final share count — waits for 4, 5
   Spec: tuq/tech-specs/faster-pr-checks.md, Decision 4, and Decision 1's "Windows test shares". From the latest run
   with pieces 1–5, collect each Windows share's `PASS [ …s]` lines and take every test still over 20 s, slowest
   first. Candidates today (crates/otaq-core/tests/suite unless noted): deliveries::the_bytes_typed_for_each_tool,
   agent_launches::every_resume_passes_the_sessions_own_model, agent_launches::every_resume_of_a_tool_default_
   session_passes_no_model, warm_keep::skill_copies_are_neither_kept_nor_seeded, deliveries::a_paste_or_a_recalled_
   draft_still_counts_as_unsent_text, m24_example::the_files_view_reads_the_example, suggest::cancel_stops_it_and_
   without_a_tool_it_is_refused, migrations if still slow; otaq-cli agent tests (~12 s each). Rules: each test keeps
   asserting exactly what it asserts; allowed: share one world/service across table cases (fresh state per case
   where needed), replace fixed sleeps with waiting for the event, drop repeated agent launches that add nothing.
   Not allowed: dropping a case, #[cfg(not(windows))], longer timeouts, retries or test groups in
   .config/nextest.toml. A test that can't get faster stays. Then set the Windows share count in checks.yml's
   `include` list to the smallest number where the slowest Windows test job ends within ~15 minutes. Out of scope:
   the three already failing tests named in the spec's "Out of scope". Pushes only when the user asks.

<!-- tuq:plan:end -->
