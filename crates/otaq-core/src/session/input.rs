//! The "unsent input" flag (SVC-06, d0048): whether a session's input line holds text the user typed but hasn't
//! sent. Every keystroke from the app or `otaq attach` passes through [`InputTracker::feed`]; Otaq's own
//! typing doesn't. The flag is what keeps Otaq from mixing its text with the user's (P7).
//!
//! The tracker sees only bytes, not the program's screen, so it errs on the side of "there is text":
//!
//! - Printable characters (and pasted text) set the flag and count one character each.
//! - **Enter** (`\r`) sends the line: the flag clears and an "Enter pressed" is reported (ATT-02). A line break
//!   inside the input (`\n` = Ctrl+J, or ESC + Enter) counts as a character.
//! - **Backspace** removes one counted character; backspacing to empty clears the flag, **but only while the
//!   count is exact**. Moving the cursor (←/→/Home/End, Ctrl+A/E/B/F, Alt+B/F) makes it inexact, because a
//!   backspace at the start of the line deletes nothing. Up/down (history, menus) may fill the line, so they
//!   set the flag with an inexact count.
//! - A line break (`\n`, ESC + Enter, or one inside pasted text) makes the draft **multi-line**, as does a
//!   history entry, which may hold several lines. Some tools' clear-line keys clear only the current line of such a
//!   draft (ADP-06), so it is the user's to clear (SES-07).
//! - Keys that delete in other ways (Ctrl+U/K/W, Alt+Backspace, Ctrl+C, Esc) leave the flag as it is: the
//!   tracker can't know what they removed. The next Enter or Otaq's [`clear_line`](InputTracker::clear_line)
//!   resets it.
//!
//! Escape sequences may arrive split across writes; the parser keeps its state between them.
//!
//! **The line's text** is followed as well as bytes allow, for an answer typed in the terminal (d0119): typed and
//! pasted characters are added, Backspace takes the last one off, Ctrl+U and Ctrl+C empty it, and Enter reports it
//! ([`InputEffect::sent`]). A line recalled from history, or edited after moving the cursor, may not be what the
//! program shows; a recalled line reports no text at all.

/// What one write did to the input line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputEffect {
    /// How many times Enter was pressed.
    pub enters: usize,
    /// The text of each line sent with Enter, in order, trimmed: empty for an empty line (or one whose text the
    /// tracker couldn't follow, a history entry).
    pub sent: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Empty,
    Text {
        /// Characters counted on the line.
        count: usize,
        /// Whether the count can be trusted for backspacing to empty.
        exact: bool,
        /// The draft may run over several lines.
        multi_line: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parse {
    Ground,
    /// After ESC.
    Escape,
    /// Inside `ESC [ …`, collecting parameter bytes (for bracketed paste markers).
    Csi,
    /// After `ESC O`: one more byte.
    Ss3,
    /// Inside a bracketed paste: everything is text until `ESC [ 2 0 1 ~`.
    Paste,
    /// After ESC inside a paste.
    PasteEscape,
    PasteCsi,
}

#[derive(Debug, Clone)]
pub struct InputTracker {
    line: Line,
    parse: Parse,
    params: Vec<u8>,
    /// The line's text as far as it can be followed, in UTF-8 bytes.
    text: Vec<u8>,
    /// The line came from history: its text is unknown until it is sent or cleared.
    recalled: bool,
}

impl Default for InputTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl InputTracker {
    pub fn new() -> Self {
        Self {
            line: Line::Empty,
            parse: Parse::Ground,
            params: Vec::new(),
            text: Vec::new(),
            recalled: false,
        }
    }

    /// Whether the input line may hold unsent text.
    pub fn has_unsent_text(&self) -> bool {
        self.line != Line::Empty
    }

    /// Whether the unsent text may run over several lines.
    pub fn has_multi_line_text(&self) -> bool {
        matches!(
            self.line,
            Line::Text {
                multi_line: true,
                ..
            }
        )
    }

    /// Otaq cleared the line itself (with the bytes of the session's agent tool, ADP-06).
    pub fn clear_line(&mut self) {
        self.line = Line::Empty;
        self.text.clear();
        self.recalled = false;
    }

    /// Takes keystrokes typed by a person.
    pub fn feed(&mut self, bytes: &[u8]) -> InputEffect {
        let mut effect = InputEffect::default();
        for &b in bytes {
            self.byte(b, &mut effect);
        }
        // A lone ESC at the end of a write is the Esc key, not the start of a sequence.
        if self.parse == Parse::Escape {
            self.parse = Parse::Ground;
        }
        effect
    }

    fn byte(&mut self, b: u8, effect: &mut InputEffect) {
        match self.parse {
            Parse::Ground => match b {
                b'\r' => {
                    self.line = Line::Empty;
                    effect.enters += 1;
                    let sent = if self.recalled {
                        String::new()
                    } else {
                        String::from_utf8_lossy(&self.text).trim().to_owned()
                    };
                    effect.sent.push(sent);
                    self.text.clear();
                    self.recalled = false;
                }
                0x1b => self.parse = Parse::Escape,
                b'\n' => self.add_line_break(),
                0x7f | 0x08 => self.backspace(),
                // Ctrl+A, Ctrl+E, Ctrl+B, Ctrl+F: cursor movement.
                0x01 | 0x05 | 0x02 | 0x06 => self.cursor_moved(),
                // Ctrl+P, Ctrl+N: history.
                0x10 | 0x0e => self.history(),
                // Ctrl+C, Ctrl+U: the text is gone from the line, though the flag can't trust it.
                0x03 | 0x15 => self.text.clear(),
                // Other control keys (Tab, …) don't add text and can't be trusted to remove it.
                0x00..=0x1f => {}
                // UTF-8 continuation bytes belong to the character already counted.
                0x80..=0xbf => self.text.push(b),
                _ => self.add_text(b),
            },
            Parse::Escape => match b {
                b'[' => {
                    self.params.clear();
                    self.parse = Parse::Csi;
                }
                b'O' => self.parse = Parse::Ss3,
                b'\r' => {
                    // Alt/Option+Enter: a line break inside the input.
                    self.add_line_break();
                    self.parse = Parse::Ground;
                }
                b'b' | b'f' => {
                    self.cursor_moved();
                    self.parse = Parse::Ground;
                }
                0x1b => {}
                _ => self.parse = Parse::Ground,
            },
            Parse::Csi => {
                if (0x40..=0x7e).contains(&b) {
                    self.parse = Parse::Ground;
                    match b {
                        b'A' | b'B' => self.history(),
                        b'C' | b'D' | b'H' | b'F' => self.cursor_moved(),
                        b'~' if self.params == b"200" => self.parse = Parse::Paste,
                        // Home/End as `ESC [ 1 ~` / `ESC [ 4 ~` and friends.
                        b'~' if matches!(self.params.as_slice(), b"1" | b"4" | b"7" | b"8") => {
                            self.cursor_moved()
                        }
                        _ => {}
                    }
                } else {
                    self.params.push(b);
                }
            }
            Parse::Ss3 => {
                self.parse = Parse::Ground;
                match b {
                    b'A' | b'B' => self.history(),
                    b'C' | b'D' | b'H' | b'F' => self.cursor_moved(),
                    _ => {}
                }
            }
            Parse::Paste => match b {
                0x1b => self.parse = Parse::PasteEscape,
                0x80..=0xbf => self.text.push(b),
                b'\r' | b'\n' => self.add_line_break(),
                _ => self.add_text(b),
            },
            Parse::PasteEscape => {
                if b == b'[' {
                    self.params.clear();
                    self.parse = Parse::PasteCsi;
                } else {
                    self.add_text(0x1b);
                    self.parse = Parse::Paste;
                }
            }
            Parse::PasteCsi => {
                if (0x40..=0x7e).contains(&b) {
                    self.parse = if b == b'~' && self.params == b"201" {
                        Parse::Ground
                    } else {
                        Parse::Paste
                    };
                } else {
                    self.params.push(b);
                }
            }
        }
    }

    fn add_text(&mut self, b: u8) {
        if b >= 0x20 {
            self.text.push(b);
        }
        self.add(false);
    }

    fn add_line_break(&mut self) {
        self.text.push(b'\n');
        self.add(true);
    }

    fn add(&mut self, line_break: bool) {
        self.line = match self.line {
            Line::Empty => Line::Text {
                count: 1,
                exact: true,
                multi_line: line_break,
            },
            Line::Text {
                count,
                exact,
                multi_line,
            } => Line::Text {
                count: count + 1,
                exact,
                multi_line: multi_line || line_break,
            },
        };
    }

    fn backspace(&mut self) {
        // The last character: its continuation bytes, then its first byte.
        while self.text.pop().is_some_and(|b| (0x80..=0xbf).contains(&b)) {}
        if let Line::Text {
            count,
            exact: true,
            multi_line,
        } = self.line
        {
            self.line = if count <= 1 {
                Line::Empty
            } else {
                // The count can't tell which character went, so a line break once typed stays counted.
                Line::Text {
                    count: count - 1,
                    exact: true,
                    multi_line,
                }
            };
        }
    }

    fn cursor_moved(&mut self) {
        if let Line::Text {
            count, multi_line, ..
        } = self.line
        {
            self.line = Line::Text {
                count,
                exact: false,
                multi_line,
            };
        }
    }

    fn history(&mut self) {
        self.recalled = true;
        let count = match self.line {
            Line::Empty => 1,
            Line::Text { count, .. } => count,
        };
        self.line = Line::Text {
            count,
            exact: false,
            multi_line: true,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(chunks: &[&[u8]]) -> (InputTracker, usize) {
        let mut t = InputTracker::new();
        let enters = chunks.iter().map(|c| t.feed(c).enters).sum();
        (t, enters)
    }

    #[test]
    fn typing_sets_and_enter_clears() {
        let (t, enters) = after(&[b"hello"]);
        assert!(t.has_unsent_text());
        assert_eq!(enters, 0);
        let (t, enters) = after(&[b"hello", b"\r"]);
        assert!(!t.has_unsent_text());
        assert_eq!(enters, 1);
        let (_, enters) = after(&[b"\r\r"]);
        assert_eq!(enters, 2, "Enter on an empty line still counts");
    }

    /// d0119: the text of each sent line, for an answer typed in the terminal.
    #[test]
    fn enter_reports_the_line_it_sent() {
        let sent = |chunks: &[&[u8]]| {
            let mut t = InputTracker::new();
            chunks
                .iter()
                .flat_map(|c| t.feed(c).sent)
                .collect::<Vec<_>>()
        };
        assert_eq!(sent(&[b"Post", b"mark\r"]), ["Postmark"]);
        assert_eq!(sent(&[b"\r"]), [""], "an empty line");
        assert_eq!(sent(&[b"  \r"]), [""], "only spaces");
        assert_eq!(sent(&[b"SESx\x7f\r"]), ["SES"]);
        assert_eq!(sent(&["Résumé\x7f\x7fé\r".as_bytes()]), ["Résué"]);
        assert_eq!(sent(&[b"one\rtwo\r"]), ["one", "two"]);
        assert_eq!(sent(&[b"wrong\x15right\r"]), ["right"], "Ctrl+U");
        assert_eq!(sent(&[b"wrong\x03"],), Vec::<String>::new());
        assert_eq!(sent(&[b"\x1b[200~a\rb\x1b[201~\r"]), ["a\nb"]);
        assert_eq!(
            sent(&[b"\x1b[A\r"]),
            [""],
            "a recalled line's text is unknown"
        );
        assert_eq!(sent(&[b"\x1b[Ax\r", b"y\r"]), ["", "y"]);
    }

    #[test]
    fn arrows_and_backspace_that_leave_text_keep_it() {
        let (t, _) = after(&[b"abc", b"\x1b[D", b"\x1b[C", b"\x7f"]);
        assert!(t.has_unsent_text());
        let (t, _) = after(&[b"abc", b"\x7f\x7f"]);
        assert!(t.has_unsent_text());
    }

    #[test]
    fn backspacing_to_empty_clears() {
        let (t, _) = after(&[b"abc", b"\x7f\x7f\x7f"]);
        assert!(!t.has_unsent_text());
        let (t, _) = after(&[b"ab", b"\x08\x7f", b"\x7f"]);
        assert!(!t.has_unsent_text(), "extra backspaces are harmless");
        // Multi-byte characters count once.
        let (t, _) = after(&["çé".as_bytes(), b"\x7f\x7f"]);
        assert!(!t.has_unsent_text());
    }

    #[test]
    fn after_moving_the_cursor_backspace_cant_clear() {
        // "abc", cursor to the start, three backspaces delete nothing on screen.
        let (t, _) = after(&[b"abc", b"\x1b[D\x1b[D\x1b[D", b"\x7f\x7f\x7f"]);
        assert!(t.has_unsent_text());
        let (t, _) = after(&[b"abc", b"\x01", b"\x7f\x7f\x7f\x7f"]);
        assert!(t.has_unsent_text(), "Ctrl+A moves to the start");
        let (t, _) = after(&[b"abc", b"\x1bb", b"\x7f\x7f\x7f"]);
        assert!(t.has_unsent_text(), "Alt+B moves a word back");
    }

    #[test]
    fn history_may_fill_the_line() {
        let (t, _) = after(&[b"\x1b[A"]);
        assert!(t.has_unsent_text());
        let (t, _) = after(&[b"\x1bOA", b"\x7f"]);
        assert!(t.has_unsent_text(), "inexact after history");
        let (t, _) = after(&[b"\x1b[B", b"\r"]);
        assert!(!t.has_unsent_text());
    }

    #[test]
    fn unknown_deletions_keep_the_flag() {
        for keys in [
            &b"\x15"[..],
            b"\x0b",
            b"\x17",
            b"\x1b\x7f",
            b"\x03",
            b"\x1b",
        ] {
            let (t, _) = after(&[b"abc", keys]);
            assert!(t.has_unsent_text(), "{keys:?}");
        }
    }

    #[test]
    fn line_breaks_inside_the_input_are_text() {
        let (t, enters) = after(&[b"\n"]);
        assert!(t.has_unsent_text());
        assert_eq!(enters, 0);
        let (t, enters) = after(&[b"\x1b\r"]);
        assert!(t.has_unsent_text());
        assert_eq!(enters, 0);
    }

    #[test]
    fn controls_without_text_leave_an_empty_line_empty() {
        let (t, _) = after(&[b"\t\x03\x1b\x1b[C\x15"]);
        assert!(!t.has_unsent_text());
    }

    #[test]
    fn bracketed_paste_is_text_even_with_returns() {
        let (t, enters) = after(&[b"\x1b[200~line 1\rline 2\x1b[201~"]);
        assert!(t.has_unsent_text());
        assert_eq!(enters, 0);
        // Split across writes.
        let (t, enters) = after(&[b"\x1b[2", b"00~x", b"\x1b[20", b"1~", b"\r"]);
        assert!(!t.has_unsent_text());
        assert_eq!(enters, 1);
    }

    #[test]
    fn sequences_split_across_writes() {
        let (t, _) = after(&[b"abc", b"\x1b[", b"D", b"\x7f\x7f\x7f"]);
        assert!(
            t.has_unsent_text(),
            "the split arrow still made the count inexact"
        );
    }

    #[test]
    fn line_breaks_make_a_multi_line_draft() {
        let (t, _) = after(&[b"one line"]);
        assert!(!t.has_multi_line_text());
        for keys in [
            &b"first\nsecond"[..],
            b"first\x1b\rsecond",
            b"\x1b[200~line 1\rline 2\x1b[201~",
            b"\x1b[A",
        ] {
            let (t, _) = after(&[keys]);
            assert!(t.has_multi_line_text(), "{keys:?}");
        }
        let (t, _) = after(&[b"first\nsecond", b"\r"]);
        assert!(!t.has_multi_line_text(), "sent");
    }

    #[test]
    fn otaq_clear_line_resets() {
        let (mut t, _) = after(&[b"abc\x1b[D"]);
        t.clear_line();
        assert!(!t.has_unsent_text());
    }
}
