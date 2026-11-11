//! What the session's input line shows on screen, for the unsent-input flag ([`super::InputTracker`]).
//!
//! The tracker sees only keystrokes, so a key that a tool's own menu took (Codex's `y` on "Would you like to run
//! the following command?") still counts as text in the input line, and Otaq's notes would wait for a line that
//! is in fact empty. And a tool that pastes the clipboard itself on Ctrl+V (Codex) puts text there no key accounts
//! for, so one Backspace can bring the tracker's count to nothing over a full line. So the screen is read too,
//! whatever the tracker says: the kept output is replayed through a headless terminal ([`vt100`], as
//! [`super::screen`] does) and the row the cursor is on is the input line.
//!
//! The line is **empty** when, on that row,
//!
//! - left of the cursor there is nothing but the tool's prompt marker (`›`, `❯`, `┃`, `>`: one cell that isn't a
//!   letter or digit), and
//! - right of the cursor there is only the tool's **placeholder**: text drawn faded, either dim (SGR 2: Codex's
//!   "Ask Codex to do anything", Claude Code's "Try …") or in a colour much closer to the background than the
//!   terminal's own text colour (OpenCode's grey "Ask anything…"). Tools show a placeholder only while the line
//!   is empty, whatever its words.
//!
//! A row with the prompt marker and nothing after it (Copilot's empty line has no placeholder) is
//! [`InputLine::Blank`]: empty too, unless the draft may run over several lines, whose other lines can sit above
//! the cursor. A row that starts with the prompt marker and has text after it that isn't faded is
//! [`InputLine::Typed`]: the line holds text, whatever the keys say. Anything else, a hidden cursor (a menu, a
//! permission prompt) or text on a row without a prompt marker (a shell's line) included, is
//! [`InputLine::Unknown`]: the tracker's answer stands.

use crate::board::settings::TerminalColors;

use super::TerminalSize;

/// What the row under the cursor shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputLine {
    /// The prompt marker and the tool's faded placeholder: the line is empty.
    Placeholder,
    /// The prompt marker and nothing after it.
    Blank,
    /// The prompt marker and text after it: the line holds text.
    Typed,
    /// No input line to go by.
    Unknown,
}

/// How much closer to the background a placeholder's colour is than the terminal's text colour: below this share
/// of the text colour's contrast, text counts as faded.
const FADED: f32 = 0.65;
/// The same on a background the tool paints itself, as a share of the most contrast that background allows.
const FADED_ON_OWN: f32 = 0.5;

/// Replays `bytes` (the session's kept output) in a terminal of `size` and reads the row the cursor is on.
/// `colors` are the terminal's own text and background colours, for cells drawn in the default colours.
pub fn read(bytes: &[u8], size: TerminalSize, colors: TerminalColors) -> InputLine {
    let (rows, cols) = (size.rows.max(1), size.cols.max(1));
    let mut parser = vt100::Parser::new(rows, cols, 0);
    // A trimmed scrollback can start inside a character.
    let start = bytes
        .iter()
        .take(3)
        .take_while(|b| (0x80..0xc0).contains(*b))
        .count();
    parser.process(&bytes[start..]);
    let screen = parser.screen();
    if screen.hide_cursor() {
        return InputLine::Unknown;
    }
    let (row, cursor) = screen.cursor_position();
    let cells = (0..cols).filter_map(|col| {
        screen
            .cell(row, col)
            .filter(|c| !c.is_wide_continuation())
            .map(|c| (col, c))
    });

    // The row starts with the prompt marker (left of the cursor).
    let mut marker = false;
    let mut left = 0;
    let mut placeholder = false;
    let mut text = false;
    for (col, cell) in cells {
        let contents = cell.contents();
        if contents.chars().all(char::is_whitespace) {
            continue;
        }
        if col < cursor {
            left += 1;
            if left == 1 && is_prompt_marker(contents) {
                marker = true;
            } else {
                text = true;
            }
        } else if faded(cell, colors) {
            placeholder = true;
        } else {
            text = true;
        }
    }
    match (text, placeholder, marker) {
        (true, _, true) => InputLine::Typed,
        // Text on a row without a prompt marker: a shell's line, or output.
        (true, _, false) => InputLine::Unknown,
        (false, true, _) => InputLine::Placeholder,
        (false, false, true) => InputLine::Blank,
        // An empty row without even a prompt marker is no input line to go by.
        (false, false, false) => InputLine::Unknown,
    }
}

/// A tool's prompt marker: one character that isn't a letter or digit, and either not ASCII (`›`, `❯`, `┃`, `│`) or
/// `>`. ASCII punctuation the user may have typed (`/`, `!`, `@`) isn't one.
fn is_prompt_marker(text: &str) -> bool {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return false;
    };
    !c.is_alphanumeric() && (!c.is_ascii() || c == '>')
}

/// Whether the cell's text is drawn faded: dim, or in a colour much closer to its background than the terminal's
/// text colour is.
fn faded(cell: &vt100::Cell, colors: TerminalColors) -> bool {
    if cell.dim() {
        return true;
    }
    let (fg, bg) = if cell.inverse() {
        (cell.bgcolor(), cell.fgcolor())
    } else {
        (cell.fgcolor(), cell.bgcolor())
    };
    // On the terminal's own background the tool's text is in the terminal's text colour; on a background of its
    // own (OpenCode paints its palette), the most contrast that background allows stands in for it.
    let own_background = bg != vt100::Color::Default;
    let bg = lightness(rgb(bg, if cell.inverse() { colors.fg } else { colors.bg }));
    let fg = lightness(rgb(fg, if cell.inverse() { colors.bg } else { colors.fg }));
    let this = (fg - bg).abs();
    if own_background {
        this < bg.max(255.0 - bg) * FADED_ON_OWN
    } else {
        let text = (lightness(rgb_of(colors.fg)) - bg).abs();
        text > 0.0 && this < text * FADED
    }
}

fn rgb_of(color: u32) -> [u8; 3] {
    let [_, r, g, b] = color.to_be_bytes();
    [r, g, b]
}

/// A cell's colour as RGB: `default` for the terminal's own colour, xterm's palette for the 256 indexed ones.
fn rgb(color: vt100::Color, default: u32) -> [u8; 3] {
    match color {
        vt100::Color::Default => rgb_of(default),
        vt100::Color::Rgb(r, g, b) => [r, g, b],
        vt100::Color::Idx(i) => indexed(i),
    }
}

fn indexed(i: u8) -> [u8; 3] {
    const BASE: [[u8; 3]; 16] = [
        [0, 0, 0],
        [205, 0, 0],
        [0, 205, 0],
        [205, 205, 0],
        [0, 0, 238],
        [205, 0, 205],
        [0, 205, 205],
        [229, 229, 229],
        [127, 127, 127],
        [255, 0, 0],
        [0, 255, 0],
        [255, 255, 0],
        [92, 92, 255],
        [255, 0, 255],
        [0, 255, 255],
        [255, 255, 255],
    ];
    match i {
        0..=15 => BASE[usize::from(i)],
        16..=231 => {
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            let i = i - 16;
            [level(i / 36), level(i / 6 % 6), level(i % 6)]
        }
        _ => {
            let v = 8 + (i - 232) * 10;
            [v, v, v]
        }
    }
}

/// Perceived lightness, 0–255.
fn lightness([r, g, b]: [u8; 3]) -> f32 {
    0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The terminal colours of Otaq's dark theme, as the captures were answered.
    const DARK: TerminalColors = TerminalColors {
        fg: 0xD6D9E0,
        bg: 0x14161C,
    };
    const LIGHT: TerminalColors = TerminalColors {
        fg: 0x2A2C31,
        bg: 0xFAFAF8,
    };
    /// The captures' terminal: 120 × 30.
    const SIZE: TerminalSize = TerminalSize {
        cols: 120,
        rows: 30,
    };

    /// Real output of each tool in Windows' ConPTY (Otaq's), captured cell for cell: idle with an empty input line,
    /// then after typing `abc`.
    fn capture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/tests/fixtures/input-line/{name}.bin",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// Codex draws "› Ask Codex to do anything" with the placeholder dim (SGR 2) and the cursor on its first letter.
    #[test]
    fn codexs_placeholder_is_an_empty_line() {
        assert_eq!(
            read(&capture("codex-empty"), SIZE, DARK),
            InputLine::Placeholder
        );
        assert_eq!(read(&capture("codex-typed"), SIZE, DARK), InputLine::Typed);
    }

    /// The release test's case: `y` answered Codex's "Would you like to run the following command?" and Codex went
    /// back to its empty input line. While the question shows, the cursor is hidden: nothing to go by.
    #[test]
    fn codex_after_its_command_question_shows_an_empty_line() {
        assert_eq!(
            read(&capture("codex-approval"), SIZE, DARK),
            InputLine::Unknown
        );
        assert_eq!(
            read(&capture("codex-after-approval"), SIZE, DARK),
            InputLine::Placeholder
        );
    }

    #[test]
    fn claude_codes_placeholder_is_an_empty_line() {
        assert_eq!(
            read(&capture("claude-empty"), SIZE, DARK),
            InputLine::Placeholder
        );
        assert_eq!(read(&capture("claude-typed"), SIZE, DARK), InputLine::Typed);
    }

    /// OpenCode's placeholder isn't dim: it is grey (128,128,128) on its own background, typed text near white.
    #[test]
    fn opencodes_grey_placeholder_is_an_empty_line() {
        for colors in [DARK, LIGHT] {
            assert_eq!(
                read(&capture("opencode-empty"), SIZE, colors),
                InputLine::Placeholder
            );
            assert_eq!(
                read(&capture("opencode-typed"), SIZE, colors),
                InputLine::Typed
            );
        }
    }

    /// Copilot shows no placeholder: its empty line is the `┃` marker and nothing else.
    #[test]
    fn copilots_empty_line_is_blank() {
        assert_eq!(
            read(&capture("copilot-empty"), SIZE, DARK),
            InputLine::Blank
        );
        assert_eq!(
            read(&capture("copilot-typed"), SIZE, DARK),
            InputLine::Typed
        );
    }

    fn line(bytes: &str) -> InputLine {
        read(bytes.as_bytes(), TerminalSize { cols: 40, rows: 5 }, DARK)
    }

    /// The rule is about how the text is drawn, not its words.
    #[test]
    fn any_faded_text_after_the_marker_is_a_placeholder() {
        assert_eq!(
            line("\x1b[3;1H> \x1b[2mType your message\x1b[22m\x1b[3;3H"),
            InputLine::Placeholder
        );
        // xterm's grey (index 8) and a dark grey truecolour on the default background.
        assert_eq!(
            line("\x1b[3;1H\u{276f} \x1b[90mwhat next?\x1b[39m\x1b[3;3H"),
            InputLine::Placeholder
        );
        assert_eq!(
            line("\x1b[3;1H\u{2502} \x1b[38;2;90;90;90mAsk\x1b[39m\x1b[3;3H"),
            InputLine::Placeholder
        );
        // On a background of the tool's own, light and dark.
        assert_eq!(
            line("\x1b[3;1H\x1b[48;2;250;250;250m> \x1b[38;2;140;140;140mAsk\x1b[3;3H"),
            InputLine::Placeholder
        );
        assert_eq!(
            line("\x1b[3;1H\x1b[48;2;30;30;30m> \x1b[38;2;128;128;128mAsk\x1b[3;3H"),
            InputLine::Placeholder
        );
        // A tool without a prompt marker.
        assert_eq!(
            line("\x1b[3;1H\x1b[2mAsk me\x1b[22m\x1b[3;1H"),
            InputLine::Placeholder
        );
    }

    #[test]
    fn text_in_the_line_is_text() {
        // Typed text left of the cursor, also with a faded suggestion after it.
        assert_eq!(line("\x1b[3;1H> fix it"), InputLine::Typed);
        assert_eq!(
            line("\x1b[3;1H> /he\x1b[2mlp\x1b[22m\x1b[3;5H"),
            InputLine::Typed
        );
        // A line ending in a space (a skill typed into Codex, `$push ` left unsent), the cursor after the space.
        assert_eq!(line("\x1b[3;1H\u{203a} $push "), InputLine::Typed);
        // Punctuation the user typed isn't a marker.
        assert_eq!(line("\x1b[3;1H/"), InputLine::Unknown);
        assert_eq!(line("\x1b[3;1H> /"), InputLine::Typed);
        // The cursor moved back to the start of the text: it is right of the cursor, in the text's own colour.
        assert_eq!(line("\x1b[3;1H> abc\x1b[3;3H"), InputLine::Typed);
        // Coloured text that isn't faded (blue on the dark background, its own or the terminal's), and text on a
        // light background of the tool's own.
        assert_eq!(
            line("\x1b[3;1H> \x1b[38;2;92;156;245mabc\x1b[39m\x1b[3;3H"),
            InputLine::Typed
        );
        assert_eq!(
            line("\x1b[3;1H\x1b[48;2;10;10;10m> \x1b[38;2;92;156;245mabc\x1b[3;3H"),
            InputLine::Typed
        );
        assert_eq!(
            line("\x1b[3;1H\x1b[48;2;250;250;250m> \x1b[38;2;40;40;40mabc\x1b[3;3H"),
            InputLine::Typed
        );
    }

    #[test]
    fn no_input_line_in_sight_is_unknown() {
        // A hidden cursor: a menu or a permission prompt.
        assert_eq!(
            line("\x1b[3;1H> \x1b[2mAsk\x1b[22m\x1b[3;3H\x1b[?25l"),
            InputLine::Unknown
        );
        // An empty row without a marker: a program that doesn't echo, or a draft's second line.
        assert_eq!(line("\x1b[3;1H"), InputLine::Unknown);
        assert_eq!(line("abc\r\n"), InputLine::Unknown);
        // A shell's line: its prompt isn't a marker, so its text says nothing either way.
        assert_eq!(line("\x1b[3;1H$ echo hi"), InputLine::Unknown);
        assert_eq!(line("\x1b[3;1Hhalf-typed"), InputLine::Unknown);
    }

    #[test]
    fn the_marker_alone_is_blank() {
        assert_eq!(line("\x1b[3;1H> "), InputLine::Blank);
        assert_eq!(line("\x1b[3;1H\u{2503} "), InputLine::Blank);
    }
}
