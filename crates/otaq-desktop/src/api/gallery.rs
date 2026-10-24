//! `gallery.*`: the design gallery (T-0217), a developer-only page for comparing the foundations and the components
//! with *crewboard.pen → 01* by hand. It opens over the app when the app starts with `OTAQ_GALLERY=1`, as the
//! GPUI app's gallery did.

use std::ffi::OsStr;

use otaq_core::naming;
use otaq_view::view_models::{
    activity, agent_panel, agent_tools_page, ask_bar, board_page, card_page, cleanup, decision_change, decision_item, gallery, lead_status, lead_tab, needs_you_strip,
    limit_banner, onboarding_page, settings_page, stage_choices, tool_avatar,
};
use serde_json::Value;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(_app: &App, method: &str, _params: Value) -> Result<Value, RequestError> {
    match method {
        "gallery.wanted" => reply(&wanted(std::env::var_os(env_var()).as_deref())),
        "gallery.view" => reply(&gallery::view()),
        // The card grids (T-0218): every Ticket Card and Epic Card variant from the fixtures.
        "gallery.cards" => reply(&card_page::gallery()),
        // Every Decision Item variant and the two review lists (T-0287).
        "gallery.decisions" => reply(&decision_item::gallery()),
        // The Change… and Add a fix child dialogs (T-0288).
        "gallery.decision_change" => reply(&decision_change::gallery()),
        // The Lead Status, the track label and the epic card with a lead (T-0289).
        "gallery.leads" => reply(&lead_status::gallery()),
        // The epic sheet's Lead tab in each state (T-0290).
        "gallery.lead_tabs" => reply(&lead_tab::gallery()),
        // The Needs you strip in each state of `s24-03` and `s24-12` (T-0459).
        "gallery.needs_you_strip" => reply(&needs_you_strip::gallery()),
        // The List view by stage, status and agent (T-0461, `s24-03`, `s24-04`).
        "gallery.list_view" => reply(&otaq_view::view_models::list_view::gallery()),
        // Pause and Resume in the title bar with their scope menus (T-0457, `s24-07`, `s24-08`).
        "gallery.pause" => reply(&otaq_view::view_models::pause::gallery(
            &otaq_view::key_labels::KeyLabels::base(otaq_view::keymap::Platform::Mac),
        )),
        // The Files tab: the tree and a file, a search, All changes with a line comment, the review bar (T-0468).
        "gallery.files" => reply(&otaq_view::view_models::files_tab::gallery()),
        // The project Files view (T-0478): All with the chips, Needs you, loading, empty, the picker, one ticket.
        "gallery.files_view" => reply(&otaq_view::view_models::files_view::gallery()),
        // Settings' Environment variables row in each state (T-0300).
        "gallery.env_vars" => reply(&settings_page::env_vars_gallery()),
        // Settings' warm build switch in each state (T-0301).
        "gallery.warm_switch" => reply(&settings_page::warm_switch_gallery()),
        // Cleanup's warm builds section in each state (T-0302).
        "gallery.cleanup_warm" => reply(&cleanup::warm_gallery()),
        // The add-project form's folder trust line and failed state, the note and the failed warning (T-0306).
        "gallery.trust" => reply(&onboarding_page::trust_gallery()),
        // The four tool avatars and the line saying they are hidden while one tool is connected (T-0321).
        "gallery.tool_avatars" => reply(&tool_avatar::gallery()),
        // The Review column's states, the drop verdicts, the empty board and the agent chip (T-0324).
        "gallery.board_review" => reply(&board_page::gallery()),
        // Activity's button and popover in each state (T-0325).
        "gallery.activity" => reply(&activity::gallery()),
        // The Project agent panel in each state (T-0330).
        "gallery.agent_panel" => reply(&agent_panel::gallery()),
        // The Ask bar and the rail's Project agent button (T-0458).
        "gallery.ask_bar" => reply(&ask_bar::gallery()),
        // The Terminals drawer: terminals only (T-0331).
        "gallery.drawer" => reply(&super::drawer::gallery()),
        // 15 · Settings → Agents: fresh install, a list you made, the picker, the mark, ⋯ and Delete, the project
        // block (T-0340; T-0343 draws it).
        // 05/08 · Stage choices and the path: the ⌘N form, mid-flow, after a Fail, finished, a choice you set, the
        // default flow (T-0411).
        "gallery.stage_choices" => reply(&stage_choices::gallery()),
        // 15 · Settings → Agent tools: every state per tool, the notes, Disconnect's dialog (T-0360; T-0365 draws it).
        "gallery.agent_tools" => reply(&agent_tools_page::gallery()),
        // 14 · The Tools step with agent tools: the sample, none connected, one connected, hidden and warned (T-0365).
        "gallery.onboarding_tools" => reply(&onboarding_page::tools_gallery()),
        // 27 · The flow editor: shop-web's flow in the project, Otaq default in onboarding with two tools (T-0471).
        "gallery.flow_editor" => reply(&otaq_view::view_models::flow_editor::gallery()),
        // 26 · The project menu: the flow in use, and a change waiting (T-0473).
        "gallery.project_menu" => reply(&otaq_view::view_models::project_menu::gallery()),
        // 11 · The usage-limit banner's line per tool, Move waiting to…, a tool that isn't available (T-0364).
        "gallery.limit_lines" => reply(&limit_banner::gallery()),
        // The notification banners and Settings → Notifications in every state (T-0372).
        "gallery.notifications" => reply(&settings_page::notifications_gallery()),
        // Settings → Keyboard shortcuts in every frame of update AF, macOS and Windows / Linux (T-0395).
        "gallery.keyboard_page" => reply(&otaq_view::view_models::keyboard_page::gallery()),
        // The answer options' notes in the sheet and the card's hover card (T-0375).
        "gallery.option_notes" => reply(&super::sheet::notes_gallery()),
        // 25 · The ticket sheet's header, details line and path: needs you, your check, an epic's child, Backlog
        // (T-0464).
        "gallery.sheet" => reply(&super::sheet::sheet_gallery()),
        // 25 · Overview's attachments and description words for SHOP-22 (T-0465).
        "gallery.overview" => reply(&otaq_view::view_models::overview::gallery()),
        "gallery.epic_sheet" => reply(&otaq_view::view_models::epic_sheet::gallery()),
        // The New ticket form's report: its words and its box (T-0474).
        "gallery.new_ticket_report" => reply(&otaq_view::view_models::editor::report_field()),
        // 24 · Dependencies: a blocked card, a blocker, the sheet's section and picker, DPV-03's dialog (T-0463).
        "gallery.dependencies" => reply(&otaq_view::view_models::dependencies::gallery()),
        _ => Err(unknown(method)),
    }
}

/// `OTAQ_GALLERY`.
fn env_var() -> String {
    naming::env_var("GALLERY")
}

/// Whether the variable asks for the gallery: set, and neither empty nor `0`.
fn wanted(value: Option<&OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_opens_the_gallery_unless_empty_or_zero() {
        assert_eq!(env_var(), format!("{}GALLERY", naming::env_var("")));
        assert!(wanted(Some(OsStr::new("1"))));
        assert!(wanted(Some(OsStr::new("yes"))));
        for off in [None, Some(OsStr::new("")), Some(OsStr::new("0"))] {
            assert!(!wanted(off), "{off:?}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_view_is_served_by_the_request_table() {
        let h = crate::service::testing::Harness::new(None);
        let view = crate::api::request(&h.app, "gallery.view", Value::Null)
            .await
            .unwrap();
        assert_eq!(view["legend"].as_array().map(Vec::len), Some(4));
        assert_eq!(view["keyboard_map"]["letters"][0]["label"], "Next / previous ticket (panel)");
        assert_eq!(view["shortcut_lists"][1]["columns"][1][0]["rows"][3]["mark"], "changed");
        assert_eq!(view["priority"]["chips"][0]["short"], "P1");
        let cards = crate::api::request(&h.app, "gallery.cards", Value::Null)
            .await
            .unwrap();
        assert_eq!(cards["variants"][0]["card"]["key"], "SHOP-31");
        let leads = crate::api::request(&h.app, "gallery.leads", Value::Null)
            .await
            .unwrap();
        assert_eq!(leads["statuses"][0]["view"]["full"], "Wishlist lead · idle");
        assert!(cards["epics"].as_array().is_some_and(|e| !e.is_empty()));
        let tabs = crate::api::request(&h.app, "gallery.lead_tabs", Value::Null)
            .await
            .unwrap();
        assert_eq!(tabs["tabs"][0]["view"]["status"]["full"], "Wishlist lead · idle");
        let env = crate::api::request(&h.app, "gallery.env_vars", Value::Null)
            .await
            .unwrap();
        assert_eq!(env["cells"][0]["row"]["control"]["kind"], "env_vars");
        let warm = crate::api::request(&h.app, "gallery.cleanup_warm", Value::Null)
            .await
            .unwrap();
        assert_eq!(warm["cells"][0]["page"]["warm"]["total"], "41.6 GB on disk");
        let switch = crate::api::request(&h.app, "gallery.warm_switch", Value::Null)
            .await
            .unwrap();
        assert_eq!(switch["cells"][0]["row"]["control"]["kind"], "warm_build");
        let trust = crate::api::request(&h.app, "gallery.trust", Value::Null)
            .await
            .unwrap();
        assert_eq!(trust["banners"][0]["banners"][0]["kind"], "trust_note");
        assert!(trust["forms"][2]["form"]["trust_error"].is_string());
        let avatars = crate::api::request(&h.app, "gallery.tool_avatars", Value::Null)
            .await
            .unwrap();
        assert_eq!(avatars["avatars"][1]["background"], "tool-cx");
        assert_eq!(avatars["shown"], false);
        let board = crate::api::request(&h.app, "gallery.board_review", Value::Null)
            .await
            .unwrap();
        assert_eq!(board["review"][0]["column"]["sections"][1]["id"], "approved");
        let choices = crate::api::request(&h.app, "gallery.stage_choices", Value::Null)
            .await
            .unwrap();
        assert_eq!(choices["samples"][0]["choices"]["rows"][1]["value"], "Rust developer");
        assert_eq!(choices["samples"][1]["path"]["rows"][1]["detail"], "Rust developer \u{b7} working");
        let notifications = crate::api::request(&h.app, "gallery.notifications", Value::Null)
            .await
            .unwrap();
        assert_eq!(
            notifications["banners"][0]["banners"][0]["kind"],
            "notifications_ask"
        );
        assert_eq!(notifications["pages"].as_array().map(Vec::len), Some(6));
        let lines = crate::api::request(&h.app, "gallery.limit_lines", Value::Null)
            .await
            .unwrap();
        assert_eq!(lines["cells"][0]["lines"][0]["title"], "Claude Code limit reached");
        h.service.stop_all().await;
    }
}
