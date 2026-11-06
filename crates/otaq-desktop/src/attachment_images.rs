//! A posted comment's images in the page (tuq 004, 006): the thread's thumbnails, the Attachments column's and
//! History's, and the full-size view, all load `otaq-attachment://localhost/<KEY>/<comment n>/<attachment n>`
//! (`http://otaq-attachment.localhost/…` on Windows), which [`crate::door`] answers with [`serve`].
//!
//! The page names an image only by its ticket, its comment's number and its own, never by a path: the service says
//! where that attachment is (`ticket.report`, which also finds a deleted comment's, for History), and only an
//! **image** whose file, with every link and `..` resolved, lies **inside the data folder's reports** is sent
//! ([`confine`]). Anything else is refused, so the page can't read the rest of the disk. The CSP's `img-src` allows
//! this one scheme; the reply carries the image's type, `nosniff` and, for SVG, a CSP of its own with no scripts (the
//! page draws images with `<img>` only, where scripts never run anyway).

use std::path::{Path, PathBuf};

use otaq_core::board::reports::AttachmentKind;
use otaq_core::domain::TicketKey;
use otaq_core::protocol::{Reply, Request, TicketReportParams};

use crate::App;

/// The scheme the page loads attachment images from.
pub const SCHEME: &str = otaq_core::naming::ATTACHMENT_SCHEME;

/// What [`serve`] answers: an HTTP status, its type and its bytes; the door makes it a response.
#[derive(Debug, PartialEq, Eq)]
pub struct Served {
    pub status: u16,
    pub mime: &'static str,
    pub body: Vec<u8>,
}

impl Served {
    fn refused(status: u16, why: &str) -> Self {
        Self {
            status,
            mime: "text/plain; charset=utf-8",
            body: why.as_bytes().to_vec(),
        }
    }
}

/// The headers every answer carries, besides its type: no sniffing it into something else, no caching (an
/// attachment can go with its ticket), and no scripts or loads from inside an image (SVG).
pub const HEADERS: &[(&str, &str)] = &[
    ("X-Content-Type-Options", "nosniff"),
    ("Cache-Control", "no-cache"),
    (
        "Content-Security-Policy",
        "default-src 'none'; style-src 'unsafe-inline'; sandbox",
    ),
];

/// `%2F` and the like back to their characters; `None` for a broken escape or text that isn't UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The ticket, comment number and attachment number of a request's path: `/SHOP-34/4/1`, or the page's encoded
/// `/SHOP-34%2F4%2F1`. Nothing else is accepted: no more parts, no path.
pub fn parse(path: &str) -> Option<(TicketKey, u32, u32)> {
    let path = percent_decode(path)?;
    let mut parts = path.trim_start_matches('/').split('/');
    let key = parts.next()?.parse::<TicketKey>().ok()?;
    let n = parts.next()?.parse::<u32>().ok()?;
    let attachment = parts.next()?.parse::<u32>().ok()?;
    match parts.next() {
        None => Some((key, n, attachment)),
        Some(_) => None,
    }
}

/// `file` as a real path (links and `..` resolved) when it is a file inside `reports`, else `None`.
pub fn confine(reports: &Path, file: &Path) -> Option<PathBuf> {
    let reports = reports.canonicalize().ok()?;
    let file = file.canonicalize().ok()?;
    (file.starts_with(&reports) && file.is_file()).then_some(file)
}

/// An image's type from its name; `None` for a name that isn't one of the image types Otaq knows
/// ([`otaq_core::board::reports::IMAGE_EXTENSIONS`]).
pub fn mime(name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "heif" => "image/heif",
        _ => return None,
    })
}

/// Answers a request for `path` (the URL's path): the image's bytes, or why not.
pub async fn serve(app: &App, path: &str) -> Served {
    let Some((key, n, attachment)) = parse(path) else {
        return Served::refused(400, "not an attachment");
    };
    let Some(data) = app.service.data() else {
        return Served::refused(503, "no data folder");
    };
    let file = match app
        .service
        .send(Request::TicketReport(TicketReportParams {
            key: key.to_string(),
            n,
            attachment: Some(attachment),
        }))
        .await
    {
        Ok(Reply::TicketReport(file)) => file,
        _ => return Served::refused(404, "no such attachment"),
    };
    let a = &file.attachment;
    let (AttachmentKind::Image, Some(mime)) = (a.kind, mime(&a.name)) else {
        return Served::refused(403, "not an image");
    };
    let Some(real) = confine(&data.reports_dir(), Path::new(&a.path)) else {
        return Served::refused(403, "outside the reports folder");
    };
    match tokio::task::spawn_blocking(move || std::fs::read(real)).await {
        Ok(Ok(body)) => Served {
            status: 200,
            mime,
            body,
        },
        _ => Served::refused(404, "the image can\u{2019}t be read"),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    use super::*;

    #[test]
    fn a_path_names_a_ticket_a_comment_and_an_attachment_only() {
        let key = |s: &str| s.parse::<TicketKey>().unwrap();
        assert_eq!(parse("/SHOP-34/4/1"), Some((key("SHOP-34"), 4, 1)));
        assert_eq!(parse("/SHOP-34%2F4%2F2"), Some((key("SHOP-34"), 4, 2)));
        for bad in [
            "/",
            "/SHOP-34/4",
            "/SHOP-34/4/1/x",
            "/SHOP-34/../../etc/passwd",
            "/%2E%2E%2Fetc%2Fpasswd",
            "/etc/passwd",
            "/SHOP-34/4/%ZZ",
            "/SHOP-34/-1/1",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_image_types_have_a_mime() {
        assert_eq!(mime("shot.PNG"), Some("image/png"));
        assert_eq!(mime("a.jpeg"), Some("image/jpeg"));
        assert_eq!(mime("logo.svg"), Some("image/svg+xml"));
        assert_eq!(mime("run.txt"), None);
        assert_eq!(mime("noext"), None);
    }

    /// Only a file inside the reports folder is served: not one outside it, nor one reached through `..` or a link.
    #[test]
    fn a_file_outside_the_reports_folder_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let reports = root.path().join("reports");
        let inside = reports.join("shop-web/SHOP-34/c/1");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::write(inside.join("shot.png"), b"png").unwrap();
        let secret = root.path().join("secret.png");
        std::fs::write(&secret, b"secret").unwrap();

        assert!(confine(&reports, &inside.join("shot.png")).is_some());
        assert_eq!(confine(&reports, &secret), None);
        assert_eq!(
            confine(&reports, &inside.join("../../../../../secret.png")),
            None
        );
        assert_eq!(confine(&reports, &inside), None, "a folder isn't a file");
        assert_eq!(confine(&reports, &inside.join("missing.png")), None);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, inside.join("link.png")).unwrap();
            assert_eq!(confine(&reports, &inside.join("link.png")), None);
        }
    }

    /// A posted image is served by its ticket and numbers with its type; its text attachment, an unknown one and a
    /// path are refused, and a deleted comment's image still loads (History).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_posted_image_is_served_by_its_numbers() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let Reply::ProjectAdded { project, .. } = h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap()
        else {
            panic!("no project");
        };
        let Reply::Ticket(created) = h
            .app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.key.clone(),
                title: "Export orders as CSV".into(),
                ..Default::default()
            }))
            .await
            .unwrap()
        else {
            panic!("no ticket");
        };
        let key = created.key.to_string();
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        let shot = repos.repo("shots", 1).join("shot.png");
        std::fs::write(&shot, &png).unwrap();
        table(
            &h.app,
            "ticket_comments.post",
            json!({ "key": key, "body": "The page.", "attachments": [{ "kind": "file", "path": shot.display().to_string() }] }),
        )
        .await
        .unwrap();
        table(
            &h.app,
            "ticket_comments.post",
            json!({ "key": key, "body": "The log.", "attachments": [{ "kind": "pasted", "text": "one\ntwo\n" }] }),
        )
        .await
        .unwrap();

        let served = serve(&h.app, &format!("/{key}%2F1%2F1")).await;
        assert_eq!((served.status, served.mime), (200, "image/png"));
        assert_eq!(served.body, png);
        assert_eq!(
            serve(&h.app, &format!("/{key}/2/1")).await.status,
            403,
            "text"
        );
        assert_eq!(serve(&h.app, &format!("/{key}/1/9")).await.status, 404);
        assert_eq!(serve(&h.app, "/SHOP-999/1/1").await.status, 404);
        let outside = serve(&h.app, &shot.display().to_string()).await;
        assert_eq!(outside.status, 400, "a path is never taken");

        table(
            &h.app,
            "ticket_comments.delete",
            json!({ "key": key, "n": 1 }),
        )
        .await
        .unwrap();
        assert_eq!(serve(&h.app, &format!("/{key}/1/1")).await.body, png);
        h.service.stop_all().await;
    }
}
