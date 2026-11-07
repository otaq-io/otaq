//! One place that decides what the CLI prints (d0032): human text by default, `--json`, `-q/--quiet` (errors
//! only) or `--silent` (only the ID).
//!
//! Errors: human mode prints `error: <message>` on stderr. With `--json` the error is printed as
//! `{"error": {"kind": …, "message": …}}` on **stdout**, so a caller that parses stdout always gets JSON. Either
//! way the process exits non-zero.

use std::process::ExitCode;

use otaq_core::protocol::ErrorBody;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Human,
    Json,
    Quiet,
    Silent,
}

#[derive(Debug, Clone, Copy)]
pub struct Output {
    mode: Mode,
}

/// Exit code for an error reported by a command.
pub const EXIT_ERROR: u8 = 1;
/// Exit code for wrong usage (same as clap's).
pub const EXIT_USAGE: u8 = 2;

impl Output {
    pub fn new(mode: Mode) -> Self {
        Self { mode }
    }

    /// The default mode: human text (not `--json`, `--quiet` or `--silent`).
    pub fn is_human(&self) -> bool {
        self.mode == Mode::Human
    }

    /// What a successful command prints in each mode: `human` text, the `json` value, or the `id` alone.
    pub fn render<T: Serialize>(&self, human: &str, json: &T, id: Option<&str>) -> Option<String> {
        match self.mode {
            Mode::Human => Some(human.to_owned()).filter(|h| !h.is_empty()),
            Mode::Json => Some(serde_json::to_string_pretty(json).expect("output serializes")),
            Mode::Quiet => None,
            Mode::Silent => id.map(str::to_owned),
        }
    }

    pub fn print<T: Serialize>(&self, human: &str, json: &T, id: Option<&str>) {
        if let Some(text) = self.render(human, json, id) {
            print_line(&text);
        }
    }
}

/// Prints one line on stdout. A reader that went away (`otaq prime | head`, a closed pipe on Windows: os error 232) ends
/// the command quietly instead of panicking as `println!` does; any other write failure is reported on stderr.
fn print_line(text: &str) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = writeln!(stdout, "{text}").and_then(|()| stdout.flush()) {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("error: can't print the answer: {e}");
        std::process::exit(i32::from(EXIT_ERROR));
    }
}

impl Output {
    /// The error text for this mode: the JSON object, or `error: <message>`.
    pub fn render_error(&self, error: &ErrorBody) -> String {
        match self.mode {
            Mode::Json => json!({ "error": error }).to_string(),
            _ => format!("error: {}", error.message),
        }
    }

    /// Prints an error (also in quiet and silent mode) and returns the exit code to use.
    pub fn error(&self, error: &ErrorBody, code: u8) -> ExitCode {
        let text = self.render_error(error);
        match self.mode {
            Mode::Json => println!("{text}"),
            _ => eprintln!("{text}"),
        }
        ExitCode::from(code)
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::protocol::ErrorKind;

    use super::*;

    #[test]
    fn each_mode_prints_its_form() {
        let value = json!({"key": "SHOP-18"});
        let render = |mode| Output::new(mode).render("Created SHOP-18", &value, Some("SHOP-18"));
        assert_eq!(render(Mode::Human).as_deref(), Some("Created SHOP-18"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&render(Mode::Json).unwrap()).unwrap(),
            value
        );
        assert_eq!(render(Mode::Quiet), None);
        assert_eq!(render(Mode::Silent).as_deref(), Some("SHOP-18"));
        assert_eq!(Output::new(Mode::Silent).render("x", &value, None), None);
    }

    #[test]
    fn json_errors_have_kind_and_message() {
        let error = ErrorBody::new(
            ErrorKind::Disconnected,
            "the background service isn't running",
        );
        let text = Output::new(Mode::Json).render_error(&error);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            parsed,
            json!({"error": {"kind": "disconnected", "message": "the background service isn't running"}})
        );
        assert_eq!(
            Output::new(Mode::Human).render_error(&error),
            "error: the background service isn't running"
        );
    }
}
