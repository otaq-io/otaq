// Area 03 · Board (T-0220, T-0221): the board slot, New ticket (⌘N, `c`: the box for new work opens) and, inside the
// board, ⌘D, Park and Esc during a drag; the header's search and filter field and `/`; the card's right-click menu, the
// delete confirmation every place that deletes opens (`delete_ticket`) and the drag-back dialog (`drag_back`, T-0421).
// What it says is Rust's (`src/api/board.rs`,
// the `otaq-view` board, filter, card-menu and delete view models). A board a newer Otaq changed shows its screen
// instead (UPD-08, `withNewerDatabase`).
import { openOverlay } from "../../app/overlays";
import type { Screen } from "../../app/registry";
import { selection } from "../../app/selection";
import { openNewWork } from "../new_work/logic";
import { withNewerDatabase } from "../recovery/NewerDatabase";
import { GitIdentityDialog } from "../settings/GitIdentity";
import { Board } from "./Board";
import "./board.css";
import { DeleteDialog } from "./DeleteDialog";
import { DragBackDialog } from "./DragBackDialog";
import { requestSearchFocus } from "./filter";
import { SearchField } from "./Search";

export const board: Screen = (app) => {
  app.slots.register("board", { Component: withNewerDatabase(Board) });
  app.slots.register("header_search", { Component: SearchField });
  app.overlays.register("delete_ticket", { Component: DeleteDialog });
  app.overlays.register("drag_back", { Component: DragBackDialog });
  // A start when git has no name and email (the board's Start and drop, Create & start, Noyan's created rows).
  app.overlays.register("git_identity", { Component: GitIdentityDialog });
  app.shortcuts.on("new_ticket", () => {
    openNewWork(() => {
      openOverlay("new_ticket");
    });
  });
  // `/` (Filter the board, d0097): the field takes the keyboard (only while a project is shown).
  app.shortcuts.on("filter_board", () => {
    if (selection.get().project === null) return false;
    requestSearchFocus();
    return true;
  });
};
