//! The report view (CMT-12; d0109; *05 → Row · Ticket comments and a report* `s05-41` / `s05-42`, "Opening a
//! report"): a comment's report, or text pasted into a comment box, read-only over the sheet. "Report · 48 KB · 2,310
//! lines · read-only", who posted it and on which comment, Copy, Download, search (⌘F) with "2 of 6" and next /
//! previous, line numbers, the match highlighted.
//!
//! A report may be 20 MB, so the page never holds it whole: [`OpenReport`] indexes where each line starts by
//! streaming the file once, then hands out pages of lines ([`OpenReport::page`]) by seeking to the first one, and
//! searches by streaming it again line by line ([`OpenReport::search`]), keeping only where the matches are. Copy and
//! Download work on the file itself on the Rust side (the desktop's `report.*` requests).

use std::fs::File;
use std::io::{self, BufRead, BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;

use otaq_core::board::reports::{count_label, size_label};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::ticket_comments::{CommentRow, Mark};

/// How many lines the page asks for at once (and at most gets).
pub const PAGE_LINES: u32 = 200;
/// A longer line is cut, with "…", in the view (Copy and Download keep it whole).
pub const LINE_CHARS: usize = 4_000;
/// Matches past this many are counted as "10,000+" and not reachable with next / previous.
pub const MATCHES: usize = 10_000;

pub const TITLE: &str = "Report";
pub const PASTED_TITLE: &str = "Pasted text";
pub const READ_ONLY: &str = "read-only";

/// Where a report's text is: its file in Otaq's data folder, or the text pasted into a box, not posted yet.
#[derive(Debug, Clone)]
pub enum Body {
    File(PathBuf),
    Text(Arc<[u8]>),
}

trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

impl Body {
    fn reader(&self) -> io::Result<Box<dyn ReadSeek>> {
        Ok(match self {
            Self::File(path) => Box::new(File::open(path)?),
            Self::Text(text) => Box::new(Cursor::new(Arc::clone(text))),
        })
    }
}

/// A search's matches: each as its line (0-based) and which match on that line it is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Search {
    query: String,
    hits: Vec<(u64, u32)>,
    /// More than [`MATCHES`].
    more: bool,
}

/// An open report: where its lines start, and the last search.
#[derive(Debug)]
pub struct OpenReport {
    body: Body,
    bytes: u64,
    /// The byte offset of each line's start, in order; its length is the line count.
    starts: Vec<u64>,
    search: Option<Search>,
    /// The current match's place in the search's hits.
    current: usize,
}

/// Where each line of `reader` starts, read in blocks (never the whole text at once), and its size in bytes. A last
/// line without a newline counts; an empty text has no lines.
fn index(reader: impl Read) -> io::Result<(Vec<u64>, u64)> {
    let mut reader = BufReader::with_capacity(64 * 1024, reader);
    let mut starts = Vec::new();
    let mut at = 0u64;
    let mut line_open = false;
    loop {
        let block = reader.fill_buf()?;
        if block.is_empty() {
            break;
        }
        for (i, b) in block.iter().enumerate() {
            if !line_open {
                starts.push(at + i as u64);
                line_open = true;
            }
            if *b == b'\n' {
                line_open = false;
            }
        }
        let n = block.len();
        at += n as u64;
        reader.consume(n);
    }
    Ok((starts, at))
}

/// A line's bytes as text: without its line ending, invalid UTF-8 replaced.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Where `query` occurs in `line`, ignoring ASCII case: each match's byte range, left to right, never overlapping.
fn find_all(line: &str, query: &str) -> Vec<(usize, usize)> {
    let (hay, needle) = (line.as_bytes(), query.as_bytes());
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > hay.len() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            out.push((i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// A piece of a line: plain text, or a match (the current one stands out).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportSegmentView")]
pub struct Segment {
    pub text: String,
    pub hit: bool,
    pub current: bool,
}

/// One line of the view: its number and its text in pieces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportLine")]
pub struct Line {
    /// 1-based, as the page shows it ("1180").
    pub number: u64,
    pub segments: Vec<Segment>,
    /// The line of the current match: drawn highlighted.
    pub current: bool,
}

/// A page of lines: `start` is the first one's 0-based index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportPage")]
pub struct Page {
    pub start: u64,
    pub lines: Vec<Line>,
}

/// Where a search stands: "2 of 6", and the current match's line to scroll to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportSearch")]
pub struct Found {
    /// "2 of 6", "No matches", or empty with no query.
    pub label: String,
    /// The matches next / previous reach.
    pub count: u32,
    /// The current match, 0-based.
    pub current: Option<u32>,
    /// Its line, 0-based.
    pub line: Option<u64>,
}

/// "48,210 lines", "1 line".
fn lines_label(n: u64) -> String {
    match n {
        1 => "1 line".to_owned(),
        n => format!("{} lines", count_label(n)),
    }
}

/// `text` cut to [`LINE_CHARS`] characters, with "…".
fn cut(text: &str) -> (&str, bool) {
    match text.char_indices().nth(LINE_CHARS) {
        Some((at, _)) => (&text[..at], true),
        None => (text, false),
    }
}

impl OpenReport {
    /// Indexes the report's lines (one streaming read).
    pub fn open(body: Body) -> io::Result<Self> {
        let (starts, bytes) = index(body.reader()?)?;
        Ok(Self {
            body,
            bytes,
            starts,
            search: None,
            current: 0,
        })
    }

    pub fn lines(&self) -> u64 {
        self.starts.len() as u64
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn body(&self) -> &Body {
        &self.body
    }

    /// "· 48 KB · 2,310 lines · read-only".
    pub fn meta(&self) -> String {
        format!(
            "\u{b7} {} \u{b7} {} \u{b7} {READ_ONLY}",
            size_label(self.bytes),
            lines_label(self.lines())
        )
    }

    /// The raw lines `start..start + count` (cut to the report's end), read from where the first one starts.
    fn raw_lines(&self, start: u64, count: u64) -> io::Result<Vec<String>> {
        let total = self.lines();
        let start = start.min(total);
        let end = start.saturating_add(count).min(total);
        if start == end {
            return Ok(Vec::new());
        }
        let from = self.starts[start as usize];
        let to = self.starts.get(end as usize).copied().unwrap_or(self.bytes);
        let mut reader = self.body.reader()?;
        reader.seek(SeekFrom::Start(from))?;
        let mut reader = BufReader::new(reader.take(to - from));
        let mut out = Vec::with_capacity((end - start) as usize);
        let mut buf = Vec::new();
        while out.len() < (end - start) as usize {
            buf.clear();
            if reader.read_until(b'\n', &mut buf)? == 0 {
                break;
            }
            out.push(decode(&buf));
        }
        Ok(out)
    }

    /// Lines `start..start + count` (at most [`PAGE_LINES`]), with the last search's matches marked.
    pub fn page(&self, start: u64, count: u32) -> io::Result<Page> {
        let count = count.min(PAGE_LINES);
        let current = self.current_hit();
        let lines = self
            .raw_lines(start, u64::from(count))?
            .into_iter()
            .enumerate()
            .map(|(i, text)| {
                let index = start + i as u64;
                let current_nth = current.filter(|(line, _)| *line == index).map(|(_, n)| n);
                self.line(index, &text, current_nth)
            })
            .collect();
        Ok(Page {
            start: start.min(self.lines()),
            lines,
        })
    }

    fn line(&self, index: u64, text: &str, current: Option<u32>) -> Line {
        let (shown, was_cut) = cut(text);
        let hits = self
            .search
            .as_ref()
            .map(|s| find_all(shown, &s.query))
            .unwrap_or_default();
        let mut segments = Vec::new();
        let mut at = 0;
        for (n, (from, to)) in hits.into_iter().enumerate() {
            if from > at {
                segments.push(Segment {
                    text: shown[at..from].to_owned(),
                    hit: false,
                    current: false,
                });
            }
            segments.push(Segment {
                text: shown[from..to].to_owned(),
                hit: true,
                current: current == u32::try_from(n).ok(),
            });
            at = to;
        }
        let mut rest = shown[at..].to_owned();
        if was_cut {
            rest.push('\u{2026}');
        }
        if !rest.is_empty() || segments.is_empty() {
            segments.push(Segment {
                text: rest,
                hit: false,
                current: false,
            });
        }
        Line {
            number: index + 1,
            segments,
            current: current.is_some(),
        }
    }

    /// The current match: a new search's first, until [`Self::step`] moves it.
    fn current_hit(&self) -> Option<(u64, u32)> {
        let s = self.search.as_ref()?;
        s.hits.get(self.current).copied()
    }

    /// Searches the whole report for `query` (ASCII case ignored), streaming it line by line; the first match
    /// becomes the current one. An empty query clears the search.
    pub fn search(&mut self, query: &str) -> io::Result<Found> {
        if query.is_empty() {
            self.search = None;
            self.current = 0;
            return Ok(self.found());
        }
        if self.search.as_ref().is_some_and(|s| s.query == query) {
            return Ok(self.found());
        }
        let mut reader = BufReader::with_capacity(64 * 1024, self.body.reader()?);
        let mut hits = Vec::new();
        let mut more = false;
        let mut buf = Vec::new();
        let mut index = 0u64;
        'lines: loop {
            buf.clear();
            if reader.read_until(b'\n', &mut buf)? == 0 {
                break;
            }
            let text = decode(&buf);
            let (shown, _) = cut(&text);
            for n in 0..find_all(shown, query).len() {
                if hits.len() == MATCHES {
                    more = true;
                    break 'lines;
                }
                hits.push((index, u32::try_from(n).unwrap_or(u32::MAX)));
            }
            index += 1;
        }
        self.search = Some(Search {
            query: query.to_owned(),
            hits,
            more,
        });
        self.current = 0;
        Ok(self.found())
    }

    /// Moves to the next (`forward`) or previous match, round the ends.
    pub fn step(&mut self, forward: bool) -> Found {
        let count = self.search.as_ref().map_or(0, |s| s.hits.len());
        if count > 0 {
            self.current = match forward {
                true => (self.current + 1) % count,
                false => (self.current + count - 1) % count,
            };
        }
        self.found()
    }

    /// Where the search stands.
    pub fn found(&self) -> Found {
        let Some(s) = &self.search else {
            return Found {
                label: String::new(),
                count: 0,
                current: None,
                line: None,
            };
        };
        let count = s.hits.len();
        let total = match s.more {
            true => format!("{}+", count_label(count as u64)),
            false => count_label(count as u64),
        };
        Found {
            label: match count {
                0 => "No matches".to_owned(),
                _ => format!("{} of {total}", count_label(self.current as u64 + 1)),
            },
            count: u32::try_from(count).unwrap_or(u32::MAX),
            current: (count > 0).then(|| u32::try_from(self.current).unwrap_or(u32::MAX)),
            line: s.hits.get(self.current).map(|(line, _)| *line),
        }
    }
}

/// Who posted the report: "Validator · Validate · round 2 · 3:05 pm" "· on SHOP-34's comment", with their mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportPoster")]
pub struct Poster {
    pub mark: Mark,
    pub heading: String,
    pub on: String,
}

impl Poster {
    pub fn of(row: &CommentRow, key: &str) -> Self {
        Self {
            mark: row.mark.clone(),
            heading: row.heading.clone(),
            on: format!("\u{b7} on {key}\u{2019}s comment"),
        }
    }
}

/// The view's words and the report's size; its lines come by [`Page`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReportView")]
pub struct View {
    /// What the page sends back for pages, searches, Copy and Download.
    pub handle: String,
    /// "Report", or "Pasted text" for a paste not posted yet.
    pub title: String,
    /// "· 48 KB · 2,310 lines · read-only".
    pub meta: String,
    /// Who posted it; none for a paste, or a deleted comment's report.
    pub poster: Option<Poster>,
    pub lines: u64,
    pub page_lines: u32,
    pub copy: String,
    pub copied: String,
    pub download: String,
    pub saved: String,
    /// The save dialog's title and the name it offers: "SHOP-34-comment-4.txt".
    pub save_title: String,
    pub file_name: String,
    pub close: String,
    pub search: String,
    pub next: String,
    pub previous: String,
}

/// What the view opens on.
pub enum Of<'a> {
    /// Comment `n` of `key`, and its row in the thread (none once deleted).
    Comment {
        key: &'a str,
        n: u32,
        row: Option<&'a CommentRow>,
    },
    /// Text pasted into a box on `key`.
    Pasted { key: &'a str },
}

pub fn view(handle: String, report: &OpenReport, of: &Of) -> View {
    let (title, poster, file_name) = match of {
        Of::Comment { key, n, row } => (
            TITLE,
            row.map(|r| Poster::of(r, key)),
            format!("{key}-comment-{n}.txt"),
        ),
        Of::Pasted { key } => (PASTED_TITLE, None, format!("{key}-pasted.txt")),
    };
    View {
        handle,
        title: title.to_owned(),
        meta: report.meta(),
        poster,
        lines: report.lines(),
        page_lines: PAGE_LINES,
        copy: "Copy".to_owned(),
        copied: "Copied".to_owned(),
        download: "Download".to_owned(),
        saved: "Saved".to_owned(),
        save_title: "Save the report".to_owned(),
        file_name,
        close: "Close".to_owned(),
        search: "Search".to_owned(),
        next: "Next match".to_owned(),
        previous: "Previous match".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> OpenReport {
        OpenReport::open(Body::Text(Arc::from(s.as_bytes()))).unwrap()
    }

    fn plain(line: &Line) -> String {
        line.segments.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn lines_are_indexed_like_the_service_counts_them() {
        assert_eq!(text("").lines(), 0);
        assert_eq!(text("one").lines(), 1);
        assert_eq!(text("one\n").lines(), 1);
        assert_eq!(text("one\ntwo").lines(), 2);
        assert_eq!(text("one\r\n\ntwo\n").lines(), 3);
        let r = text("one\r\n\ntwo\n");
        let page = r.page(0, 10).unwrap();
        assert_eq!(
            page.lines.iter().map(plain).collect::<Vec<_>>(),
            ["one", "", "two"]
        );
        assert_eq!(
            page.lines.iter().map(|l| l.number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    /// *05 → Opening a report*: "Report · 48 KB · 2,310 lines · read-only", the poster, the buttons.
    #[test]
    fn the_head_reads_like_the_frame() {
        // 2,310 lines making 48 KB.
        let body: String = (0..2_310)
            .map(|i| format!("{}\n", "x".repeat(if i < 200 { 21 } else { 20 })))
            .collect();
        let r = text(&body);
        assert_eq!(r.lines(), 2_310);
        assert_eq!(r.meta(), "\u{b7} 48 KB \u{b7} 2,310 lines \u{b7} read-only");
        let row = CommentRow {
            n: 4,
            author: "Validator".into(),
            mine: false,
            mark: Mark::Agent,
            meta: "Validate \u{b7} round 2 \u{b7} 3:05 pm".into(),
            editing_meta: String::new(),
            edited: None,
            heading: "Validator \u{b7} Validate \u{b7} round 2 \u{b7} 3:05 pm".into(),
            body: "Fail".into(),
            report: None,
        };
        let v = view(
            "h1".into(),
            &r,
            &Of::Comment {
                key: "SHOP-34",
                n: 4,
                row: Some(&row),
            },
        );
        assert_eq!(v.title, "Report");
        let poster = v.poster.unwrap();
        assert_eq!(
            poster.heading,
            "Validator \u{b7} Validate \u{b7} round 2 \u{b7} 3:05 pm"
        );
        assert_eq!(poster.on, "\u{b7} on SHOP-34\u{2019}s comment");
        assert_eq!((v.copy.as_str(), v.download.as_str()), ("Copy", "Download"));
        assert_eq!(v.file_name, "SHOP-34-comment-4.txt");
        assert_eq!(v.lines, 2_310);
        let pasted = view("h2".into(), &text("x\n"), &Of::Pasted { key: "SHOP-34" });
        assert_eq!(
            (pasted.title.as_str(), pasted.meta.as_str(), pasted.poster),
            (
                "Pasted text",
                "\u{b7} 2 bytes \u{b7} 1 line \u{b7} read-only",
                None
            )
        );
    }

    fn numbered(n: u64) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }

    #[test]
    fn pages_start_where_asked_and_stop_at_the_end() {
        let r = text(&numbered(1_000));
        let page = r.page(500, 200).unwrap();
        assert_eq!(page.start, 500);
        assert_eq!(page.lines.len(), 200);
        assert_eq!(plain(&page.lines[0]), "line 501");
        assert_eq!(page.lines[199].number, 700);
        // Never more than a page, never past the end.
        assert_eq!(r.page(0, 5_000).unwrap().lines.len(), PAGE_LINES as usize);
        let last = r.page(950, 200).unwrap();
        assert_eq!(last.lines.len(), 50);
        assert_eq!(plain(&last.lines[49]), "line 1000");
        assert!(r.page(1_000, 10).unwrap().lines.is_empty());
        assert!(r.page(u64::MAX, 10).unwrap().lines.is_empty());
    }

    /// *05 → Opening a report*: "10000" found "2 of 6", next / previous round the ends, the match marked on its line.
    #[test]
    fn a_search_across_pages_counts_steps_and_marks_the_match() {
        let mut body = numbered(1_000);
        // Six matches spread over pages 1, 3 and 5 (two on one line).
        for (line, text) in [
            (10, "rows: 10000"),
            (450, "expected at most 10000 rows"),
            (451, "left: 12500 right: 10000"),
            (900, "10000 and 10000 again"),
            (999, "limit 10000"),
        ] {
            let mut lines: Vec<String> = body.lines().map(str::to_owned).collect();
            lines[line] = text.to_owned();
            body = lines.join("\n");
        }
        let mut r = text(&body);
        let found = r.search("10000").unwrap();
        assert_eq!(found.label, "1 of 6");
        assert_eq!(
            (found.count, found.current, found.line),
            (6, Some(0), Some(10))
        );
        let found = r.step(true);
        assert_eq!((found.label.as_str(), found.line), ("2 of 6", Some(450)));
        // The page holding the current match marks it; the line is highlighted.
        let page = r.page(400, 200).unwrap();
        let line = &page.lines[50];
        assert_eq!(line.number, 451);
        assert!(line.current);
        assert_eq!(
            line.segments,
            [
                Segment {
                    text: "expected at most ".into(),
                    hit: false,
                    current: false
                },
                Segment {
                    text: "10000".into(),
                    hit: true,
                    current: true
                },
                Segment {
                    text: " rows".into(),
                    hit: false,
                    current: false
                },
            ]
        );
        // The next line has a match that isn't the current one.
        let next = &page.lines[51];
        assert!(!next.current);
        assert!(next.segments.iter().any(|s| s.hit && !s.current));
        // Two matches on one line: the 5th is that line's second.
        r.step(true);
        r.step(true);
        let found = r.step(true);
        assert_eq!((found.label.as_str(), found.line), ("5 of 6", Some(900)));
        let line = &r.page(900, 1).unwrap().lines[0];
        let hits: Vec<bool> = line
            .segments
            .iter()
            .filter(|s| s.hit)
            .map(|s| s.current)
            .collect();
        assert_eq!(hits, [false, true]);
        // Round the ends both ways.
        r.step(true);
        assert_eq!(r.step(true).label, "1 of 6");
        assert_eq!(r.step(false).label, "6 of 6");
        // Case is ignored for ASCII; no match, no query.
        assert_eq!(r.search("LINE 77").unwrap().label, "1 of 11");
        assert_eq!(r.search("nothing here").unwrap().label, "No matches");
        let cleared = r.search("").unwrap();
        assert_eq!((cleared.label.as_str(), cleared.count), ("", 0));
        assert!(
            r.page(0, 1).unwrap().lines[0]
                .segments
                .iter()
                .all(|s| !s.hit)
        );
    }

    #[test]
    fn too_many_matches_are_capped_and_long_lines_cut() {
        let mut r = text(&"ab\n".repeat(MATCHES + 5));
        let found = r.search("ab").unwrap();
        assert_eq!(found.label, "1 of 10,000+");
        assert_eq!(found.count, 10_000);
        let r = text(&"ab ".repeat(LINE_CHARS));
        let line = &r.page(0, 1).unwrap().lines[0];
        let shown: String = line.segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(shown.chars().count(), LINE_CHARS + 1);
        assert!(shown.ends_with('\u{2026}'));
    }

    #[test]
    fn invalid_utf8_still_reads() {
        let r = OpenReport::open(Body::Text(Arc::from(&b"ok \xff here\nnext"[..]))).unwrap();
        let page = r.page(0, 2).unwrap();
        assert_eq!(plain(&page.lines[0]), "ok \u{fffd} here");
        assert_eq!(plain(&page.lines[1]), "next");
    }

    /// CMT-12: a 20 MB report file opens, pages and searches without being read whole: the index holds one offset
    /// per line, a page reads only its own bytes, and a search streams.
    #[test]
    fn a_20_mb_report_pages_and_searches_from_the_file() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("report-view-20mb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.txt");
        let limit = otaq_core::board::reports::REPORT_LIMIT_BYTES;
        {
            let mut f = std::io::BufWriter::new(File::create(&path).unwrap());
            let mut written = 0u64;
            let mut i = 0u64;
            while written < limit - 200 {
                i += 1;
                let line = if i.is_multiple_of(50_000) {
                    format!("test export::limit_rows_{i} ... FAILED (10000 rows)\n")
                } else {
                    format!("test export::case_{i:08} ... ok\n")
                };
                written += line.len() as u64;
                f.write_all(line.as_bytes()).unwrap();
            }
        }
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size > 20_000_000 && size <= limit, "{size}");
        let mut r = OpenReport::open(Body::File(path.clone())).unwrap();
        let lines = r.lines();
        assert!(lines > 600_000, "{lines}");
        assert_eq!(r.bytes(), size);
        // The last page, read by seeking: only its lines.
        let last = r.page(lines - 3, 200).unwrap();
        assert_eq!(last.lines.len(), 3);
        assert_eq!(last.lines[2].number, lines);
        // A search across the whole file, then a jump to a far match and its page.
        let found = r.search("FAILED").unwrap();
        let count = lines / 50_000;
        assert_eq!(found.label, format!("1 of {count}"));
        assert_eq!(found.line, Some(49_999));
        r.step(false);
        let found = r.found();
        assert_eq!(found.label, format!("{count} of {count}"));
        let line = found.line.unwrap();
        let page = r.page(line, 1).unwrap();
        assert!(page.lines[0].current);
        assert!(plain(&page.lines[0]).contains("FAILED (10000 rows)"));
        // The index is one offset per line: well under the file's size in memory.
        assert!(
            r.starts.len() * std::mem::size_of::<u64>() < size as usize,
            "the index is smaller than the file"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
