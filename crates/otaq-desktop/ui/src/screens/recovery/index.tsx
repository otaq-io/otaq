// Area 16 · Recovery and Cleanup (T-0235): the after-restart summary sheet (opened by itself once per service start
// that interrupted sessions, and by the banner's Review & resume), Start from scratch's confirmation and the Cleanup
// overlay. The "Before the restart" view is the Terminal tab's (T-0224), Start interrupted the card's and the sheet's
// (T-0218, T-0222). What it says is Rust's (`src/api/recovery.rs`; `otaq-view`'s `after_restart.rs`,
// `recovery.rs`, `cleanup.rs`).
import { overlays } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import { noticesStore } from "../notices/notices";
import { AfterRestart } from "./AfterRestart";
import { Cleanup } from "./Cleanup";
import { summaryToOpen } from "./logic";
import "./recovery.css";
import { StartFromScratch } from "./StartFromScratch";

export const recovery: Screen = (app) => {
  app.overlays.register("after_restart", { Component: AfterRestart });
  app.overlays.register("start_from_scratch", { Component: StartFromScratch });
  app.overlays.register("cleanup", { Component: Cleanup });
  let opened: number | null = null;
  noticesStore.subscribe(() => {
    const start = summaryToOpen(noticesStore.get(), opened);
    if (start === null) return;
    opened = start;
    overlays.open("after_restart");
  });
};
