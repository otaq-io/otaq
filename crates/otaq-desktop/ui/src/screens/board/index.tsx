// Area 03 · Board (T-0220, T-0221): the board slot, New ticket (⌘N, `c`: a new Backlog ticket) and, inside the
// board, ⌘D, Park and Esc during a drag; the header's search and filter field and `/`; the card's right-click menu, the
// delete confirmation every place that deletes opens (`delete_ticket`), the drag-back dialog (`drag_back`, T-0421) and
// the blocked ticket's (`blocked`, DPV-03, T-0463). What it says is Rust's (`src/api/board.rs`,
// the `otaq-view` board, filter, card-menu and delete view models). A project in Flow setup shows its Flow setup page
// in the slot instead (ASK-04, `withFlowSetup`), and a board a newer Otaq changed its screen (UPD-08, `withNewerDatabase`).
import { openOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import { selection } from "../../app/selection";
import { withFlowSetup } from "../onboarding/FlowSetupPage";
import { withNewerDatabase } from "../recovery/NewerDatabase";
import { Board } from "./Board";
import "./board.css";
import { DeleteDialog } from "./DeleteDialog";
import { BlockedDialog } from "./BlockedDialog";
import { DragBackDialog } from "./DragBackDialog";
import { requestSearchFocus } from "./filter";
import { SearchField } from "./Search";

export const board: Screen = (app) => {
  app.slots.register("board", { Component: withNewerDatabase(withFlowSetup(Board)) });
  app.slots.register("header_search", { Component: SearchField });
  app.overlays.register("delete_ticket", { Component: DeleteDialog });
  app.overlays.register("drag_back", { Component: DragBackDialog });
  app.overlays.register("blocked", { Component: BlockedDialog });
  app.shortcuts.on("new_ticket", () => {
    openOverlay("new_ticket");
  });
  // `/` (Filter the board, d0097): the field takes the keyboard (only while a project is shown).
  app.shortcuts.on("filter_board", () => {
    if (selection.get().project === null) return false;
    requestSearchFocus();
    return true;
  });
};
