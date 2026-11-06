//! Comment attachments (CMT-03, d0109; tuq 001, 002, 005, 006): the files attached to one ticket comment, any number
//! and of any type, kept in Otaq's data folder ([`DataDir::attachment_path`]), never in the repository (d0023). Each
//! keeps its own name (`reports/<repo>/<KEY>/<comment id>/1/test-run-login.md`), which the thread, the Attachments
//! column and `prime` show (RPT-02); pasted text is stored as `…/<n>/pasted-text.txt` and shows as "Pasted text".
//! Every attachment has a kind ([`AttachmentKind`]): an image, text (which has a line count and opens in the report
//! view) or any other file.
//!
//! An attachment comes from a file an agent's tool wrote (`cargo test > "$OTAQ_RESOURCE_DIR/test-run.txt"`, then
//! `--report-file`, repeated for several) or from text or a file the user added in the app. Either way it is
//! **copied** (the original stays where it is), **streamed** (never read into memory whole) and sized, line-counted
//! and checked for text on the way. A file over [`REPORT_LIMIT_BYTES`] is refused by name and nothing is posted or
//! left behind. Attachments outlive their comment's delete and go with their ticket (d0060).

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::domain::TicketKey;
use crate::store::new_id;

/// The largest attachment Otaq keeps: 20 MB (20 × 1024 × 1024 bytes) per file. Exactly this size is accepted
/// (CMT-03, tuq 005); a comment may have any number.
pub const REPORT_LIMIT_BYTES: u64 = 20 * 1024 * 1024;

/// What an attachment without a name of its own is called: text the user pasted.
pub const PASTED_TEXT: &str = "Pasted text";

/// Where a new attachment comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportSource {
    /// A file to copy (`--report-file PATH`), as an absolute path; the CLI makes a relative one absolute.
    Path(PathBuf),
    /// Text the user pasted into the comment box (the app's chip, CMT-11): a "Pasted text" attachment.
    Text(String),
}

/// What an attachment is, from its type (tuq 002): an **image** (by its file's extension, [`IMAGE_EXTENSIONS`]),
/// **text** (pasted text, or a file whose bytes are UTF-8 with no NUL byte) or any **other** file. Text opens in the
/// report view and has a line count; images and other files are opened from their path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
    Text,
    Other,
}

/// The extensions an attachment is an image by (compared in lowercase).
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif", "ico", "tif", "tiff", "heic", "heif",
];

impl AttachmentKind {
    /// As stored: `image`, `text`, `other`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Text => "text",
            Self::Other => "other",
        }
    }

    /// From what [`Self::as_str`] stored; anything else is `other`.
    pub fn parse(stored: &str) -> Self {
        match stored {
            "image" => Self::Image,
            "text" => Self::Text,
            _ => Self::Other,
        }
    }

    /// Whether a file called `name` is an image: its extension is one of [`IMAGE_EXTENSIONS`], in any case.
    pub fn is_image_name(name: &str) -> bool {
        Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    }

    /// The kind of a file called `name` whose bytes are (`text`) or aren't UTF-8 text.
    pub fn of(name: &str, text: bool) -> Self {
        if Self::is_image_name(name) {
            Self::Image
        } else if text {
            Self::Text
        } else {
            Self::Other
        }
    }
}

/// One attachment of a ticket comment (tuq 001, 002) as the thread, `prime`, the CLI and History show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Its number in its comment, 1, 2, 3… in the order it was attached: `otaq report KEY N --attachment A`.
    pub n: u32,
    /// Its file's name (`test-run-login.md`, RPT-02), or "Pasted text" for pasted text.
    pub name: String,
    pub kind: AttachmentKind,
    /// Its size in bytes.
    pub bytes: u64,
    /// Its line count: text only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u64>,
    /// Its file's full path in the data folder: where an agent opens an image or any other file.
    pub path: String,
}

impl Attachment {
    /// "48 KB, 2,310 lines" for text, "120 KB" for the rest.
    pub fn size_and_lines(&self) -> String {
        let size = size_label(self.bytes);
        match self.lines {
            Some(lines) if self.kind == AttachmentKind::Text => format!(
                "{size}, {} line{}",
                count_label(lines),
                if lines == 1 { "" } else { "s" }
            ),
            _ => size,
        }
    }
}

/// "with test-run.txt (48 KB)", "with 3 attachments (2.1 MB)": what a comment carries, for the CLI's answer and
/// History's line; `None` without attachments.
pub fn with_attachments<'a>(
    attachments: impl IntoIterator<Item = (&'a str, u64)>,
) -> Option<String> {
    let all: Vec<(&str, u64)> = attachments.into_iter().collect();
    let total: u64 = all.iter().map(|(_, bytes)| bytes).sum();
    match all.as_slice() {
        [] => None,
        // "with pasted text (67 KB)": the name reads as words in the line.
        [(name, _)] if *name == PASTED_TEXT => {
            Some(format!("with pasted text ({})", size_label(total)))
        }
        [(name, _)] => Some(format!("with {name} ({})", size_label(total))),
        many => Some(format!(
            "with {} attachments ({})",
            many.len(),
            size_label(total)
        )),
    }
}

/// An attachment copied into the data folder for a comment that is about to be posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAttachment {
    pub n: u32,
    pub name: String,
    pub kind: AttachmentKind,
    pub bytes: u64,
    /// Text only.
    pub lines: Option<u64>,
    /// The file, relative to the data folder (`reports/shop-web/SHOP-34/<id>/1/test-run.txt`), as the comment stores
    /// it.
    pub relative: String,
    /// The file's full path.
    pub path: PathBuf,
}

/// The attachments copied for a comment that is about to be posted ([`Board::prepare_attachments`]), all in the
/// comment's folder. The comment takes its id. If the comment isn't posted after all (its transaction failed),
/// [`Self::discard`] removes the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAttachments {
    /// The id the comment gets: its folder is named after it.
    pub comment_id: String,
    /// `reports/<repo>/<KEY>/<comment id>`, full path.
    pub folder: PathBuf,
    /// In order: attachment 1 first.
    pub files: Vec<StoredAttachment>,
}

impl StoredAttachments {
    /// Removes the copied files and their comment's folder: its comment wasn't posted.
    pub fn discard(&self) {
        remove_folder(&self.folder);
    }
}

fn remove_folder(folder: &Path) {
    match std::fs::remove_dir_all(folder) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(path = %folder.display(), error = %e, "can't remove an unposted comment's attachments");
        }
    }
}

/// The name a file attachment's source gives it (`test-run-login.md`); pasted text has none. A `\` (a folder
/// separator on Windows, a plain character elsewhere) becomes `_` so the name is one path part everywhere.
fn source_name(source: &ReportSource) -> Option<String> {
    match source {
        ReportSource::Path(path) => path
            .file_name()
            .map(|n| n.to_string_lossy().replace('\\', "_")),
        ReportSource::Text(_) => None,
    }
}

/// What an attachment from `source` is called: its file's name, "Pasted text" for pasted text.
pub fn display_name(source: &ReportSource) -> String {
    source_name(source).unwrap_or_else(|| PASTED_TEXT.to_owned())
}

/// The file pasted text is stored as, in its numbered folder.
const PASTED_FILE: &str = "pasted-text.txt";

/// A size as people read it: "312 bytes", "48 KB", "1.5 MB", "212 MB" (1 KB = 1024 bytes). The thread's chip, `prime`'s
/// "Report: 48 KB" line and the refusals all use it.
pub fn size_label(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    let scaled = |unit: u64, name: &str| {
        // Under 10 of a unit, one decimal ("1.5 MB"); a whole number otherwise ("212 MB").
        let tenths = (bytes * 10 + unit / 2) / unit;
        if tenths < 100 && !tenths.is_multiple_of(10) {
            format!("{}.{} {name}", tenths / 10, tenths % 10)
        } else {
            format!("{} {name}", (bytes + unit / 2) / unit)
        }
    };
    match bytes {
        1 => "1 byte".to_owned(),
        b if b < KB => format!("{b} bytes"),
        b if b < MB - KB / 2 => format!("{} KB", ((b + KB / 2) / KB).max(1)),
        b if b < GB - MB / 2 => scaled(MB, "MB"),
        _ => scaled(GB, "GB"),
    }
}

/// A count with thousands separators: "2,000", "48,210".
pub fn count_label(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The refusal of an attachment over the limit (CMT-03, tuq 005), naming it: "perf.mov is 212 MB; the limit is 20 MB.
/// Keep the part that matters, for example the failing tests." `bytes` is `None` when the copy stopped at the limit
/// without knowing the whole size; a size that reads the same as the limit says "more than 20 MB".
pub fn too_big(name: &str, bytes: Option<u64>) -> String {
    let limit = size_label(REPORT_LIMIT_BYTES);
    let size = bytes
        .map(size_label)
        .filter(|s| *s != limit)
        .unwrap_or_else(|| format!("more than {limit}"));
    format!(
        "{name} is {size}; the limit is {limit}. Keep the part that matters, for example the failing tests."
    )
}

fn refused(message: String) -> BoardError {
    BoardError::refused(Refusal::BadReport, message)
}

/// Why the file at `path` can't be read: "Can't read the report /tmp/run.txt: it doesn't exist".
fn unreadable(path: &Path, e: &io::Error) -> BoardError {
    let why = match e.kind() {
        io::ErrorKind::NotFound => "it doesn't exist".to_owned(),
        io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        _ => e.to_string(),
    };
    refused(format!("Can't read the report {}: {why}", path.display()))
}

/// The name a refusal gives the file at `path`: its file name, else the whole path.
fn file_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Opens the file at `path` (absolute) for its copy, or the refusal: missing, unreadable, not a file, or over the
/// limit (naming it). The CLI calls it for every file before it posts anything (CMT-06).
pub fn open_report(path: &Path) -> Result<File, BoardError> {
    if !path.is_absolute() {
        return Err(refused(format!(
            "The report path must be absolute: {}",
            path.display()
        )));
    }
    let file = File::open(path).map_err(|e| unreadable(path, &e))?;
    let meta = file.metadata().map_err(|e| unreadable(path, &e))?;
    if !meta.is_file() {
        return Err(refused(format!(
            "Can't read the report {}: it isn't a file",
            path.display()
        )));
    }
    if meta.len() > REPORT_LIMIT_BYTES {
        return Err(refused(too_big(&file_label(path), Some(meta.len()))));
    }
    Ok(file)
}

/// Checks every source before anything is copied: each file readable and within the limit, pasted text within it.
pub fn check_sources(sources: &[ReportSource]) -> Result<(), BoardError> {
    for source in sources {
        match source {
            ReportSource::Path(path) => {
                open_report(path)?;
            }
            ReportSource::Text(text) => {
                if text.len() as u64 > REPORT_LIMIT_BYTES {
                    return Err(refused(too_big(PASTED_TEXT, Some(text.len() as u64))));
                }
            }
        }
    }
    Ok(())
}

/// What a copy found: the size, the line count, and whether the bytes are UTF-8 text (no NUL byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Copied {
    pub bytes: u64,
    pub lines: u64,
    pub text: bool,
}

/// Counts bytes and lines as they pass to `inner`, and checks they are text.
struct Counting<W> {
    inner: W,
    bytes: u64,
    newlines: u64,
    last: Option<u8>,
    /// A NUL byte or bytes that aren't UTF-8 were seen.
    binary: bool,
    /// The start of a UTF-8 character a write cut (at most 3 bytes).
    pending: Vec<u8>,
}

impl<W: Write> Counting<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            bytes: 0,
            newlines: 0,
            last: None,
            binary: false,
            pending: Vec::new(),
        }
    }

    fn check_text(&mut self, done: &[u8]) {
        if self.binary {
            return;
        }
        if done.contains(&0) {
            self.binary = true;
            return;
        }
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(done);
        if let Err(e) = std::str::from_utf8(&buf) {
            match e.error_len() {
                // Cut at the end: the rest of the character comes with the next write.
                None => self.pending = buf[e.valid_up_to()..].to_vec(),
                Some(_) => self.binary = true,
            }
        }
    }

    /// Lines: every newline, and a last line without one.
    fn copied(&self) -> Copied {
        Copied {
            bytes: self.bytes,
            lines: self.newlines + u64::from(self.last.is_some_and(|b| b != b'\n')),
            text: !self.binary && self.pending.is_empty(),
        }
    }
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        let done = &buf[..n];
        self.bytes += n as u64;
        self.newlines += done.iter().filter(|b| **b == b'\n').count() as u64;
        if let Some(&last) = done.last() {
            self.last = Some(last);
        }
        self.check_text(done);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Copies `source` to `dest` (its folder created), streaming, and returns what it found. Refused when the source
/// can't be read or is over the limit (by `name`); nothing is left at `dest` then (the copy goes to `<dest>.part` and
/// is renamed only once whole).
pub(crate) fn copy_report(
    source: &ReportSource,
    name: &str,
    dest: &Path,
) -> Result<Copied, BoardError> {
    // Everything is checked before anything is written.
    let mut input: Box<dyn Read> = match source {
        ReportSource::Path(path) => Box::new(open_report(path)?),
        ReportSource::Text(text) => {
            if text.len() as u64 > REPORT_LIMIT_BYTES {
                return Err(refused(too_big(name, Some(text.len() as u64))));
            }
            Box::new(io::Cursor::new(text.as_bytes()))
        }
    };
    let store_error =
        |e: io::Error| refused(format!("Can't store the report in {}: {e}", dest.display()));
    if let Some(folder) = dest.parent() {
        DataDir::ensure_dir(folder).map_err(store_error)?;
    }
    // `<name>.part`, next to it: never the name itself (a report called `run.part`).
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let copied = (|| {
        let mut out = Counting::new(File::create(&part).map_err(store_error)?);
        // One byte past the limit is enough to know it is over (a file that grew since it was checked).
        let read = io::copy(&mut (&mut input).take(REPORT_LIMIT_BYTES + 1), &mut out);
        if let Err(e) = read {
            return Err(match source {
                ReportSource::Path(path) => unreadable(path, &e),
                ReportSource::Text(_) => store_error(e),
            });
        }
        out.flush().map_err(store_error)?;
        let copied = out.copied();
        if copied.bytes > REPORT_LIMIT_BYTES {
            return Err(refused(too_big(name, None)));
        }
        std::fs::rename(&part, dest).map_err(store_error)?;
        Ok(copied)
    })();
    if copied.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    copied
}

/// Copies every source into `folder` (`<folder>/<n>/<name>`), in order. On any refusal the folder is removed:
/// nothing is left behind.
fn copy_all(
    sources: &[ReportSource],
    folder: &Path,
    root: &Path,
) -> Result<Vec<StoredAttachment>, BoardError> {
    let copied = (|| {
        let mut files = Vec::with_capacity(sources.len());
        for (i, source) in sources.iter().enumerate() {
            let n = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let name = display_name(source);
            let file_name = source_name(source).unwrap_or_else(|| PASTED_FILE.to_owned());
            let file_name = match file_name.as_str() {
                "" | "." | ".." => PASTED_FILE.to_owned(),
                _ => file_name,
            };
            let path = folder.join(n.to_string()).join(&file_name);
            let copied = copy_report(source, &name, &path)?;
            let kind = match source {
                ReportSource::Text(_) => AttachmentKind::Text,
                ReportSource::Path(_) => AttachmentKind::of(&name, copied.text),
            };
            files.push(StoredAttachment {
                n,
                name,
                kind,
                bytes: copied.bytes,
                lines: (kind == AttachmentKind::Text).then_some(copied.lines),
                relative: path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned(),
                path,
            });
        }
        Ok(files)
    })();
    if copied.is_err() {
        remove_folder(folder);
    }
    copied
}

impl Board {
    /// Copies a comment's attachments into the data folder for a comment on the ticket `key` of the repository
    /// `repo`, before the comment is posted: every source is checked first, so a missing, unreadable or oversized file
    /// is refused (by name) before anything is copied or posted (CMT-06, tuq 005). `None` without sources. The copy
    /// runs off the async threads.
    pub async fn prepare_attachments_in(
        &self,
        repo: &str,
        key: &TicketKey,
        sources: Vec<ReportSource>,
    ) -> Result<Option<StoredAttachments>, BoardError> {
        if sources.is_empty() {
            return Ok(None);
        }
        check_sources(&sources)?;
        let comment_id = new_id();
        let folder = self
            .data()
            .comment_attachments_dir(repo, key, &comment_id)
            .map_err(|e| refused(format!("Can't store the report: {e}")))?;
        let root = self.data().root().to_path_buf();
        let dest = folder.clone();
        let files = tokio::task::spawn_blocking(move || copy_all(&sources, &dest, &root))
            .await
            .map_err(|e| refused(format!("Can't store the report: {e}")))??;
        Ok(Some(StoredAttachments {
            comment_id,
            folder,
            files,
        }))
    }

    /// [`Self::prepare_attachments_in`] for a comment on the ticket `key`, its repository looked up.
    pub async fn prepare_attachments(
        &self,
        key: &TicketKey,
        sources: Vec<ReportSource>,
    ) -> Result<Option<StoredAttachments>, BoardError> {
        if sources.is_empty() {
            return Ok(None);
        }
        let project = self.project(key.project.as_str()).await?;
        let repo = crate::warm::store::repo_name(&project);
        self.prepare_attachments_in(&repo, key, sources).await
    }

    /// An attachment's full path from what its comment stores.
    pub fn attachment_file(&self, relative: &str) -> PathBuf {
        self.data().root().join(relative)
    }

    /// Removes the attachments of deleted tickets (d0060), and their folders once empty. Best effort: a file that
    /// can't be removed is logged.
    pub(crate) fn remove_reports(&self, relative: &[String]) {
        let stop = self.data().reports_dir();
        for file in relative {
            let path = self.attachment_file(file);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "can't remove a deleted ticket's attachment");
                }
            }
            // The attachment's folder, its comment's, the ticket's, then the repository's, when nothing else is in
            // them; never the reports folder itself.
            let mut folder = path.parent();
            while let Some(dir) = folder {
                if !dir.starts_with(&stop) || dir == stop || std::fs::remove_dir(dir).is_err() {
                    break;
                }
                folder = dir.parent();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_the_canvas() {
        assert_eq!(size_label(0), "0 bytes");
        assert_eq!(size_label(1), "1 byte");
        assert_eq!(size_label(312), "312 bytes");
        assert_eq!(size_label(1024), "1 KB");
        assert_eq!(size_label(48 * 1024 + 100), "48 KB");
        assert_eq!(size_label(1024 * 1024 - 1), "1 MB");
        assert_eq!(size_label(1536 * 1024), "1.5 MB");
        assert_eq!(size_label(2 * 1024 * 1024), "2 MB");
        assert_eq!(size_label(REPORT_LIMIT_BYTES), "20 MB");
        assert_eq!(size_label(212 * 1024 * 1024), "212 MB");
        assert_eq!(size_label(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
    }

    #[test]
    fn counts_have_thousands_separators() {
        assert_eq!(count_label(0), "0");
        assert_eq!(count_label(999), "999");
        assert_eq!(count_label(2_000), "2,000");
        assert_eq!(count_label(48_210), "48,210");
        assert_eq!(count_label(1_234_567), "1,234,567");
    }

    #[test]
    fn the_refusal_names_the_file_and_its_size() {
        assert_eq!(
            too_big("perf.mov", Some(212 * 1024 * 1024)),
            "perf.mov is 212 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests."
        );
        assert_eq!(
            too_big("run.txt", Some(REPORT_LIMIT_BYTES + 1)),
            "run.txt is more than 20 MB; the limit is 20 MB. Keep the part that matters, for example the failing \
             tests."
        );
    }

    #[test]
    fn a_copied_file_keeps_its_name_and_pasted_text_has_none() {
        let path = ReportSource::Path("/tmp/runs/test-run-login.md".into());
        assert_eq!(source_name(&path).as_deref(), Some("test-run-login.md"));
        assert_eq!(display_name(&path), "test-run-login.md");
        // On Windows `\` separates folders already.
        #[cfg(unix)]
        assert_eq!(
            source_name(&ReportSource::Path("/tmp/a\\b.txt".into())).as_deref(),
            Some("a_b.txt")
        );
        assert_eq!(source_name(&ReportSource::Text("x".into())), None);
        assert_eq!(display_name(&ReportSource::Text("x".into())), PASTED_TEXT);
    }

    #[test]
    fn the_kind_comes_from_the_name_and_the_bytes() {
        assert_eq!(AttachmentKind::of("shot.PNG", false), AttachmentKind::Image);
        assert_eq!(
            AttachmentKind::of("photo.jpeg", true),
            AttachmentKind::Image
        );
        assert_eq!(AttachmentKind::of("run.txt", true), AttachmentKind::Text);
        assert_eq!(AttachmentKind::of("Makefile", true), AttachmentKind::Text);
        assert_eq!(
            AttachmentKind::of("repro.zip", false),
            AttachmentKind::Other
        );
        for kind in [
            AttachmentKind::Image,
            AttachmentKind::Text,
            AttachmentKind::Other,
        ] {
            assert_eq!(AttachmentKind::parse(kind.as_str()), kind);
        }
    }

    #[test]
    fn a_report_named_like_its_part_file_is_copied_whole() {
        let dir = tempfile::TempDir::new().unwrap();
        let dest = dir.path().join("c1").join("run.part");
        let copied = copy_report(&ReportSource::Text("one\n".into()), "x", &dest).unwrap();
        assert_eq!(copied.bytes, 4);
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "one\n");
        assert!(!dir.path().join("c1").join("run.part.part").exists());
    }

    #[test]
    fn lines_count_a_last_line_without_a_newline() {
        let dir = tempfile::TempDir::new().unwrap();
        let dest = dir.path().join("r").join("a.txt");
        let text = |t: &str| copy_report(&ReportSource::Text(t.into()), "a.txt", &dest).unwrap();
        assert_eq!(
            text("one\ntwo\nthree"),
            Copied {
                bytes: 13,
                lines: 3,
                text: true
            }
        );
        assert_eq!(
            text("one\ntwo\n"),
            Copied {
                bytes: 8,
                lines: 2,
                text: true
            }
        );
        assert_eq!(
            text(""),
            Copied {
                bytes: 0,
                lines: 0,
                text: true
            }
        );
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "");
    }

    #[test]
    fn bytes_that_arent_utf8_or_hold_a_nul_arent_text() {
        let mut counting = Counting::new(Vec::new());
        // "ş" cut between two writes is still text.
        counting.write_all(&[b'a', 0xC5]).unwrap();
        counting.write_all(&[0x9F, b'\n']).unwrap();
        assert!(counting.copied().text);
        let mut nul = Counting::new(Vec::new());
        nul.write_all(b"PK\x03\x04\x00\x00").unwrap();
        assert!(!nul.copied().text);
        let mut bad = Counting::new(Vec::new());
        bad.write_all(&[0xFF, 0xFE, b'a']).unwrap();
        assert!(!bad.copied().text);
        let mut cut = Counting::new(Vec::new());
        cut.write_all(&[b'a', 0xC5]).unwrap();
        assert!(!cut.copied().text, "a character cut at the end isn't text");
    }

    #[test]
    fn several_sources_go_in_numbered_folders_and_a_refusal_leaves_nothing() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("b")).unwrap();
        std::fs::write(src.join("run.txt"), "one\ntwo\n").unwrap();
        std::fs::write(src.join("b").join("run.txt"), "x").unwrap();
        std::fs::write(src.join("shot.png"), [0x89, b'P', b'N', b'G', 0, 0]).unwrap();
        let folder = dir.path().join("reports").join("c1");
        let files = copy_all(
            &[
                ReportSource::Path(src.join("run.txt")),
                ReportSource::Path(src.join("b").join("run.txt")),
                ReportSource::Path(src.join("shot.png")),
                ReportSource::Text("pasted".into()),
            ],
            &folder,
            dir.path(),
        )
        .unwrap();
        let seen: Vec<(u32, &str, AttachmentKind, Option<u64>, &str)> = files
            .iter()
            .map(|f| (f.n, f.name.as_str(), f.kind, f.lines, f.relative.as_str()))
            .collect();
        let rel = |n: &str, f: &str| {
            Path::new("reports")
                .join("c1")
                .join(n)
                .join(f)
                .to_string_lossy()
                .into_owned()
        };
        let (r1, r2, r3, r4) = (
            rel("1", "run.txt"),
            rel("2", "run.txt"),
            rel("3", "shot.png"),
            rel("4", "pasted-text.txt"),
        );
        assert_eq!(
            seen,
            [
                (1, "run.txt", AttachmentKind::Text, Some(2), r1.as_str()),
                (2, "run.txt", AttachmentKind::Text, Some(1), r2.as_str()),
                (3, "shot.png", AttachmentKind::Image, None, r3.as_str()),
                (4, PASTED_TEXT, AttachmentKind::Text, Some(1), r4.as_str()),
            ]
        );
        // One missing file: nothing stays.
        let refused = copy_all(
            &[
                ReportSource::Path(src.join("run.txt")),
                ReportSource::Path(src.join("nope.txt")),
            ],
            &dir.path().join("reports").join("c2"),
            dir.path(),
        );
        assert!(refused.is_err());
        assert!(!dir.path().join("reports").join("c2").exists());
    }

    #[test]
    fn with_attachments_names_one_and_counts_several() {
        assert_eq!(with_attachments([]), None);
        assert_eq!(
            with_attachments([("run.txt", 48 * 1024)]).as_deref(),
            Some("with run.txt (48 KB)")
        );
        assert_eq!(
            with_attachments([
                ("a.png", 1024 * 1024),
                ("b.txt", 1024 * 1024),
                ("c.zip", 102 * 1024)
            ])
            .as_deref(),
            Some("with 3 attachments (2.1 MB)")
        );
    }
}
