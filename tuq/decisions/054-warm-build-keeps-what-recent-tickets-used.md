# 054 · A warm build carries only the files the last 5 tickets' builds used

- **Date:** 2026-11-13
- **Status:** Accepted
- **Related:** 055, 056 (ticket OTAQ-5)

## Summary

When Otaq hands a build to a new ticket (warm builds), it leaves out the build files that none of the project's last
5 finished tickets' builds used. Stale outputs drop out on their own, so a new ticket starts from a small, current
build. It is automatic: no setting, no setup, the same rule for every project, and Otaq still knows no build tool.

## Decision

- **What is left out:** a gitignored file in a kept build that no build in the project's last 5 finished tickets used
  (wrote or read). A file any of those 5 used is kept. Fewer than 5 finished tickets so far: the ones there are count.
- **When:** whenever a build is handed on, so the next ticket's copy is small and the kept build stays small. A
  ticket's own worktree is never trimmed while the ticket works in it.
- **Counted in tickets, not days:** the rule follows the user's pace; a quiet week doesn't throw anything away.
- **Tool-agnostic:** every gitignored file is treated the same, as today. No Cargo-, node_modules- or other
  tool-specific rules, no cleanup command, no size limit.
- **No setting:** not per project, not global. The warm builds switch stays the only control (on by default).
- **Cost accepted:** a file needed only now and then (one no recent ticket built) is rebuilt by the next ticket that
  needs it; that ticket's first build is slower, and the file counts as used again.
- **Nothing new on screen:** Settings → Cleanup keeps listing each project's kept build with its size, largest first,
  with Clear…. Trimming is quiet: no "freed" line, no toast.

## Why

On 2026-11-13 the otaq project's kept build was 211 GB and each new worktree about 345 GB (du), most of it stale
build output: Cargo never deletes old artifacts (OTAQ-1's target/debug/deps held 150 GB in ~299k files,
target/debug/incremental 60 GB), and warmup copied all of it forward to every ticket. The user chose "keep it small
for me, no setup" over a project cleanup command, a size limit, or both; "the last few tickets" over a week or a
month; and 5 tickets over 3 or 10, as a middle ground between copy size and rebuilds. Cleanup already shows the size,
which the user said is enough.
