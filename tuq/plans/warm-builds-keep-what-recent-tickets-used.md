# Plan · warm-builds-keep-what-recent-tickets-used

- **Ticket:** OTAQ-5
- **Date:** 2026-11-13
- **Decisions:** 054, 055, 056
- **Tech spec:** tuq/tech-specs/warm-builds-keep-what-recent-tickets-used.md
- **Orchestrator prompt:** none (the ticket's agent builds from OTAQ-5's Plan)

<!-- tuq:plan:start -->

Built from tuq/tech-specs/warm-builds-keep-what-recent-tickets-used.md (decisions 054–056) in four pieces, Rust only in
crates/otaq-core. Piece 1 is the new warm/usage.rs (the ledger, measuring, arming, the read probe), which both places
that apply it build on: the hand-on at Review's Free and Done's keep (2) and the copy of the user's checkout (3), side
by side once 1 is in. Throwing old stocks away once (056, piece 4) touches only store.rs and data_dir.rs and waits
for nothing. No protocol, UI or setting change; nothing new on screen (054).

## Pieces

1. warm/usage.rs: the usage ledger, measuring, arming and the read probe — waits for nothing
   Spec: tuq/tech-specs/warm-builds-keep-what-recent-tickets-used.md, "How used is seen" and "The ledger". New module
   crates/otaq-core/src/warm/usage.rs (add it to warm/mod.rs); the only code that knows "used". It holds:
   - `RECENT_TICKETS = 5` (decision 054's number, the one constant).
   - The ledger `warm/<repo>.usage` beside the stock (path via DataDir like store.rs's `sibling`): format
     `usage 1 \0 <tickets newest first> \0 <mask> <path> \0 …`, paths relative to the worktree, `/`-separated, files
     and symlinks only, bit i = i-th ticket, mask-0 paths not written. Record: a ticket already listed moves to the
     front and ORs its new paths into its bit; a new ticket goes in front, the sixth drops out with its bit. Keep set =
     every path in the file. Atomic write (`<repo>.usage.tmp` then rename); the caller holds the stock's `.incoming`
     lock (WarmBuilds::incoming). Missing/damaged/unknown format reads as empty.
   - `since(worktree)`: the mtime of the worktree's `.git` file; None (every file counts as used) when absent.
   - Measuring a folder: a file is used when its mtime or atime ≥ since. Arming: set atime to
     min(mtime − 1 s, now − 2 h), mtime untouched (File::set_times / filetime-equivalent std API).
   - The read probe: writes a file in `<app-data>/warm/.probe`, arms it, reads it, checks the atime moved. Run once at
     service start next to the clone probe (find where `CloneProbe::new()` is created) and stored on WarmBuilds
     (store.rs) with an accessor such as `reads_recorded()`; a test double like `CloneProbe::forced` so tests can force
     it on or off. Log once when reads aren't recorded.
   - The two entry points pieces 2 and 3 call, with these signatures so they can be built side by side:
     `usage::hand_on(warm, project, ticket, incoming_path, since) -> Trimmed` (measure what `ticket` used, record it,
     move files outside the keep set into `<repo>.trash/` via WarmBuilds::to_trash-like renames, a folder with no kept
     file as one rename, arm the rest; `Trimmed::{Left { kept, dropped }, Whole}`, Whole when reads aren't recorded or
     anything failed, logged; never an error) and `usage::trim_copy(warm, project, worktree, cloned_paths)` (drop
     from the worktree's copy what is outside the keep set, arm the rest; empty ledger → whole copy; never touches
     the source). Debug log "kept N files, left out M nothing used in the last 5 tickets".
   Unit tests in usage.rs (spec "Tests"): ledger round trip; a sixth ticket drops the oldest's bit and its only paths;
   a returning ticket moves to the front and keeps its earlier paths; fewer than 5 tickets; damaged/unknown file →
   empty; measuring with File::set_times (written, read, neither, no since); arming puts atime below mtime and
   now − 2 h and keeps mtime; hand_on and trim_copy on temp folders.
2. Trim a build as it is handed on (Review's Free, Done's keep) — waits for 1
   Spec: "Where it is applied". In crates/otaq-core/src/warm/keep.rs `move_to_stock`, after the paths are renamed
   into `.incoming` and before the WarmRecord is written and `stock.add` runs, call
   `usage::hand_on(warm, project, ticket, incoming.path(), usage::since(worktree))` (since read from the worktree
   before the move). Both Free (ticket_env/free.rs) and Done's keep go through move_to_stock, so nothing else changes;
   a Free or Done never fails because of it. Cleanup's size follows from the smaller entry (measure_later), as today.
   Tests in crates/otaq-core/tests/suite/warm_keep.rs: a kept build holds the files this ticket and the earlier 4
   used and not the rest; a Free records too; with the read probe forced off the build is kept whole and no ledger is
   written. Run `cargo test -p otaq-core warm_keep` and usage's unit tests.
3. Trim only the worktree's copy of the user's checkout — waits for 1
   Spec: "Where it is applied", decision 055 (the user's own folder is never written, renamed or re-timed). In
   crates/otaq-core/src/warm/seed.rs `Seeder::seed_from_folder`, right after the checkout's paths are cloned into the
   worktree and before the timestamp trick, call `usage::trim_copy(warm, project, worktree, &cloned)`. With the read
   probe off, or an empty ledger, the copy stays whole as today. Update seed.rs's module comment to say the copy is
   trimmed to what recent tickets used (still no build-tool knowledge). seed_scratch (Merge check) stays unchanged.
   Tests in crates/otaq-core/tests/suite/warm_seed.rs (not warm_keep.rs, which piece 2 edits): with a ledger, only
   the worktree's copy is trimmed and the checkout's files, their times and count are unchanged; with an empty ledger
   the copy is whole.
4. Stock format 2 throws old warm builds away once — waits for nothing
   Spec: "Throwing old warm builds away once (056)". In crates/otaq-core/src/warm/store.rs: STOCK_FORMAT becomes
   "stock 2" (make_stock writes it). remove_interrupted renames into `<repo>.trash/` (emptied in the background, as
   today via found.trash) every stock whose marker isn't "stock 2", every pre-stock folder (no marker) and every
   `<repo>.mig` folder; `migrate` and `finish_migration` go, and the doc comment's step 1 is rewritten. Worktrees are
   not touched. In crates/otaq-core/src/data_dir.rs add ".usage" to WARM_SIBLING_SUFFIXES (keep ".mig" so no project
   is named x.mig); remove_interrupted keeps `<repo>.usage` (a file, not a stock) and deletes a leftover
   `<repo>.usage.tmp`. Check is_temporary and the Cleanup list (WarmBuilds::list) don't treat `.usage` files as
   stocks or temporary folders to delete.
   Tests: store.rs unit tests (replace an_interrupted_migration_is_finished): a "stock 1" stock, a pre-stock folder
   and a .mig folder go to the trash; a "stock 2" stock and <repo>.usage stay; <repo>.usage.tmp is deleted. Fix
   tests/suite/warm_store.rs and stock.rs where they write the old marker.

<!-- tuq:plan:end -->
