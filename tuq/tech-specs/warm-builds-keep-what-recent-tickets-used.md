# Warm builds keep only what recent tickets used (OTAQ-5)

How decisions 054 (a warm build carries only the gitignored files the last 5 tickets' builds used), 055 (the user's
own folder is never changed) and 056 (warm builds kept before this are thrown away once) are built. Rust only, in
`crates/otaq-core/src/warm/` and `data_dir.rs`; no protocol, UI or setting change.

## The problem in one paragraph

Warm builds hand a ticket's gitignored files on at Review (Free, `ticket_env/free.rs`) and at Done (keep,
`warm/keep.rs`), both through `keep::move_to_stock`; a new worktree is seeded from the stock (`warm/seed.rs`), or from
the user's checkout when the stock is empty. Nothing ever leaves a build, so whatever a build tool wrote once (Cargo's
old hash-named artifacts in `target/debug/deps`, old `incremental/` sessions) is carried to every ticket forever. To
leave out "what no recent ticket used" without knowing any build tool, Otaq has to see, per file, whether a ticket's
builds **wrote or read** it.

## Decision: a usage ledger beside the stock, measured from file times, applied when a build is handed on

### How "used" is seen: modified and access times, armed so a read always shows

- **Written** = the file's modified time is at or after the moment the worktree's build began. **Read** = its access
  time is at or after that moment. Every build tool reads its inputs and writes its outputs through the file system,
  so this is the one tool-agnostic signal there is (rejected alternatives below).
- **When the build began** (`since`): the modified time of the worktree's `.git` file, which `git worktree add`
  writes once when the worktree is made and nothing rewrites. No new column or file: the worktree carries it, and it
  is right for worktrees made before this update too. A fetch (Send back after a Free) brings a build into an older
  worktree; files of that build written after the worktree was made count as this ticket's too. That only keeps a
  little more (they are in the window's set already), never less. No `.git` file: every file counts as used.
- **Arming.** Systems don't record every read: macOS APFS and Linux `relatime` update the access time only when it is
  older than the modified time (checked on this Mac: atime = mtime is not updated by a read, atime < mtime is), and
  NTFS only when it is more than an hour old. So every file Otaq hands on gets its access time set to
  `min(mtime − 1 s, now − 2 h)` (`arm`), its modified time untouched: then the first read in the next worktree always
  updates it. Clones keep both times (checked: `clonefile` copies the armed atime, and a read in the clone changes
  only the clone), and renames keep them, so arming once when a build enters the stock covers every worktree seeded
  from it. A copy of the user's checkout is armed in the worktree (never in the checkout, decision 055).
- **Disks that don't record reads** (`noatime`, NTFS with last-access updates off, which Windows does by default on
  large volumes): a probe at service start, next to the clone probe, writes a file in `<app-data>/warm/.probe`, arms
  it, reads it and checks its access time moved. Where it didn't, nothing is measured, recorded or left out: builds
  are handed on whole, as today (logged once). Leaving files out by writes alone would drop every dependency output
  that is only ever read (Cargo's rlibs of unchanged crates), and every ticket would rebuild them.

### The ledger: `warm/<repo>.usage`, one file per stock

The paths each of the last 5 tickets used, in one file beside the stock (not inside it, so Clear in Cleanup and
decision 056 don't forget what recent tickets used, and a later copy of the checkout can still be trimmed, 055):

```
usage 1 \0 SHOP-21 SHOP-20 SHOP-19 SHOP-17 SHOP-16 \0 <mask> <path> \0 <mask> <path> \0 …
```

- The tickets, newest first, at most `RECENT_TICKETS = 5` (the one constant for decision 054's number). Each path
  (relative to the worktree, `/`-separated, a file or symlink, never a folder) has a bit mask: bit *i* is the *i*-th
  ticket. A path whose mask is 0 is not written.
- **Recording** a ticket's used paths: a ticket already in the list moves to the front and its bit is OR-ed with the
  new paths (a Send back after Review's Free merges with what it used before); a new ticket goes in front, the sixth
  drops out and its bit with it. **Fewer than 5** tickets so far: the ones there are count (054).
- **The keep set** is every path in the file (the union of the last 5 tickets).
- **A ticket counts when its build is handed on**, at Review's Free or at Done's keep, the two moments Otaq can
  measure it (afterwards the build is gone from the worktree). In the default flow every ticket is freed at Review, so
  "finished" in decision 054 reads as "its build reached the stock". A ticket sent back and handed on again is still
  one ticket.
- Written atomically (`.usage.tmp`, then renamed) while the stock's `.incoming` lock is held, the lock every hand-on
  already takes (one keep at a time per stock), so two hand-ons never lose each other's record. Readers (a checkout
  copy) read the last complete file without the lock.
- A ledger that can't be read (missing, damaged, an unknown format) is an empty one: nothing is left out until
  tickets record again. Size: one line per path any of the 5 used (tens of thousands for a Rust workspace, a few MB).
- `.usage` joins `WARM_SIBLING_SUFFIXES` (`data_dir.rs`), so no project's stock can be named `x.usage`;
  `remove_interrupted` keeps `<repo>.usage` and deletes a `<repo>.usage.tmp` a crash left.

### Where it is applied

```rust
// warm/usage.rs: the only module that knows "used", the ledger and arming.

// keep::move_to_stock (Review's Free and Done's keep), once the paths are in `.incoming`, before the record is
// written and the stock adds or replaces:
let trimmed = usage::hand_on(warm, project, ticket, incoming.path(), since).await;
// Measures what `ticket` used in `incoming` (times ≥ since), records it in the ledger, moves the files outside the
// keep set to the trash, arms the rest. Trimmed::{Left { kept, dropped }, Whole}: Whole when reads aren't recorded
// or anything failed (logged); never an error, so a Free or Done never fails because of it.

// seed.rs, Seeder::seed_from_folder (the user's checkout), right after its paths are cloned into the worktree and
// before the timestamp trick:
usage::trim_copy(warm, project, worktree, &cloned).await;
// Drops from the worktree's copy the files outside the keep set (none with an empty ledger: the copy is whole, 055),
// arms the rest. The checkout itself is never written, renamed or re-timed.
```

- **The kept build stays small** because trimming happens as a build enters the stock; a seed from the stock stays
  one `clonefile` per top path (or one rename where builds are moved), as fast as today. A ticket's own worktree is
  never trimmed while it works in it (054): only what leaves it at Review or Done, and a checkout copy before the
  agent starts.
- **Dropped files** are renamed into `<repo>.trash/` (same disk: the stock, its `.incoming` and the worktrees all live
  in the data folder), and deleted there in the background like any trashed entry (`WarmBuilds::to_trash`,
  `empty_trash_later`), so a Free or Done doesn't wait for 200k deletions. A folder none of whose files is kept goes
  as one rename; a folder left empty is not recreated (the build tool makes it again).
- **Symlinks** count like files by their own times; folders are never judged, only the files in them.
- In the stock, entries already there are not re-trimmed when the window moves (where builds are renamed, the stock
  holds several): each was trimmed with the window of its own hand-on, and the stock limit evicts old ones anyway.
- `seed_scratch` (the Merge check's scratch worktree) is unchanged: a scratch build is never handed on, and reading a
  clone never touches its source's times.
- Logging only (054: nothing new on screen): "kept 41,233 files, left out 182,410 nothing used in the last 5
  tickets" at debug level in the hand-on, and the same in the seed's trace. No event, no toast; Cleanup's size
  follows from the smaller entry as today (`measure_later`).

### Throwing old warm builds away once (056): the stock format goes to `stock 2`

- `STOCK_FORMAT` (`warm/store.rs`) becomes `stock 2`; `make_stock` writes it for every new stock.
- `remove_interrupted`, at service start, renames into `<repo>.trash/` (emptied in the background, as today) every
  stock whose marker isn't `stock 2`, every warm build from before the stock (no marker) and every `<repo>.mig`
  folder. They replace today's step 1, which made a pre-stock build entry `0` (`migrate`, `finish_migration` go).
  Because the marker says which format a stock is, this happens once per stock and needs no "done" flag: a stock made
  after the update is `stock 2` and is never thrown away again.
- Worktrees aren't touched (056). A ticket waiting in Review whose freed build was in a thrown-away stock gets the
  nearest build, or none, when it is fetched, as any fetch with an empty stock does today.
- A worktree made before the update still holds its old, unarmed build. Its first hand-on measures it like any other:
  its writes show; a read of a file whose access time was recent and newer than its modified time may not, so a few
  read-only files may be left out once and rebuilt by the next ticket. One slower build, then armed builds from there.

## Considered

- **Stamps on the files instead of a ledger** (a "last used by ticket #n" extended attribute, or the access time
  itself set to the ticket's number). Lost whenever a build tool rewrites a file (new inode, no xattr), different on
  every system (xattrs, NTFS streams), and an access time used as a stamp can't also detect reads. The ledger is one
  file Otaq owns.
- **Trimming at seed instead of at hand-on** (clone only the keep set into each new worktree). The stock would stay
  as big as it is now (054 wants the kept build small too), and a seed would become one clone per file (hundreds of
  thousands of calls at every start) instead of one per top folder.
- **Judging folders, not files** (keep a folder whole when any file in it was used). Safer for tools that assume a
  folder is whole, but Cargo's `target/debug/deps` is one folder with ~300k files, 150 GB in OTAQ-1: it would never
  shrink, which is the point of the ticket.
- **Watching the file system** (FSEvents, inotify, fanotify, ETW) or running builds under a tracer. FSEvents reports
  no reads, fanotify needs privileges, a tracer means wrapping the agent's commands: each is per system and heavier
  than file times, which every system already keeps.
- **A new `build_since` column** set at seed and fetch, for "when the build began". Exact, but two writers (start and
  fetch) of one value, a migration, and nothing for worktrees made before it; the `.git` file's time needs none of
  that and only ever keeps a little more.
- **Throwing old builds away with a flag** in the settings table. The stock marker already says which format a stock
  is; bumping it needs no second place to keep in step.

## Risks and limits (accepted)

- **A tool that assumes its folder is whole.** File-level trimming is right for tools that check each output (Cargo
  rebuilds a missing artifact). A package manager that trusts its own index (npm, pnpm: a package folder is present,
  so it is "installed") may not notice a file inside a package that no recent ticket read, and a later ticket that
  needs it fails with "cannot find module" until it reinstalls. This is decision 054's accepted cost ("rebuilt by the
  next ticket that needs it") on a tool that doesn't rebuild by itself; the project's setup command (`npm ci`) or the
  agent repairs it. Not worked around: any rule for it would be tool-specific (054).
- **Files a build only checks exist** (`stat`, never opened: an up-to-date binary that nothing ran) show no read and
  are left out after 5 tickets; the next ticket that needs them rebuilds them (054's cost).
- **Reads that aren't builds** (Spotlight, a backup, `grep -r` over `target/`) count as used: a file is kept longer,
  never dropped wrongly.
- **Time.** A hand-on walks the build once (`lstat`, plus one `utimensat` per kept file and one rename per dropped
  file or folder): seconds for ~300k files, at Review or Done, off the agent's path. A checkout copy is walked before
  the ticket's agent starts, so that start takes those seconds longer (only when there is no stock).
- **Disks that don't record reads** get no trimming (above): their warm builds behave as today.

## Tests

- `warm/usage.rs` unit tests: the ledger round trip; recording a sixth ticket drops the oldest's bit and the paths
  only it used; a returning ticket moves to the front and keeps its earlier paths; fewer than 5 tickets; a damaged or
  unknown-format file reads as empty; measuring with times set by `File::set_times` (written, read, neither, no
  `since`); arming sets the access time below both the modified time and now − 2 h and leaves the modified time.
- `tests/suite/warm_keep.rs`: a kept build holds the files this ticket and the earlier 4 used, and not the rest; a
  Free records too; with the read probe forced off (a test double, like `CloneProbe::forced`) the build is kept
  whole and no ledger is written; a checkout seed with a ledger trims only the worktree's copy, and the checkout's
  files, their times and count are the same before and after; with an empty ledger the copy is whole.
- `remove_interrupted`: a `stock 1` stock, a pre-stock folder and a `.mig` folder go to the trash; a `stock 2`
  stock and `<repo>.usage` stay; a `<repo>.usage.tmp` is deleted.
- Windows (NTFS, and a Dev Drive if there is one): the read probe's answer, and on a disk where it says yes, a
  hand-on that keeps a read-only file. The Windows VM can run this (Validate).
