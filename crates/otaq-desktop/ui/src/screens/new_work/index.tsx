// The box for new work (UX-22): the `new_work` slot at the bottom centre of the board and the List. ⌘N opens it
// (the board's `new_ticket` shortcut, `openNewWork`); what it says is Rust's (`src/api/new_work.rs`, `otaq-view`'s
// `new_work.rs`).
import type { Screen } from "../../app/registry";
import { NewWork } from "./NewWork";
import "./new_work.css";

export const newWork: Screen = (app) => {
  app.slots.register("new_work", { Component: NewWork });
};
