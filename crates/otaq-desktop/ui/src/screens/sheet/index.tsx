// Area 25 · the ticket sheet (SHT-01, SHT-02; T-0464): the sheet slot's sheet and the ticket's Close ticket and Submit
// for review dialogs (T-0222). Each tab's body is its own screen's: Overview `screens/overview` (T-0465), Session
// `screens/session` and Activity `screens/activity` (T-0466), Files `screens/files` (T-0468), an epic's Lead
// `screens/lead_tab`. What it says is Rust's (`src/api/sheet.rs`'s `sheet`, `view_models/sheet.rs`).
import type { Screen } from "../../app/registry";
import "./actions.css";
import { CloseTicketDialog, SubmitForReviewDialog } from "./Dialogs";
import { TicketSheet } from "./Sheet";
import "./sheet.css";

export const sheet: Screen = (app) => {
  app.slots.register("sheet", { Component: TicketSheet });
  app.overlays.register("close_ticket", { Component: CloseTicketDialog });
  app.overlays.register("submit_for_review", { Component: SubmitForReviewDialog });
};
