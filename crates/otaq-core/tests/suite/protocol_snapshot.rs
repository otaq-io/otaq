//! The protocol number is bumped by every incompatible change (UPD-03, d0140): the protocol's wire shape is read
//! from the source (every type reachable from `protocol::Message`) and compared with the committed
//! `src/protocol.snapshot.json`, which also records `PROTOCOL_VERSION`.
//!
//! - The shape is unchanged: passes.
//! - Only compatible changes (see `PROTOCOL_VERSION`'s comment): fails until the snapshot is regenerated.
//! - Any incompatible change: fails until `PROTOCOL_VERSION` is bumped and the snapshot regenerated; regenerating
//!   refuses while the number is the same.
//!
//! Regenerate with `OTAQ_UPDATE_PROTOCOL_SNAPSHOT=1 cargo nextest run -p otaq-core protocol_snapshot`.
//!
//! What it can't see: the inside of hand-written `Serialize` / `Deserialize` impls beyond their own text (what
//! they call, such as `as_str` or `FromStr`), `#[serde(with = ...)]` modules, and types from other crates (named
//! only). Changing those on purpose still needs a bump by hand.

mod compare;
mod schema;

use std::path::Path;

use otaq_core::protocol::PROTOCOL_VERSION;

use compare::{Changes, compare};
use schema::{Crate, Schema, extract};

const UPDATE_VAR: &str = "OTAQ_UPDATE_PROTOCOL_SNAPSHOT";
const UPDATE_COMMAND: &str =
    "OTAQ_UPDATE_PROTOCOL_SNAPSHOT=1 cargo nextest run -p otaq-core protocol_snapshot";

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn current() -> Schema {
    let krate = Crate::read(&manifest_dir().join("src"));
    extract(&krate, "protocol", "Message", PROTOCOL_VERSION)
}

fn list(items: &[String]) -> String {
    items.iter().map(|i| format!("  - {i}\n")).collect()
}

/// What the developer is told, or `None` when the snapshot matches.
fn verdict(committed: &Schema, current: &Schema) -> Option<String> {
    if committed == current {
        return None;
    }
    let Changes {
        compatible,
        incompatible,
    } = compare(committed, current);
    let file = "crates/otaq-core/src/protocol.rs";
    if current.protocol_version != committed.protocol_version {
        return Some(format!(
            "PROTOCOL_VERSION is {} but the protocol snapshot records {}. Regenerate the snapshot:\n  {UPDATE_COMMAND}\n",
            current.protocol_version, committed.protocol_version
        ));
    }
    if !incompatible.is_empty() {
        return Some(format!(
            "The protocol's messages changed in a way an older background service can't handle:\n{}\
             What to do: bump PROTOCOL_VERSION in {file} (now {}), then regenerate the snapshot:\n  {UPDATE_COMMAND}\n\
             (Or make the change compatible: new fields optional, nothing removed or renamed on the wire.)\n",
            list(&incompatible),
            current.protocol_version,
        ));
    }
    let what = if compatible.is_empty() {
        "  - no change on the wire (a Rust-only rename)\n".to_string()
    } else {
        list(&compatible)
    };
    Some(format!(
        "The protocol's messages changed compatibly, so PROTOCOL_VERSION ({}) stays:\n{what}\
         What to do: regenerate the snapshot:\n  {UPDATE_COMMAND}\n",
        current.protocol_version
    ))
}

fn read_snapshot(path: &Path) -> Option<Schema> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!(
            "{} isn't readable ({e}); regenerate it:\n  {UPDATE_COMMAND}",
            path.display()
        )
    }))
}

#[test]
fn protocol_snapshot_matches_the_protocol() {
    let path = manifest_dir().join("src/protocol.snapshot.json");
    let current = current();
    let committed = read_snapshot(&path);
    if std::env::var_os(UPDATE_VAR).is_some_and(|v| v == "1") {
        if let Some(committed) = &committed
            && committed.protocol_version == current.protocol_version
        {
            let changes = compare(committed, &current);
            assert!(
                changes.incompatible.is_empty(),
                "Not regenerating the protocol snapshot: these changes need PROTOCOL_VERSION bumped first (in \
                 crates/otaq-core/src/protocol.rs, now {}):\n{}",
                current.protocol_version,
                list(&changes.incompatible)
            );
        }
        let mut text = serde_json::to_string_pretty(&current).expect("the schema serializes");
        text.push('\n');
        std::fs::write(&path, text).expect("writing the protocol snapshot");
        return;
    }
    let Some(committed) = committed else {
        panic!(
            "{} is missing; generate it:\n  {UPDATE_COMMAND}",
            path.display()
        );
    };
    if let Some(message) = verdict(&committed, &current) {
        panic!("{message}");
    }
}

/// `scripts/release/protocol-version.sh` (for `latest.json`, T-0583) reads the number from this exact line.
#[test]
fn protocol_snapshot_version_line_is_readable_by_the_release_script() {
    let source = std::fs::read_to_string(manifest_dir().join("src/protocol.rs")).unwrap();
    let line = format!("pub const PROTOCOL_VERSION: u32 = {PROTOCOL_VERSION};");
    assert_eq!(
        source.lines().filter(|l| *l == line).count(),
        1,
        "protocol.rs must have exactly one line `{line}`"
    );
}

/// The whole protocol is reached: every request, reply and event, and what they carry.
#[test]
fn protocol_snapshot_covers_requests_replies_and_events() {
    let schema = current();
    // `Message` is also a type of `messages`, so the root goes by its full path.
    assert_eq!(schema.root, "crate::protocol::Message");
    for name in [
        "crate::protocol::Message",
        "Body",
        "Request",
        "Reply",
        "Event",
        "ErrorBody",
        "ErrorKind",
        "ServiceStatus",
        "SubscribeParams",
        "BoardSnapshot",
        "Ticket",
        "TicketKey",
        "Status",
    ] {
        assert!(
            schema.types.contains_key(name),
            "{name} isn't in the schema"
        );
    }
    let schema::Shape::Enum { variants, attrs } = &schema.types["Request"] else {
        panic!("Request is an enum");
    };
    assert_eq!(attrs, &["content=params", "tag=method"]);
    assert_eq!(
        variants["events.subscribe"].newtype.as_deref(),
        Some("SubscribeParams")
    );
    assert!(variants["ping"].newtype.is_none() && variants["ping"].fields.is_none());
}

// ---- The checker itself, on small made-up protocols ----

fn schema_of(version: u32, source: &str) -> Schema {
    let krate = Crate::from_sources(&[("protocol", source)]);
    extract(&krate, "protocol", "Message", version)
}

const BASE: &str = r#"
    use serde::{Serialize, Deserialize};
    #[derive(Serialize, Deserialize)]
    pub struct Message { pub protocol_version: u32, #[serde(flatten)] pub body: Body }
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Body { Request(Request), Event(Event) }
    #[derive(Serialize, Deserialize)]
    #[serde(tag = "method", content = "params")]
    pub enum Request {
        #[serde(rename = "ping")] Ping,
        #[serde(rename = "ticket.move")] TicketMove(MoveParams),
    }
    #[derive(Serialize, Deserialize)]
    pub struct MoveParams { pub key: String, pub to: Column, #[serde(default)] pub note: Option<String> }
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Column { Backlog, InReview }
    #[derive(Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum Event { TicketChanged { key: String }, UsageChanged }
"#;

fn changes(new: &str) -> Changes {
    compare(&schema_of(1, BASE), &schema_of(1, new))
}

#[test]
fn protocol_snapshot_the_same_source_has_no_changes() {
    assert_eq!(changes(BASE), Changes::default());
    let schema = schema_of(1, BASE);
    assert_eq!(
        schema.types["MoveParams"],
        serde_json::from_value(serde_json::json!({
            "kind": "struct",
            "fields": {
                "key": {"ty": "String", "required": true},
                "to": {"ty": "Column", "required": true},
                "note": {"ty": "Option<String>"},
            }
        }))
        .unwrap()
    );
}

#[test]
fn protocol_snapshot_renaming_a_request_field_is_incompatible() {
    let c = changes(&BASE.replace("pub key: String, pub to", "pub ticket: String, pub to"));
    assert_eq!(
        c.incompatible,
        [
            "MoveParams: field `key` removed or renamed",
            "MoveParams: new required field `ticket`"
        ]
    );
}

#[test]
fn protocol_snapshot_a_rust_only_rename_is_no_change() {
    let c = changes(
        &BASE
            .replace(
                "pub key: String, pub to",
                "#[serde(rename = \"key\")] pub ticket: String, pub to",
            )
            .replace("MoveParams", "MoveTicket"),
    );
    assert_eq!(c, Changes::default());
}

#[test]
fn protocol_snapshot_new_optional_fields_variants_and_requests_are_compatible() {
    let c = changes(
        &BASE
            .replace(
                "pub note: Option<String> }",
                "pub note: Option<String>, pub pin: Option<bool>, #[serde(default)] pub labels: Vec<String> }",
            )
            .replace("Backlog, InReview", "Backlog, InReview, Done")
            .replace("UsageChanged }", "UsageChanged, SettingsChanged }")
            .replace(
                "TicketMove(MoveParams),",
                "TicketMove(MoveParams), #[serde(rename = \"usage.summary\")] UsageSummary,",
            ),
    );
    assert!(c.incompatible.is_empty(), "{:?}", c.incompatible);
    assert_eq!(
        c.compatible,
        [
            "Column: new variant `done`",
            "Event: new variant `settings_changed`",
            "MoveParams: new optional field `labels`",
            "MoveParams: new optional field `pin`",
            "Request: new variant `usage.summary`",
        ]
    );
}

#[test]
fn protocol_snapshot_removals_type_changes_and_required_fields_are_incompatible() {
    let cases = [
        (
            BASE.replace("#[serde(rename = \"ping\")] Ping,", ""),
            "Request: variant `ping` removed or renamed",
        ),
        (
            BASE.replace(
                "#[serde(rename = \"ticket.move\")]",
                "#[serde(rename = \"ticket.drag\")]",
            ),
            "Request: variant `ticket.move` removed or renamed",
        ),
        (
            BASE.replace("pub key: String,", "pub key: u64,"),
            "MoveParams.key: type changed from `String` to `u64`",
        ),
        (
            BASE.replace(
                "#[serde(default)] pub note: Option<String>",
                "pub note: String",
            ),
            "MoveParams.note: now required",
        ),
        (
            BASE.replace("{ key: String }", "{ key: String, project: String }"),
            "Event `ticket_changed`: new required field `project`",
        ),
        (
            BASE.replace("Backlog, InReview", "Backlog, Review"),
            "Column: variant `in_review` removed or renamed",
        ),
        (
            BASE.replace(
                "#[serde(tag = \"kind\", rename_all",
                "#[serde(tag = \"type\", rename_all",
            ),
            "Event: serde attributes changed from [\"tag=kind\"] to [\"tag=type\"]",
        ),
    ];
    for (source, expected) in cases {
        let c = changes(&source);
        assert!(
            c.incompatible.iter().any(|i| i == expected),
            "expected {expected:?} in {:?}",
            c.incompatible
        );
    }
}

#[test]
fn protocol_snapshot_messages_say_what_to_do() {
    let old = schema_of(1, BASE);
    assert_eq!(verdict(&old, &old), None);

    let renamed = schema_of(
        1,
        &BASE.replace("pub key: String, pub to", "pub ticket: String, pub to"),
    );
    let message = verdict(&old, &renamed).unwrap();
    assert!(message.contains("bump PROTOCOL_VERSION"), "{message}");
    assert!(message.contains(UPDATE_COMMAND), "{message}");

    let bumped = schema_of(
        2,
        &BASE.replace("pub key: String, pub to", "pub ticket: String, pub to"),
    );
    let message = verdict(&old, &bumped).unwrap();
    assert!(
        message.contains("PROTOCOL_VERSION is 2 but the protocol snapshot records 1"),
        "{message}"
    );

    let added = schema_of(
        1,
        &BASE.replace("UsageChanged }", "UsageChanged, SettingsChanged }"),
    );
    let message = verdict(&old, &added).unwrap();
    assert!(message.contains("changed compatibly"), "{message}");
    assert!(!message.contains("bump"), "{message}");
}
