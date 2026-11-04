//! A session's terminal as the user sees it, as plain text (SHL-10, d0087): `otaq show <KEY> --terminal`.
//!
//! The kept scrollback ([`super::Scrollback`], the last 2 MB) is replayed through a headless terminal emulator
//! (`vt100`) sized like the session's terminal, so a full-screen redraw (cursor moves, clears, a permission prompt
//! drawn over the last one) comes out as it shows on screen rather than as the raw stream. The text is the visible
//! screen plus up to [`LINES_ABOVE`] lines above it, without colours or escape codes. It runs only when asked:
//! no emulator is kept per session. Output older than the kept scrollback is gone (d0045).

use super::TerminalSize;

/// How many lines above the visible screen the text keeps (d0087: "about the last 200").
pub const LINES_ABOVE: usize = 200;

/// Replays `bytes` (a session's kept output) in a terminal of `size` and returns the screen plus up to
/// [`LINES_ABOVE`] lines above it: trailing spaces dropped from every line, empty lines at the end dropped.
pub fn render(bytes: &[u8], size: TerminalSize) -> String {
    render_above(bytes, size, LINES_ABOVE)
}

/// [`render`], keeping up to `above` lines above the screen (`otaq run log --tail N`, agents run the app).
pub fn render_above(bytes: &[u8], size: TerminalSize, above: usize) -> String {
    let (rows, cols) = (size.rows.max(1), size.cols.max(1));
    let mut parser = vt100::Parser::new(rows, cols, above);
    // A trimmed scrollback can start inside a character: its continuation bytes would show as "�".
    let start = bytes
        .iter()
        .take(3)
        .take_while(|b| (0x80..0xc0).contains(*b))
        .count();
    parser.process(&bytes[start..]);
    let screen = parser.screen_mut();

    // The lines above the screen, read a screen at a time from the top: at offset `k` the first visible row is
    // the `k`-th line above the screen.
    let mut lines = Vec::new();
    screen.set_scrollback(usize::MAX);
    let mut above = screen.scrollback();
    while above > 0 {
        screen.set_scrollback(above);
        let take = above.min(usize::from(rows));
        lines.extend(screen.rows(0, cols).take(take));
        above -= take;
    }
    screen.set_scrollback(0);
    lines.extend(screen.rows(0, cols));

    let mut text = String::new();
    for line in &lines {
        text.push_str(line.trim_end());
        text.push('\n');
    }
    text.truncate(text.trim_end().len());
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(cols: u16, rows: u16) -> TerminalSize {
        TerminalSize { cols, rows }
    }

    #[test]
    fn plain_lines() {
        assert_eq!(
            render(b"$ npm test\r\n\r\n12 passing\r\n", size(80, 24)),
            "$ npm test\n\n12 passing"
        );
        assert_eq!(render(b"", size(80, 24)), "");
    }

    /// A Claude Code-like permission prompt: the spinner line redrawn in place, then the screen cleared and the
    /// prompt drawn with cursor moves, colours and a box; what's left is the prompt as on screen.
    #[test]
    fn a_full_screen_redraw_gives_the_prompt_as_on_screen() {
        let mut sample = Vec::new();
        sample.extend_from_slice(b"\x1b[?25l");
        for frame in ["\u{273b} Thinking\u{2026}", "\u{273d} Thinking\u{2026}"] {
            sample.extend_from_slice(b"\r\x1b[2K\x1b[38;5;174m");
            sample.extend_from_slice(frame.as_bytes());
            sample.extend_from_slice(b"\x1b[39m");
        }
        sample.extend_from_slice(b"\x1b[2J\x1b[H");
        sample.extend_from_slice(
            "\x1b[1m\x1b[38;2;215;119;87m\u{256d}\u{2500}\u{2500}\u{256e}\x1b[0m\r\n".as_bytes(),
        );
        sample.extend_from_slice(b"\x1b[3;3H\x1b[1mBash command\x1b[22m");
        sample.extend_from_slice(b"\x1b[5;5H\x1b[2mrm -rf .next\x1b[22m");
        sample.extend_from_slice(b"\x1b[7;3HDo you want to proceed?");
        sample.extend_from_slice("\x1b[8;3H\x1b[36m\u{276f} 1. Yes\x1b[39m          ".as_bytes());
        sample.extend_from_slice(b"\x1b[9;3H  2. No, and tell Claude what to do differently");
        // A stray line drawn further down, then erased again.
        sample.extend_from_slice(b"\x1b[12;1Hstale status line\x1b[12;1H\x1b[2K\x1b[8;5H");
        assert_eq!(
            render(&sample, size(60, 14)),
            "\u{256d}\u{2500}\u{2500}\u{256e}\n\
             \n  Bash command\n\n    rm -rf .next\n\n  Do you want to proceed?\n  \u{276f} 1. Yes\n\
             \x20   2. No, and tell Claude what to do differently"
        );
    }

    #[test]
    fn colours_are_stripped() {
        let text = render(
            b"\x1b[31merror\x1b[0m: \x1b[1;4;38;5;208mbuild\x1b[0m failed\r\n",
            size(80, 24),
        );
        assert_eq!(text, "error: build failed");
    }

    #[test]
    fn only_the_last_200_lines_above_the_screen_are_kept() {
        let mut output = String::new();
        for n in 1..=500 {
            output.push_str(&format!("line {n}\r\n"));
        }
        output.push_str("$ ");
        let text = render(output.as_bytes(), size(80, 24));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), LINES_ABOVE + 24);
        // The screen shows lines 478…500 and the prompt; the 200 lines above it are 278…477.
        assert_eq!(lines[0], "line 278");
        assert_eq!(lines[LINES_ABOVE - 1], "line 477");
        assert_eq!(lines[LINES_ABOVE], "line 478");
        assert_eq!(*lines.last().unwrap(), "$");
    }

    #[test]
    fn trailing_spaces_and_empty_end_lines_are_dropped() {
        let text = render(b"ready   \r\n  indented  \r\n\r\n\r\n   \r\n", size(40, 10));
        assert_eq!(text, "ready\n  indented");
    }

    /// The kept scrollback was trimmed inside a character and an escape sequence: the rest still renders.
    #[test]
    fn a_scrollback_that_starts_mid_line_still_renders() {
        let mut bytes = "\u{2713} done".as_bytes()[1..].to_vec();
        bytes.extend_from_slice(b";31mhalf a colour\x1b[0m\r\nnext line\r\n");
        let text = render(&bytes, size(80, 24));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].ends_with("half a colour"), "{text}");
        assert!(!lines[0].contains('\u{fffd}'), "{text}");
        assert_eq!(lines[1], "next line");
    }

    #[test]
    fn long_lines_wrap_at_the_terminal_width() {
        assert_eq!(render(b"abcdefghij", size(4, 5)), "abcd\nefgh\nij");
    }
}
