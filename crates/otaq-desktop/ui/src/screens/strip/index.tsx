// The Needs you strip (T-0459; NYS-01…NYS-03, d0114): under the toolbar of the Board and the List (each view renders
// `<NeedsYouStrip />`), Next on ⌘J from anywhere, and Resume all's confirmation. It replaces the Inbox overlay. What
// it says is Rust's (`src/api/strip.rs`, `otaq-view`'s `needs_you_strip.rs`).
import type { Screen } from "../../app/registry";
import { next } from "./logic";
import { ResumeAll } from "./ResumeAll";
import "./strip.css";

export const strip: Screen = (app) => {
  app.overlays.register("resume_all", { Component: ResumeAll });
  // From anywhere, a focused terminal included (keys.ts passes the app's keys).
  app.shortcuts.on("next_needs_you_item", next);
};
