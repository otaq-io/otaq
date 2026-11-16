# Terminal captures for the input-line and dialog readings

`src/session/input_line.rs`'s tests replay these files. Each one is raw program output as Otaq's own ConPTY passed it
on Windows 11 ARM64, kept byte for byte, at 120 × 30 with Otaq's dark theme colours (text `#D6D9E0` on `#14161C`).

| File | What |
|---|---|
| `<tool>-empty.bin`, `<tool>-typed.bin` | each tool idle with an empty input line, then after typing `abc` |
| `codex-approval.bin`, `codex-after-approval.bin` | Codex's "Would you like to run the following command?" (cursor hidden), then its input line after `y` |
| `copilot-approval.bin` | Copilot 1.0.112's permission prompt: a box with "Do you want to run this command?", `❯ 1. Yes`, `2. No, and tell Copilot what to do differently (Esc to stop)`; cursor hidden |
| `copilot-approval-no.bin` | the same prompt with the pointer moved to option 2, which becomes a text field: the cursor is visible in it |
| `copilot-approval-feedback.bin` | the same, with the user's feedback typed into option 2 |
| `copilot-working.bin` | Copilot at work after that refusal and feedback: its usual `┃` input line, and the footer `○ Working · 144 B ctrl+c interrupt` |
| `copilot-idle-after.bin` | Copilot idle again after the turn: the same input line, the footer `← open sidebar · Interactive · …` |
| `copilot-idle-unfocused.bin` | Copilot idle in its fresh layout (`❯` between two rules), then focus out (`ESC[O`): it redraws its logo with the cursor hidden and leaves it there |
| `copilot-idle-refocused.bin` | the same, then focus in (`ESC[I`): the cursor back on its input line, shown |
| `copilot-idle-unfocused-attach.bin` | Copilot idle in its `┃` layout after a turn, its terminal unfocused (cursor hidden at the screen's end); replayed through `otaq attach` |
| `copilot-approval-unfocused.bin`, `copilot-approval-unfocused-attach.bin` | Copilot's permission prompt with its terminal unfocused, raw and replayed through `otaq attach`: drawn as when focused, cursor hidden |
| `copilot-trust.bin` | Copilot's "Confirm folder trust" prompt (`❯ 1. Yes`, …), cursor hidden |
| `copilot-model-unfocused.bin` | Copilot's `/model` picker, unfocused: no numbers, `❯ Copilot Fusion ✓` and `❯  Search models…` over `↑/↓ to navigate · … enter to select · esc to cancel`, cursor hidden (focused too) |
| `claude-trust-unfocused.bin` | Claude Code 2.1.325's folder trust prompt, unfocused: `> No, exit` over `Yes, I trust this folder`, no numbers, `Enter to confirm · Esc to cancel`, cursor hidden |
| `claude-resume-unfocused.bin` | Claude Code's `/resume` picker, unfocused: its list (`> Mkdir newdir command`) under the `> /resume` echo, cursor hidden |
| `opencode-permission-unfocused.bin` | OpenCode 1.19.5's permission prompt (`Allow once   Allow always   Reject`, `⇆ select  enter confirm`), unfocused, cursor hidden |

The Copilot 1.0.112 files (2026-11-15) come from a real signed-in session (model MAI-Code-1.1-Flash) in a scratch
repository (`~\cap-copilot\repo`). A small ConPTY host started Copilot, answered the terminal's colour and cursor
queries, sent the keys for each step and saved the output at each one. The Windows user name in the folder path is
replaced with `otaq-tester`, which has the same length so the layout stays the same.

The unfocused captures (2026-11-16) come from the same kind of host (`conrec.ps1`, 120 × 30, the same colour
answers), which sends the focus reports Otaq's terminal passes on (`ESC[O` when it loses focus, `ESC[I` when it gets
it back); each file is the output up to the step it is named after. Copilot 1.0.112, Claude Code 2.1.325 and OpenCode
1.19.5 ran in scratch repositories. The two `-attach` files are Otaq's kept output of a real Copilot session (the
release test's TOOL-8) replayed by `otaq attach` inside the host: a second ConPTY drew it again, so the screen is exact
but the bytes aren't Copilot's own.
