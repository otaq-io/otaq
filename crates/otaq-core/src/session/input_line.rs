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
//! [`InputLine::Blank`]: empty too, unless lines of the draft may sit above the cursor (a typed line break, a
//! history entry; not a program's own paste on Ctrl+V, which ends at the cursor). A row that starts with the prompt marker and has text after it that isn't faded is
//! [`InputLine::Typed`]: the line holds text, whatever the keys say. Anything else, a hidden cursor (a menu, a
//! permission prompt) or text on a row without a prompt marker (a shell's line) included, is
//! [`InputLine::Unknown`]: the tracker's answer stands.
//!
//! The same replay says whether the screen shows **one of the tool's own dialogs** ([`Reading::dialog`]): a
//! permission or selection prompt, where a typed line would answer the prompt (its Enter picks the option under the
//! pointer) instead of reaching the agent. Otaq types nothing over one: a tool may draw its next prompt before it says
//! so (Copilot's, after "No, and tell Copilot…"), or never say it. A dialog is one of
//!
//! - a hidden cursor (Codex's "Would you like to run the following command?", Copilot's permission prompt, folder
//!   trust and `/model` picker, captured on Copilot 1.0.112), unless the screen draws the tool's own input row and
//!   neither a numbered menu below it nor a picker's keys: Copilot hides its cursor whenever its terminal loses
//!   focus, idle or not, and draws its prompts the same either way (`hidden_cursor_dialog`);
//! - the cursor on a menu's pointed option, with another option next to it (Copilot's `│ ❯ 2. No, and tell Copilot
//!   what to do differently`, which becomes a text field for the user's feedback);
//! - with no input line under the cursor, a numbered menu inside a box (`│ ❯ 1. Yes`) among the screen's last rows.
//!
//! The cursor on the tool's own input line is never a dialog, whatever is drawn above it: the user's sent message is
//! echoed with the same pointers (`❯` in Claude Code and Copilot, `›` in Codex), and one that starts with a numbered
//! list would read as a menu.
//!
//! It also says whether the tool draws that it is still at work below its input line ([`Reading::busy`]): Copilot
//! keeps its input line in sight while a turn runs, and only its footer tells the two apart (`○ Working · 144 B
//! ctrl+c interrupt` with a spinner, `← open sidebar · Interactive · …` when idle).

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

/// What the screen shows: its input line, whether a dialog of the tool's own is drawn, and whether the tool says it is
/// at work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub line: InputLine,
    /// A permission or selection prompt of the tool's own: a typed line would answer it.
    pub dialog: bool,
    /// The cursor is visible in the dialog's own text field (Copilot's "No, and tell Copilot…" option, its feedback
    /// wrapped or not): the user is answering it.
    pub field: bool,
    /// The dialog's question as drawn ("Do you want to run this command?"), when it has one.
    pub question: Option<String>,
    /// A row below the cursor says the tool is at work (Copilot's `◉ Working · …` footer).
    pub busy: bool,
}

/// How many of the screen's last non-empty rows a boxed menu is looked for in: a tool draws its prompt at the bottom,
/// where its input line was.
const DIALOG_ROWS: usize = 16;

/// How far apart two options of one menu may be drawn, in rows.
const OPTION_GAP: usize = 3;

/// Replays `bytes` (the session's kept output) in a terminal of `size` and reads the row the cursor is on.
/// `colors` are the terminal's own text and background colours, for cells drawn in the default colours.
pub fn read(bytes: &[u8], size: TerminalSize, colors: TerminalColors) -> InputLine {
    read_all(bytes, size, colors).line
}

/// [`read`], whether the screen shows a dialog of the tool's own, and whether the tool says it is at work.
pub fn read_all(bytes: &[u8], size: TerminalSize, colors: TerminalColors) -> Reading {
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
    let line = line_of(screen, cols, colors);
    let texts: Vec<String> = screen.rows(0, cols).collect();
    let options: Vec<Option<MenuOption>> = texts.iter().map(|r| menu_option(r)).collect();
    let cursor_row = usize::from(screen.cursor_position().0);
    let menu_next_to = |at: usize| {
        (at.saturating_sub(OPTION_GAP)..=at + OPTION_GAP)
            .any(|i| i != at && options.get(i).is_some_and(Option::is_some))
    };
    // The pointed option that tells the dialog, if any. The cursor on the tool's own input line (a marker, then
    // nothing, a placeholder or the user's text) is no dialog, whatever menu-like echo is drawn above it.
    // The cursor in a dialog's text field: on its pointed option, or on a row of the box under it (the option's
    // feedback wrapped onto more rows).
    let in_field = if screen.hide_cursor() {
        None
    } else if options
        .get(cursor_row)
        .copied()
        .flatten()
        .is_some_and(|o| o.pointed)
        && menu_next_to(cursor_row)
    {
        Some(cursor_row)
    } else {
        in_boxed_menu(&texts, &options, cursor_row)
    };
    let pointed = in_field.or_else(|| {
        (!screen.hide_cursor() && line == InputLine::Unknown)
            .then(|| boxed_menu(&texts, &options, menu_next_to))
            .flatten()
    });
    let dialog = if screen.hide_cursor() {
        hidden_cursor_dialog(&texts, &options, menu_next_to)
    } else {
        pointed.is_some()
    };
    let field = in_field.is_some();
    let question = dialog
        .then(|| {
            pointed.or_else(|| {
                (0..texts.len())
                    .rev()
                    .find(|&i| options[i].is_some_and(|o| o.pointed))
            })
        })
        .flatten()
        .and_then(|at| question_above(&texts, at));
    let busy = !screen.hide_cursor() && texts.iter().skip(cursor_row + 1).any(|r| says_working(r));
    Reading {
        line,
        dialog,
        field,
        question,
        busy,
    }
}

/// Whether a screen with the cursor hidden shows a dialog of the tool's own. There is no row under the cursor to go
/// by: Copilot hides it whenever its terminal loses focus (`ESC[O`, which Otaq's terminal passes on) and leaves it
/// wherever it last drew, over its idle input line too. So the drawn text decides, among the screen's last
/// [`DIALOG_ROWS`] non-empty rows:
///
/// 1. a pointed numbered option with another option next to it and no input row below it is a dialog (Copilot's
///    permission prompt and folder trust, Codex's command question), focused or not;
/// 2. a picker's keys below the input row (`enter to select`, `Esc to cancel`, `⇆ select  enter confirm`) are a
///    dialog (Copilot's `/model`, Claude Code's `/resume`, OpenCode's permission prompt);
/// 3. otherwise the tool's input row ([`input_row`]) is no dialog: the tool is idle, or holds a draft, in a terminal
///    that isn't focused;
/// 4. anything else is a dialog, as a hidden cursor always was.
fn hidden_cursor_dialog(
    texts: &[String],
    options: &[Option<MenuOption>],
    menu_next_to: impl Fn(usize) -> bool,
) -> bool {
    let shown: Vec<usize> = (0..texts.len())
        .filter(|&i| !texts[i].trim().is_empty())
        .collect();
    let last = &shown[shown.len().saturating_sub(DIALOG_ROWS)..];
    let input = input_row(texts, last);
    let menu = last
        .iter()
        .rev()
        .copied()
        .find(|&i| options[i].is_some_and(|o| o.pointed) && menu_next_to(i));
    if menu.is_some_and(|m| input.is_none_or(|i| i < m)) {
        return true;
    }
    match input {
        Some(i) => texts[i + 1..].iter().any(|r| picker_keys(r)),
        None => true,
    }
}

/// The tool's input row among `rows` (the screen's last non-empty ones), with the cursor hidden: the lowest row that
/// starts with an input marker (`┃`, `❯`, `›`, `>`) in its first two columns and isn't a menu's numbered option, if
/// that row is no item of a list (a row under it starts where its text does: Claude Code's `> No, exit` over `Yes, I
/// trust this folder`) and no row below it starts with a pointer (Claude Code's `/resume` list under its `> /resume`).
/// The column limit leaves out Copilot's transcript scrollbar (a `┃` at its last column, a row of its own where the
/// transcript is empty) and OpenCode's bars (`┃` further in); the marker list leaves out a box's corners.
fn input_row(texts: &[String], rows: &[usize]) -> Option<usize> {
    let lead = |row: &str| row.chars().take_while(|c| c.is_whitespace()).count();
    let at = rows.iter().rev().copied().find(|&i| {
        let row = &texts[i];
        lead(row) <= 1
            && row.trim_start().starts_with(['┃', '❯', '›', '>'])
            && menu_option(row).is_none()
    })?;
    let row = &texts[at];
    let after = row.trim_start().chars().skip(1).collect::<String>();
    let text_at = lead(row) + 1 + lead(&after);
    let listed = !after.trim().is_empty()
        && texts
            .get(at + 1)
            .is_some_and(|next| !next.trim().is_empty() && lead(next) == text_at);
    let pointer_below = texts[at + 1..].iter().any(|r| {
        r.trim_start_matches(|c: char| c.is_whitespace() || "│┃║".contains(c))
            .starts_with(['❯', '›', '>', '▶', '►', '➤'])
    });
    (!listed && !pointer_below).then_some(at)
}

/// Whether the row tells a picker's keys: `↑/↓ to navigate · enter to select · esc to cancel` (Copilot), `Enter to
/// confirm · Esc to cancel` (Claude Code), `⇆ select  enter confirm` (OpenCode). An input line's footer tells none
/// (`← open sidebar · Interactive · …`, `shift+tab to cycle`, `esc to interrupt`).
fn picker_keys(row: &str) -> bool {
    let row = row.to_lowercase();
    [
        "to select",
        "to confirm",
        "to cancel",
        "to navigate",
        "enter select",
        "enter confirm",
        "\u{21c6} select",
    ]
    .iter()
    .any(|keys| row.contains(keys))
}

/// How many rows a boxed menu's pointed option may run over (its text field wrapping the user's feedback).
const FIELD_ROWS: usize = 8;

/// The row of the boxed pointed option whose box the cursor row is in, below it (`│ ❯ 2. No, and tell…` and the
/// rows its feedback wraps onto, every one behind the box's `│`).
fn in_boxed_menu(
    texts: &[String],
    options: &[Option<MenuOption>],
    cursor_row: usize,
) -> Option<usize> {
    let boxed = |i: usize| {
        texts
            .get(i)
            .is_some_and(|t| t.trim_start().starts_with('\u{2502}'))
    };
    if !boxed(cursor_row) {
        return None;
    }
    (cursor_row.saturating_sub(FIELD_ROWS)..cursor_row)
        .rev()
        .take_while(|&i| boxed(i))
        .find(|&i| options[i].is_some_and(|o| o.pointed && o.boxed))
}

/// A menu option's row: `1. Yes`, `  2) No` (`❯`, `›`, `>`, `▶` as the pointer), behind a box's border or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MenuOption {
    /// The pointer is on it.
    pointed: bool,
    /// Drawn inside a box: a border (`│`, `┃`, `║`) before it.
    boxed: bool,
}

fn menu_option(row: &str) -> Option<MenuOption> {
    let start = row.trim_start();
    let boxed = start.starts_with(['│', '┃', '║']);
    let row = start.trim_start_matches(|c: char| c.is_whitespace() || "│┃║".contains(c));
    let (pointed, rest) = match row.chars().next() {
        Some(c) if "❯›>▶►➤".contains(c) => (true, row[c.len_utf8()..].trim_start()),
        _ => (false, row),
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let after = &rest[digits..];
    (digits > 0 && (after.starts_with(". ") || after.starts_with(") ")))
        .then_some(MenuOption { pointed, boxed })
}

/// The row of a boxed menu's pointed option among the screen's last non-empty rows, with another option next to it.
fn boxed_menu(
    texts: &[String],
    options: &[Option<MenuOption>],
    menu_next_to: impl Fn(usize) -> bool,
) -> Option<usize> {
    let shown: Vec<usize> = (0..texts.len())
        .filter(|&i| !texts[i].trim().is_empty())
        .collect();
    shown[shown.len().saturating_sub(DIALOG_ROWS)..]
        .iter()
        .copied()
        .find(|&i| options[i].is_some_and(|o| o.pointed && o.boxed) && menu_next_to(i))
}

/// The question a dialog asks: the nearest row above its pointed option that ends with `?`, without a box's borders.
fn question_above(texts: &[String], pointed: usize) -> Option<String> {
    (pointed.saturating_sub(8)..pointed).rev().find_map(|i| {
        let text = texts[i].trim_matches(|c: char| c.is_whitespace() || "│┃║".contains(c));
        (text.ends_with('?') && menu_option(text).is_none()).then(|| text.to_owned())
    })
}

/// Whether the row says the tool is at work: `◉ Working · 123 B ctrl+c interrupt` (Copilot's footer, after its
/// spinner).
fn says_working(row: &str) -> bool {
    let row = row.trim();
    let mut chars = row.chars();
    let rest = match chars.next() {
        Some(c) if !c.is_alphanumeric() => chars.as_str().trim_start(),
        _ => row,
    };
    rest.starts_with("Working \u{b7}")
}

/// The row the cursor is on.
fn line_of(screen: &vt100::Screen, cols: u16, colors: TerminalColors) -> InputLine {
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

/// A tool's prompt marker: one character that isn't a letter or digit, and either not ASCII (`›`, `❯`, `┃`) or `>`.
/// ASCII punctuation the user may have typed (`/`, `!`, `@`) isn't one, nor a dialog box's thin border `│` (Copilot's
/// input line is the heavy `┃`; its permission prompt's box is drawn with `│`).
fn is_prompt_marker(text: &str) -> bool {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return false;
    };
    !c.is_alphanumeric() && c != '\u{2502}' && (!c.is_ascii() || c == '>')
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
            line("\x1b[3;1H\u{2503} \x1b[38;2;90;90;90mAsk\x1b[39m\x1b[3;3H"),
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

    /// What [`read_all`] gives for a capture: its input line, whether it is a dialog, and whether the tool says it is
    /// at work.
    fn reading_of(name: &str) -> (InputLine, bool, bool) {
        let r = read_all(&capture(name), SIZE, DARK);
        (r.line, r.dialog, r.busy)
    }

    /// Codex's command question (cursor hidden, `› 1. Yes, proceed (y)`) is a dialog; no tool's input line is, empty
    /// or typed.
    #[test]
    fn a_tools_own_dialog_is_told_from_its_input_line() {
        assert!(reading_of("codex-approval").1);
        for name in [
            "codex-after-approval",
            "codex-empty",
            "codex-typed",
            "claude-empty",
            "claude-typed",
            "copilot-empty",
            "copilot-typed",
            "opencode-empty",
            "opencode-typed",
        ] {
            assert!(!reading_of(name).1, "{name}");
        }
    }

    /// Copilot 1.0.112 (see the fixtures' README): its permission prompt hides the cursor; with the pointer on "No, and
    /// tell Copilot what to do differently" that option is a text field, the cursor visible in it, before and after
    /// the user's feedback is typed: a dialog all the same. At work after the answer it shows its input line as when
    /// idle, and only its footer (`○ Working · …`) says it is at work.
    #[test]
    fn copilots_permission_prompt_and_its_work() {
        assert_eq!(
            reading_of("copilot-approval"),
            (InputLine::Unknown, true, false)
        );
        let r = read_all(&capture("copilot-approval"), SIZE, DARK);
        assert_eq!(
            r.question.as_deref(),
            Some("Do you want to run this command?")
        );
        // The box's `│` is no input line's marker: no input line under the cursor, and the user is in the field.
        assert!(!r.field);
        for name in ["copilot-approval-no", "copilot-approval-feedback"] {
            assert_eq!(
                reading_of(name),
                (InputLine::Unknown, true, false),
                "{name}"
            );
            assert!(read_all(&capture(name), SIZE, DARK).field, "{name}");
        }
        assert_eq!(
            reading_of("copilot-working"),
            (InputLine::Blank, false, true)
        );
        assert_eq!(
            reading_of("copilot-idle-after"),
            (InputLine::Blank, false, false)
        );
        assert_eq!(
            reading_of("copilot-empty"),
            (InputLine::Blank, false, false)
        );
    }

    /// Long feedback wraps inside Copilot's box: the cursor on the box's next row (`│   …more feedback…`), under the
    /// pointed option, is still in the dialog's text field.
    #[test]
    fn feedback_wrapped_inside_copilots_box_is_the_dialog() {
        let mut bytes = capture("copilot-approval-feedback");
        bytes.extend_from_slice(
            "\x1b[28;1H\u{2502}     and then run git status to check the result\x1b[28;52H\x1b[?25h".as_bytes(),
        );
        let r = read_all(&bytes, SIZE, DARK);
        assert!(r.dialog && r.field, "{r:?}");
        assert_eq!(r.line, InputLine::Unknown);
        assert!(!r.busy);
        assert_eq!(
            r.question.as_deref(),
            Some("Do you want to run this command?")
        );
    }

    fn reading(bytes: &str) -> Reading {
        read_all(bytes.as_bytes(), TerminalSize { cols: 60, rows: 14 }, DARK)
    }

    /// A permission prompt drawn in a box (`│ ❯ 1. Yes`, …) is a dialog with the cursor hidden, on its pointed option,
    /// or anywhere but on an input line; so is an unboxed menu with the cursor hidden (Codex's).
    #[test]
    fn a_numbered_menu_with_a_pointer_is_a_dialog() {
        let menu = "\x1b[3;1H\u{2502} Do you want to run this command?\
                    \x1b[4;1H\u{2502} \u{276f} 1. Yes\
                    \x1b[5;1H\u{2502}   2. Yes, and approve `git` for this session\
                    \x1b[6;1H\u{2502}   3. No, and tell Copilot what to do differently (Esc)";
        for cursor in ["\x1b[?25l", "\x1b[9;1H", "\x1b[4;5H"] {
            let r = reading(&format!("{menu}{cursor}"));
            assert!(r.dialog, "{cursor:?}: {r:?}");
            assert_eq!(
                r.question.as_deref(),
                Some("Do you want to run this command?")
            );
        }
        // The pointer on another option.
        assert!(reading("\x1b[3;3H1. Yes\x1b[4;1H\u{203a} 2. No (esc)\x1b[?25l").dialog);
        // A hidden cursor over a shell's output.
        assert!(reading("abc\r\n\x1b[?25l").dialog);
        assert!(!reading("abc\r\n").dialog);
    }

    /// The user's sent message echoed with the tool's pointer, starting with a numbered list, over the tool's own input
    /// line: an idle screen, not a dialog (Claude Code's `❯` and placeholder, Codex's `›` and placeholder, Copilot's
    /// `❯` echo and bare `┃` line). Nor an agent's numbered list.
    #[test]
    fn a_numbered_message_over_the_input_line_is_no_dialog() {
        let echo = |pointer: char| {
            format!(
                "\x1b[2;1H{pointer} 1. Fix the coupon field\x1b[3;3H2. Fix the gift card total\
                 \x1b[5;1H\u{25cf} Both are fixed."
            )
        };
        let claude = format!(
            "{}\x1b[8;1H\u{276f} \x1b[2mTry \"fix lint\"\x1b[22m\x1b[8;3H",
            echo('\u{276f}')
        );
        let codex = format!(
            "{}\x1b[8;1H\u{203a} \x1b[2mAsk Codex\x1b[22m\x1b[8;3H",
            echo('\u{203a}')
        );
        let copilot = format!("{}\x1b[8;1H\u{2503} ", echo('\u{276f}'));
        // Close to the input line, as right after the user sent it.
        let copilot_at_once = "\x1b[6;3H\u{276f} 1. Fix the coupon field\x1b[7;5H2. Fix the gift card total\
                               \x1b[8;1H\u{2503} ";
        // The same under a box's border: Copilot's input line is `┃` too.
        let boxed =
            "\x1b[6;1H\u{2502} \u{276f} 1. Fix it\x1b[7;1H\u{2502}   2. Test it\x1b[8;1H\u{2503} ";
        for (name, screen) in [
            ("claude", claude.as_str()),
            ("codex", codex.as_str()),
            ("copilot", copilot.as_str()),
            ("copilot at once", copilot_at_once),
            ("boxed", boxed),
        ] {
            let r = reading(screen);
            assert!(!r.dialog, "{name}: {r:?}");
            assert_ne!(r.line, InputLine::Unknown, "{name}");
        }
        let list = "\x1b[3;1H\u{25cf} Plan:\x1b[4;3H1. Read the spec\x1b[5;3H2. Write the test\
                    \x1b[9;1H\u{2503} ";
        assert!(!reading(list).dialog);
        // A user's own draft that starts like an option, alone on the input line.
        assert!(!reading("\x1b[9;1H\u{276f} 1. Read the spec").dialog);
    }

    /// Copilot 1.0.112 hides its cursor when its terminal loses focus (`ESC[O`) and leaves it on its logo or at the
    /// screen's end, its idle input line still drawn: no dialog, in its fresh layout (`❯` between two rules) and in
    /// its `┃` layout (replayed through `otaq attach`). Focus back (`ESC[I`) shows the cursor on the line again.
    #[test]
    fn copilots_idle_screen_in_an_unfocused_terminal_is_no_dialog() {
        for name in ["copilot-idle-unfocused", "copilot-idle-unfocused-attach"] {
            assert_eq!(
                reading_of(name),
                (InputLine::Unknown, false, false),
                "{name}"
            );
        }
        assert_eq!(
            reading_of("copilot-idle-refocused"),
            (InputLine::Blank, false, false)
        );
    }

    /// Copilot's prompts read as dialogs with its terminal unfocused (cursor hidden, drawn as when focused): its
    /// permission prompt, raw and replayed through `otaq attach`, its folder trust, its `/model` picker (no numbers),
    /// and "No, and tell Copilot…" once focus leaves its field.
    #[test]
    fn copilots_prompt_in_an_unfocused_terminal_is_still_a_dialog() {
        for name in [
            "copilot-approval-unfocused",
            "copilot-approval-unfocused-attach",
        ] {
            let r = read_all(&capture(name), SIZE, DARK);
            assert!(r.dialog, "{name}: {r:?}");
            assert_eq!(
                r.question.as_deref(),
                Some("Do you want to run this command?"),
                "{name}"
            );
        }
        let trust = read_all(&capture("copilot-trust"), SIZE, DARK);
        assert!(trust.dialog, "{trust:?}");
        assert_eq!(
            trust.question.as_deref(),
            Some("Do you trust the files in this folder?")
        );
        assert!(reading_of("copilot-model-unfocused").1);
        let mut no = capture("copilot-approval-no");
        no.extend_from_slice(b"\x1b[?25l");
        assert!(read_all(&no, SIZE, DARK).dialog);
    }

    /// The other tools' pickers and prompts with the cursor hidden, their terminal unfocused: Claude Code's folder
    /// trust (`> No, exit` over `Yes, I trust this folder`, no numbers) and `/resume` (its list under the `> /resume`
    /// echo), OpenCode's permission prompt (`Allow once   Allow always   Reject` buttons inside its `┃` bar).
    #[test]
    fn other_tools_pickers_in_an_unfocused_terminal_are_dialogs() {
        for name in [
            "claude-trust-unfocused",
            "claude-resume-unfocused",
            "opencode-permission-unfocused",
        ] {
            assert_eq!(
                reading_of(name),
                (InputLine::Unknown, true, false),
                "{name}"
            );
        }
    }

    /// With the cursor hidden (a terminal that isn't focused), the input row decides: a numbered echo or a draft over
    /// it isn't a dialog; a menu with no input row below it, an unnumbered list, or a picker's keys are.
    #[test]
    fn a_hidden_cursor_over_the_input_row_is_no_dialog() {
        const HIDE: &str = "\x1b[?25l";
        let echo = "\x1b[6;3H\u{276f} 1. Fix the coupon field\x1b[7;5H2. Fix the gift card total\x1b[9;1H\u{2503} ";
        let boxed =
            "\x1b[6;1H\u{2502} \u{276f} 1. Fix it\x1b[7;1H\u{2502}   2. Test it\x1b[9;1H\u{2503} ";
        let footer = "\x1b[11;2H\u{2190} open sidebar \u{b7} Interactive \u{b7} / commands";
        for screen in [echo, boxed, "\x1b[9;1H\u{2503} draft text"] {
            let r = reading(&format!("{screen}{footer}\x1b[2;5H{HIDE}"));
            assert!(!r.dialog, "{screen:?}: {r:?}");
            assert_eq!(r.line, InputLine::Unknown);
        }
        // A boxed menu with the transcript's scrollbar (`┃` at the last column) beside it.
        let scrollbar = format!(
            "\x1b[2;60H\u{2503}\x1b[3;60H\u{2503}\x1b[4;60H\u{2503}\
             \x1b[6;1H\u{2502} Do you want to run this command?\
             \x1b[7;1H\u{2502} \u{276f} 1. Yes\x1b[8;1H\u{2502}   2. No\x1b[9;1H\u{2570}\u{2500}\u{2500}{HIDE}"
        );
        assert!(reading(&scrollbar).dialog);
        // An unnumbered list, its pointer on the first item, and the same with a picker's keys under a search field.
        assert!(
            reading(&format!(
                "\x1b[5;2H> Sonnet\x1b[6;4HOpus\x1b[7;4HHaiku{HIDE}"
            ))
            .dialog
        );
        assert!(
            reading(&format!(
                "\x1b[5;2H\u{276f} Fusion\x1b[8;2H\u{276f}  Search models\x1b[10;2H\u{2191}/\u{2193} to navigate \u{b7} enter to select{HIDE}"
            ))
            .dialog
        );
        // A list under the echo of the command that opened it.
        assert!(
            reading(&format!(
                "\x1b[3;1H> /resume\x1b[7;3H> Fix the build\x1b[8;5H2 days ago{HIDE}"
            ))
            .dialog
        );
    }

    /// The footer says it: `◉ Working · …` below the cursor is the tool at work; the same words above it (the
    /// transcript) or no spinner word aren't.
    #[test]
    fn a_working_footer_is_the_tool_at_work() {
        assert!(
            reading("\x1b[8;1H\u{2503} \x1b[10;1H \u{25c9} Working \u{b7} 12 B ctrl+c\x1b[8;3H")
                .busy
        );
        assert!(
            !reading(
                "\x1b[8;1H\u{2503} \x1b[10;1H \u{2190} open sidebar \u{b7} Interactive\x1b[8;3H"
            )
            .busy
        );
        assert!(!reading("\x1b[2;1H\u{25cf} Working \u{b7} hard\x1b[8;1H\u{2503} ").busy);
    }
}
