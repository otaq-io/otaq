//! The comment box's attachments (tuq 001, 002, 003, 005): what every comment box sends (the thread's
//! `ticket_comments.post`, Send back, Send back with PR comments and the drag-back dialog), one list, each a file by
//! its path or text pasted over the limit ([`Sent`]).
//!
//! A file the user picked (Attach, Overview's Add) or dropped has a path, sent as it is: the service copies it when
//! the comment is posted. A screenshot on the clipboard or files pasted from Finder have none: the page hands their
//! bytes to the door's `stage_attachment` as a raw body (Tauri's binary IPC, never JSON or base64), which writes them
//! to `staged-attachments/<id>/<name>` in the data folder ([`stage`]) and answers the same chip as a picked file; from
//! there they are sent by path like any other. A posted comment's staged files go ([`release`]); the folder is
//! emptied when the app starts ([`clear`]). The page checks a file's size before reading it (`attachments.refusal`):
//! over 20 MB it is refused by name with core's words, and nothing is read or sent.

use std::path::{Path, PathBuf};

use otaq_core::board::reports::{REPORT_LIMIT_BYTES, ReportSource, too_big};
use otaq_core::data_dir::DataDir;
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::overview::FileChip;
use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(_app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "attachments.refusal" => reply(&refusal(&parse(params)?)),
        _ => Err(unknown(method)),
    }
}

/// The page's types (`generated.rs`' one line for them).
#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    Sent::export_all(cfg)?;
    SizeParams::export_all(cfg)?;
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(format!("bad parameters: {e}")))
}

/// One attachment a comment box sends: `{"kind": "file", "path": "/Users/me/shot.png"}` (picked, dropped, or staged
/// from a paste) or `{"kind": "pasted", "text": "…48 KB of log…"}` (a paste over the limit, "Pasted text").
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "CommentAttachmentParams")]
pub enum Sent {
    File { path: String },
    Pasted { text: String },
}

/// What the service copies, in the box's order; empty pasted text is no attachment.
pub fn sources(sent: &[Sent]) -> Vec<ReportSource> {
    sent.iter()
        .filter_map(|a| match a {
            Sent::File { path } => Some(ReportSource::Path(PathBuf::from(path))),
            Sent::Pasted { text } if text.trim().is_empty() => None,
            Sent::Pasted { text } => Some(ReportSource::Text(text.clone())),
        })
        .collect()
}

/// `{"name": "perf.mov", "bytes": 22020096}`: a file about to be added to the box.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AttachmentSizeParams")]
pub struct SizeParams {
    pub name: String,
    pub bytes: u64,
}

/// Why a file of this size can't be added (tuq 005: "perf.mov is 21 MB; the limit is 20 MB. …"), or `None`.
pub fn refusal(params: &SizeParams) -> Option<String> {
    (params.bytes > REPORT_LIMIT_BYTES).then(|| too_big(&params.name, Some(params.bytes)))
}

/// The folder staged files wait in.
fn staged_dir(app: &App) -> Result<PathBuf, RequestError> {
    app.service
        .data()
        .map(|d| d.staged_attachments_dir())
        .ok_or_else(|| RequestError::plain("no data folder"))
}

/// A pasted file's name from the `x-name` header the page sends it in, `encodeURIComponent`'d (a header is ASCII):
/// `%E2%80%99` back as `’`; a broken escape stays as it is.
pub fn name_from_header(text: &str) -> String {
    let raw = text.as_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let hex = raw
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (raw[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A file name that stays one path part: its last part after any `/` or `\`; "Pasted file" when nothing is left.
fn file_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    match last {
        "" | "." | ".." => "Pasted file".to_owned(),
        n => n.to_owned(),
    }
}

/// Writes a pasted file's bytes to `<dir>/<id>/<name>`, refusing one over 20 MB by name: its chip, by the path the
/// comment sends.
pub fn stage_in(dir: &Path, name: &str, bytes: &[u8]) -> Result<FileChip, RequestError> {
    let name = file_name(name);
    let size = bytes.len() as u64;
    if let Some(why) = refusal(&SizeParams {
        name: name.clone(),
        bytes: size,
    }) {
        return Err(RequestError::plain(why));
    }
    let folder = dir.join(otaq_core::store::new_id());
    let path = folder.join(&name);
    DataDir::ensure_dir(&folder)
        .and_then(|()| std::fs::write(&path, bytes))
        .map_err(|e| RequestError::plain(format!("Can't keep {name}: {e}")))?;
    Ok(FileChip::new(path.display().to_string(), name, size))
}

/// [`stage_in`] the app's data folder.
pub async fn stage(app: &App, name: String, bytes: Vec<u8>) -> Result<FileChip, RequestError> {
    let dir = staged_dir(app)?;
    tokio::task::spawn_blocking(move || stage_in(&dir, &name, &bytes))
        .await
        .map_err(|e| RequestError::plain(e.to_string()))?
}

/// Removes the staged files among `sent`, now copied into their comment's folder; files elsewhere are the user's own.
pub fn release_in(dir: &Path, sent: &[Sent]) {
    for a in sent {
        let Sent::File { path } = a else { continue };
        let Some(folder) = Path::new(path).parent() else {
            continue;
        };
        if folder.parent() == Some(dir) {
            let _ = std::fs::remove_dir_all(folder);
        }
    }
}

/// [`release_in`] the app's data folder, after the comment carrying them was posted.
pub fn release(app: &App, sent: &[Sent]) {
    if let Ok(dir) = staged_dir(app) {
        release_in(&dir, sent);
    }
}

/// Empties the staged files' folder: no comment box outlives the app (the door, when it starts).
pub fn clear(data: &DataDir) {
    let dir = data.staged_attachments_dir();
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("can't empty the staged attachments in {}: {e}", dir.display()),
    }
}

/// An image the box shows as a thumbnail (tuq 002): its bytes, for a file named as an image and within 20 MB; the page
/// reads no other file this way.
pub fn image_bytes(path: &Path) -> Result<Vec<u8>, RequestError> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !path.is_absolute() || !otaq_core::board::reports::AttachmentKind::is_image_name(&name) {
        return Err(RequestError::plain(format!("{} isn't an image", path.display())));
    }
    let size = std::fs::metadata(path)
        .map_err(|e| RequestError::plain(e.to_string()))?
        .len();
    if let Some(why) = refusal(&SizeParams { name, bytes: size }) {
        return Err(RequestError::plain(why));
    }
    std::fs::read(path).map_err(|e| RequestError::plain(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pasted screenshot is kept under its own name in its own folder, sent by path like a picked file, and goes
    /// once its comment is posted; two of the same name never meet.
    #[test]
    fn a_pasted_file_is_staged_then_released() {
        let dir = tempfile::tempdir().unwrap();
        let one = stage_in(dir.path(), "image.png", b"\x89PNG one").unwrap();
        let two = stage_in(dir.path(), "image.png", b"\x89PNG two").unwrap();
        assert_ne!(one.path, two.path);
        assert_eq!((one.name.as_str(), one.size.as_str(), one.image), ("image.png", "8 bytes", true));
        assert_eq!(std::fs::read(&one.path).unwrap(), b"\x89PNG one");
        assert_eq!(stage_in(dir.path(), "../x/notes.txt", b"hi").unwrap().name, "notes.txt");
        assert_eq!(stage_in(dir.path(), "", b"hi").unwrap().name, "Pasted file");

        let mine = dir.path().join("mine.txt");
        std::fs::write(&mine, "keep").unwrap();
        let sent = vec![
            Sent::File { path: one.path.clone() },
            Sent::File { path: mine.display().to_string() },
            Sent::Pasted { text: "log".into() },
        ];
        assert_eq!(
            sources(&sent),
            vec![
                ReportSource::Path(one.path.clone().into()),
                ReportSource::Path(mine.clone()),
                ReportSource::Text("log".into()),
            ]
        );
        release_in(dir.path(), &sent);
        assert!(!Path::new(&one.path).exists());
        assert!(Path::new(&two.path).exists(), "only what was sent goes");
        assert!(mine.exists(), "the user's own file stays");
    }

    /// Over 20 MB a file is refused by name before anything is kept (tuq 005); exactly 20 MB is fine.
    #[test]
    fn over_the_limit_is_refused_by_name() {
        let at = SizeParams {
            name: "perf.mov".into(),
            bytes: REPORT_LIMIT_BYTES,
        };
        assert_eq!(refusal(&at), None);
        let over = SizeParams {
            name: "perf.mov".into(),
            bytes: 21 * 1024 * 1024,
        };
        assert!(refusal(&over).unwrap().starts_with("perf.mov is 21 MB; the limit is 20 MB."));
        let dir = tempfile::tempdir().unwrap();
        let big = vec![0u8; (REPORT_LIMIT_BYTES + 1) as usize];
        let refused = stage_in(dir.path(), "shot.png", &big).unwrap_err();
        assert!(refused.message.starts_with("shot.png is "), "{}", refused.message);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// The name comes back from its header as the page had it.
    #[test]
    fn a_name_from_its_header() {
        assert_eq!(name_from_header("Screen%20Shot%E2%80%99s.png"), "Screen Shot\u{2019}s.png");
        assert_eq!(name_from_header("100%.txt"), "100%.txt");
        assert_eq!(name_from_header("a%zzb"), "a%zzb");
    }

    /// Only an image is read for its thumbnail.
    #[test]
    fn only_images_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        std::fs::write(&png, b"png").unwrap();
        assert_eq!(image_bytes(&png).unwrap(), b"png");
        let txt = dir.path().join("a.txt");
        std::fs::write(&txt, b"secret").unwrap();
        assert!(image_bytes(&txt).is_err());
        assert!(image_bytes(Path::new("a.png")).is_err());
    }
}
