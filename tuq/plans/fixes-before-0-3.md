# Plan · fixes-before-0-3

- **Ticket:** none
- **Date:** 2026-11-15
- **Decisions:** none
- **Tech spec:** none (the four issues were reproduced in the 0.3 bring-together of skills-in-their-own-repos, PR #73)
- **Orchestrator prompt:** tuq/prompts/fixes-before-0-3-orchestrator.md

<!-- tuq:plan:start -->

The four issues still open on PR #73 (branch skills-repos-decisions), all reproduced, are fixed in five pieces, one
behaviour each, all side by side: none builds on another's code. Each is narrow on purpose: it changes only what its
behaviour needs, keeps every other tool's and every other command's behaviour as it is, and proves that with tests of
what must not change. Two choices were agreed with the maintainer on 2026-11-15: Codex agents may run `git commit`
without a prompt (a prefix rule, like `otaq`), and an Otaq with a non-default data folder leaves the shared Codex home
alone unless CODEX_HOME is set. Decision d0011 still holds: Codex gets no approval, sandbox or bypass flag at launch.
The agent in charge follows tuq/prompts/fixes-before-0-3-orchestrator.md.

## Pieces

1. A Codex agent passes long text to otaq without leaving the sandbox — waits for nothing
   Reproduced on Codex 0.169 in a fresh CODEX_HOME (ChatGPT sign-in): `otaq ask human --stdin <<'Q' … Q` and
   `otaq stage next --stdin <<'C' … C` print "the background service isn't running", while plain `otaq prime` works,
   so the stage goes Idle or needs approvals. Likely cause, to be confirmed first by hand: Codex's
   `prefix_rule(pattern=["otaq"], decision="allow")` in `rules/otaq.rules` (written by
   `crates/otaq-core/src/agent_tools/codex/setup.rs`) matches only a plain command; a heredoc or a redirect doesn't
   match, so the command runs inside Codex's sandbox, which can't reach the service's socket in the data folder.
   Start with the hands-on check (a scratch CODEX_HOME with only `auth.json`; never the real `~/.codex`): which
   command forms run unsandboxed under the rule (plain, a quoted argument, `--file <path>`, a heredoc, `<`), and
   where a sandboxed Codex command may write a file (the worktree, `$TMPDIR`). Then give every `otaq` command an
   agent feeds long text through `--stdin` (`crates/otaq-cli/src/cli.rs`: ask human, stage next/done, comment,
   update, helper done, answer, report and the rest that take `stdin: bool`) a way that is a plain command, e.g.
   `--file <path>` read the same way as stdin, where it doesn't exist yet, and tell Codex agents to use it: a
   Codex-only line in the prime Otaq prints for Codex sessions (`crates/otaq-core/src/prime.rs`; it already knows the
   session's tool), never a change to the shared skills' text, so Claude Code, Copilot and OpenCode read exactly what
   they read today. The file goes where the check showed Codex may write that isn't the worktree (an uncommitted file
   there blocks `otaq stage next`). `--stdin` keeps working unchanged everywhere.
   Check: in the scratch Codex home, a Product spec stage asks through the ticket and ends with `otaq stage next`
   with no permission prompt and no "isn't running"; existing `--stdin` CLI tests pass unchanged; the prime of a
   Claude Code, Copilot and OpenCode session is byte-for-byte what it was (snapshot).
2. A Codex agent commits without a prompt — waits for nothing
   Agreed 2026-11-15: `rules/otaq.rules` (`crates/otaq-core/src/agent_tools/codex/setup.rs`, written beside the
   `otaq`, `tuq` and `tuq-plan` rules) also gets `prefix_rule(pattern=["git", "commit"], decision="allow")`, so a
   ticket's commit (the worktree's git data lives in the main checkout's `.git`, outside Codex's sandbox) runs
   without a prompt. Only `git commit`: `git push`, `git reset` and every other command still ask. An existing
   rules file with the old content counts as outdated, so the setup runs again on upgrade (piece 3 of
   skills-in-their-own-repos already re-runs it when the rules differ: reuse that, safe to run twice); disconnect
   still removes the whole file. Update the module's doc comment.
   Check: setup tests show the new rule and an older rules file rewritten once; in a scratch Codex home a stage's
   `git commit` runs with no prompt and `git push --dry-run` still asks.
3. A second Otaq leaves the shared Codex home alone unless told — waits for nothing
   Agreed 2026-11-15. Today the Otaq that started last takes over Codex's one marketplace ("a marketplace added from
   another folder is removed first", `codex/setup.rs`) and rewrites `rules/otaq.rules`, so a dev build with its own
   OTAQ_HOME repoints the real `~/.codex`. New rule: an Otaq whose data folder isn't the default one changes the
   Codex home (setup, refresh of the fixed plugin, hook trust, rules, folder trust, disconnect) only when CODEX_HOME
   is set (`codex_home` in `crates/otaq-core/src/agent_tools/codex/mod.rs`, `EnvVar`); otherwise Codex shows as not
   usable in Settings → Connected tools with a line saying to set CODEX_HOME to use Codex with this Otaq, and no Codex
   command or file write happens. The default data folder and any Otaq with CODEX_HOME set behave exactly as today.
   Find how the data folder knows it is the default (`crates/otaq-core/src/data_dir.rs`, OTAQ_HOME). Tests use their
   own data folder: make sure the existing Codex tests (which set CODEX_HOME) are unaffected, and add tests for both
   sides. Mention it in scripts/README.md where a second Otaq is run.
   Check: with a non-default OTAQ_HOME and no CODEX_HOME, starting the service leaves a stand-in `~/.codex` byte for
   byte unchanged and Settings says why Codex isn't available; with CODEX_HOME set it sets up as before.
4. Copilot never updates itself during a session — waits for nothing
   Seen in the Windows VM: Copilot CLI updated itself from 1.0.109 to 1.0.112 during Otaq sessions. Add
   `--no-auto-update` to every Copilot launch Otaq makes (new session and resume; `launch` in
   `crates/otaq-core/src/agent_tools/copilot/launch.rs`, beside `--plugin-dir` and `--allow-tool`), and to any other
   Copilot command Otaq runs that could update (look for one-off runs, detect, sign-in; leave `copilot --version` as
   is if it doesn't update). Confirm the flag exists in the Copilot versions Otaq supports (1.0.109 and 1.0.112 both
   accept it: it was used by hand in the VM); an unknown flag must not break a launch on an older Copilot, so check
   the oldest version Otaq accepts. Update the launch tests' expected arguments.
   Check: the Copilot launch tests show the flag on new and resumed sessions; a Copilot stage in the Windows VM starts
   with it in its command line.
5. The floating-layers test passes on Windows — waits for nothing
   `crates/otaq-desktop/ui/src/components/Floating.test.ts` ("every useFloatingStyle user draws through
   FloatingPortal", line ~195) filters `readdirSync(src, { recursive: true })` results with
   `/(^|\/)components\/Floating\.tsx$/`, but Windows returns `components\Floating.tsx`, so the test fails there
   (from commit 21fc636a, after 0.2). Normalise the separators before matching (or match `[\\/]`), and check the
   same file and its neighbours for other path checks with `/`. Test-only change: no product code.
   Check: `npx vitest run src/components/Floating.test.ts` passes on macOS and in the Windows VM.

<!-- tuq:plan:end -->
