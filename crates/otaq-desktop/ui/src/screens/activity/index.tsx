// Area 25 · the sheet's Activity (SHT-05, T-0466; canvas `s25-07`): the ticket's History newest first, filtered by
// kind. What it says is Rust's (`src/api/sections.rs`'s `sections.history`, `history.rs`).
import type { Screen } from "../../app/registry";
import "./activity.css";
import { ActivityTab } from "./History";

export const activity: Screen = (app) => {
  app.sheetTabs.register("activity", { Component: ActivityTab });
};
