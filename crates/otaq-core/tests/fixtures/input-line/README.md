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

The Copilot 1.0.112 files (2026-11-15) come from a real signed-in session (model MAI-Code-1.1-Flash) in a scratch
repository (`~\cap-copilot\repo`). A small ConPTY host started Copilot, answered the terminal's colour and cursor
queries, sent the keys for each step and saved the output at each one. The Windows user name in the folder path is
replaced with `otaq-tester`, which has the same length so the layout stays the same.
