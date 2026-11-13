# 01 · A long one-line message Otaq types into Claude Code loses all but its last read

**Kind:** bug · **Release:** must fix (logged as a blocker) · **From:** re-test 7 NEW-12 · **Decision:** none needed

## What happens
The merge-message writer's request (one line, 1,296 characters, `crates/otaq-core/src/pr/message.rs` `prompt`)
reached Claude Code v2.1.322 on Windows as only its last 272 characters (2 of 2), so the written message never comes
and Approve stays "still writing". Messages up to 1,024 bytes arrive whole, multi-line ones arrive whole, and Codex
gets the same request whole.

The "1,024" is not a cut. The terminal hands Claude Code a long write in reads of at most 1,024 bytes (macOS: 1,022).
When a read is longer than 800 characters Claude Code takes it as a paste. When the read right after it is 800 or
shorter, Claude Code takes that one as typing, and the typing replaces what the paste had just put in the input line.
Only the last read is left. **macOS is affected the same way** (1,296 → last 274 characters), so this is not a
Windows bug.

## Repro
Outside the app, with a harness of our own (portable-pty 0.9.0 as Otaq uses it; on Windows with Otaq's ConPTY,
`conpty.dll` + `OpenConsole.exe` 1.25.260930003): start `claude --session-id <uuid>` in a scratch folder, wait for its
input line, write a 1,296-byte line with no line break in one write (what `type_and_submit` writes), then `\r`. The
user turn in `~/.claude/projects/<folder>/<uuid>.jsonl` holds the last 272 bytes (macOS: 274).

## Cause (measured, high confidence)
1. **What Otaq writes.** `Deliveries::type_and_submit` (`crates/otaq-core/src/session/deliveries.rs`) writes the text
   in one `write_all` and adds bracketed paste (`ESC[200~ … ESC[201~`) only when the text has a `\n`. Claude Code's
   `InputQuirks` add nothing (`after_text`, `before_enter` empty, no Enter delay), so after 80 ms (`SUBMIT_PAUSE`) Otaq
   sends `\r`. That is the only path that types Otaq's text into a session (`session_requests.rs` passes on the
   user's own keys). Nothing in Otaq splits or cuts the write.
2. **The terminal splits it, losing nothing.** A Node raw-mode reader under the same ConPTY got 1,296 bytes as reads
   of **1,024 + 272** (+ the `\r`), and 3,000 bytes as 1,024 + 1,024 + 952. The bytes all arrive. Several small writes
   with no gap between them (512-byte chunks) are joined back into reads of 1,024. On macOS the pty does the same
   at 1,022 bytes (1,022 + 274). So the split comes from the terminal and not from Otaq.
3. **Claude Code drops the earlier reads.** Claude Code 2.1.322's input hook (`handleKeyDown` in its bundled code)
   treats a key event whose text is longer than `_Z=800` as a paste, when it is not inside bracketed-paste markers
   (`A.key.length>_Z` → `onPaste`). A read of 801 or more goes down the paste path, and a read of 800 or fewer goes down
   the typing path. When a typing read comes right behind a paste read, before Claude Code has redrawn, the typing
   replaces the line, so the paste is lost. These cases pin it down:
   - 1,824 bytes (1,024 + 800) → only the last 800 are kept; 1,825 bytes (1,024 + 801) → whole (two paste blocks).
   - 3,000 bytes (1,024 + 1,024 + 952: all pastes) → whole, as three `<pasted_content>` blocks.
   - 1,296 bytes written as 1,000, then a 500 ms pause, then 296 → whole (a paste block, then typing). With no
     pause → only the last 274.
   - The same bytes inside bracketed paste, in any number of writes → whole, as one paste.

Claude Code's own race is the root cause, and the terminal's read size is what triggers it. Otaq can avoid it,
because Claude Code joins a bracketed paste whatever the reads are.

## Fix direction
In `type_and_submit`, use bracketed paste for Claude Code's long single-line text too, not only when the text has a
line break. Write `ESC[200~` + text + `ESC[201~` in one write, then Enter as now. 80 ms later is enough: a bracketed
1,296-byte line, the 1,255-byte writer request and a 3,000-byte 6-line text all arrived whole with Enter 80 ms
after. Two simple ways to do it:
- a Claude Code input quirk ("paste single-line text"), or
- a length rule for every tool: bracket any text longer than about 512 bytes, which keeps clear of Claude Code's 800.

For Claude Code, bracketing every text is also safe on the evidence: a bracketed 300-byte line arrived as plain text,
not as a paste block, and a bracketed `/name args` line was still taken as a slash command ("Unknown command:
/new12nope", the same as typed). Not recommended: sending writes of 800 bytes or less with pauses between them. It
worked (512-byte writes 50 ms apart, 700-byte writes 100 ms apart), but it depends on timing and is slow.

## Risks to check
- Claude Code records a paste over 800 characters in its conversation as `\n\n<pasted_content id="…">\n…\n</pasted_content
  id="…">`, so the agent reads the message inside those tags. That already happens today to every message of
  801–1,024 bytes. Otaq's own reader of user turns (`agent_tools/claude/transcript.rs` `user_text`) will see the tags
  too: check where that text is shown (for example a title or the first message).
- Otaq's skill prompts (`/otaq:address-review SHOP-18 …`, `/otaq:restack`) go through the same path. A bracketed slash
  line still ran as a command on macOS: check it with a real Otaq skill on Windows once.
- Codex, Copilot and OpenCode: leave them as they are unless the length rule is chosen. In that case, check that
  Codex's paste (its `after_text` space, its 0.1 s Enter delay and the `before_enter` key on Windows) still submits
  bracketed single-line text.
- A real-Claude-Code test like the one on `round6-piece7-claude-1kb-wip` (`claude_typing.rs`): the writer request, a
  3 KB line and a 3 KB 6-line text each arrive whole, comparing text with the `<pasted_content>` tags taken out.

## Evidence
Measured 2026-11-12 on the VMPal Windows VM (Windows 11 ARM64, Claude Code 2.1.322 signed in, Otaq's ConPTY) and on
macOS (Claude Code 2.1.322, a separate instance in a scratch folder, started with a clean environment). Each case
starts a new Claude Code session, types once, presses Enter (2 s later unless the row says 80 ms), reads the user
turn from the conversation file and then stops the session. The text is "Do nothing; reply with only the word OK. "
followed by place markers (`<01021>___`), so the first byte kept shows where the cut is. "Arrived" is the turn's text
with Claude Code's `<pasted_content>` tags taken out.

| Typed (bytes, how) | Reads the program got | Claude Code got | Whole |
|---|---|---|---|
| 800, one line, one write | 800 | 800, as typing | yes |
| 1,000, one line, one write | 1,000 | 1,000, one paste block | yes |
| 1,024, one line, one write | 1,024 | 1,024, one paste block | yes |
| 1,025, one line, one write | 1,024 + 1 | **the last 1** | no |
| 1,100, one line, one write | 1,024 + 76 | **the last 76** | no |
| 1,296, one line, one write (NEW-12) | 1,024 + 272 | **the last 272** | no |
| writer request 1,255, one write | 1,024 + 231 | **the last 231** | no |
| 1,824, one line, one write | 1,024 + 800 | **the last 800** | no |
| 1,825, one line, one write | 1,024 + 801 | 1,825, two paste blocks | yes |
| 2,100, one line, one write | 1,024 + 1,024 + 52 | **the last 52** | no |
| 3,000, one line, one write | 1,024 + 1,024 + 952 | 3,000, three paste blocks | yes |
| 3,000, 6 lines, one write, no markers | (as above) | 3,000, three paste blocks | yes |
| 1,296 in 512-byte writes, no gap | 1,024 + 272 (joined) | **the last 272** | no |
| 1,296 in 512-byte writes, 50 ms apart | 512 each | 1,296, as typing | yes |
| 3,000 in 700-byte writes, 100 ms apart | 700 each | 3,000, as typing | yes |
| bracketed 1,025 / 1,100 / 1,296 / 3,000, one write | (1,296: 1,024 + 284) | whole, one paste block | yes |
| bracketed 3,000, 6 lines (today's multi-line path) | 1,024 + 1,024 + 964 | 3,000, one paste block | yes |
| bracketed 3,000 in 1,000-byte writes, 200 ms apart | 1,000 each | 3,000, one paste block | yes |
| bracketed writer request 1,255, one write | | 1,255, one paste block | yes |
| bracketed 1,296 / 3,000 6 lines / writer request, **Enter 80 ms after** | | whole, one paste block | yes |
| 800, one line, Enter 80 ms after | 800 | 800 | yes |
| **macOS** 800, one line, one write | 800 | 800 | yes |
| **macOS** 1,296, one line, one write | 1,022 + 274 | **the last 274** | no |
| **macOS** 2,100, one line, one write | 1,022 + 1,022 + 56 | **the last 56** | no |
| **macOS** bracketed 1,296 | 1,022 + 286 | 1,296, one paste block | yes |
| **macOS** 1,296 as 1,000, then 296 after 500 ms | 1,000 + 296 | 1,296 (paste block + typing) | yes |
| **macOS** 1,296 as 1,000 + 296 with no gap / 500-byte writes with no gap | 1,022 + 274 | **the last 274** | no |
| **macOS** bracketed 300 | | 300, as plain text (no block) | yes |
| **macOS** `/new12nope reply …` typed / bracketed | | "Unknown command: /new12nope" both ways | n/a |

"Reads the program got" is what a Node raw-mode reader logged under the same terminal (Windows: Otaq's ConPTY; macOS:
the pty). The rows without a value weren't measured with that reader. Windows' own ConPTY (conhost) wasn't measured:
the reader produced no output under it when started from the VM's command runner. The installed app doesn't use it.

The 800 threshold is in Claude Code 2.1.322's bundled code: `var _Z=800`, and in the input hook
`if((e||n)&&!A.ctrl&&!A.meta&&A.key.length>_Z&&!A.defaultPrevented){… z(U.text,…)}`, which sends that key's text to
`onPaste`.

Probe scripts are in the VM at `%USERPROFILE%\otaq-build\new12`: `probe\src\main.rs` is the harness (portable-pty,
`new12probe <conpty-dir|system> <program|echo:…> <variant> <len> <lines> <outdir> [textfile]`, with variants
`single`, `paste`, `chunk<N>_<ms>` and `pastechunk<N>_<ms>`; `P_ENTER_MS` sets the Enter delay). `probe\echo.js` is the
reader. `claude-run.ps1` and `echo-run.ps1` run the cases, `check.js` compares the results, and `results\r1`–`r3`
hold the arrived turns and raw screens. Nothing in the repo changed for this, and no commits.
