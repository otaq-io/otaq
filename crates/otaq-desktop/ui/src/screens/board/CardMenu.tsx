// The card's right-click menu (*03 · Board & columns → Pinned tickets*; TKT-06, TKT-09, TKT-08, CLS-01; d0047, d0065,
// d0060, d0033): only the items `card_menu.rs` allows for the ticket, worded there (`board.menu`). Pin / Unpin and the
// priorities are sent at once (`board.menu_choose`); Delete… and Close ticket… open their confirmations by overlay
// id, as do the epic dialogs (T-0223). A disabled item (Delete… on a started ticket) can't be chosen. An epic's own
// items (its card's or its track's menu; T-0274, d0080) do what the board does: Open epic opens its sheet, Queue and
// Unqueue are the drops they stand for (on Ready for Dev, on Backlog: the queue toast, the Park-all confirmation),
// Add ticket to epic opens New ticket under it, Focus is `g` `e`.
import { openTicket } from "../../app/selection";
import { AnchoredMenu, type Anchor } from "../../components";
import type { BoardDropTarget } from "../../generated/BoardDropTarget";
import type { CardMenuChoice } from "../../generated/CardMenuChoice";
import type { CardMenuParams } from "../../generated/CardMenuParams";
import type { MenuItem } from "../../generated/MenuItem";
import { openOverlay } from "../../app/overlays";
import { request } from "../../tauri";
import { BACKLOG, READY_FOR_DEV } from "./drag";

/** An open card menu: whose, where, and its items. */
export interface OpenCardMenu {
  key: string;
  anchor: Anchor;
  items: readonly MenuItem[];
}

/** What the board does for an epic's items: a drop, as a drag would end, and the focus (`g` `e`). */
export interface MenuBoard {
  drop: (key: string, column: BoardDropTarget) => void;
  focus: (key: string) => void;
}

const NO_BOARD: MenuBoard = { drop: () => undefined, focus: () => undefined };

/** What the page does with Rust's answer to a choice. */
export function afterMenuChoice(choice: CardMenuChoice, key: string, board: MenuBoard = NO_BOARD): void {
  switch (choice.kind) {
    case "sent":
    case "nothing":
      return;
    case "delete":
      openOverlay("delete_ticket", { ticket: key });
      return;
    case "close_ticket":
      openOverlay("close_ticket", { ticket: key });
      return;
    case "move_to_epic":
      openOverlay("move_to_epic", { ticket: key });
      return;
    case "close_epic":
      openOverlay("close_epic", { epic: key });
      return;
    case "open_epic":
      openTicket(key);
      return;
    case "queue_epic":
      board.drop(key, READY_FOR_DEV);
      return;
    case "take_back_epic":
      board.drop(key, BACKLOG);
      return;
    case "add_to_epic":
      openOverlay("new_ticket", { epic: key });
      return;
    case "focus_epic":
      board.focus(key);
      return;
  }
}

/** Sends a choice and carries out the answer; a refusal reaches `onError`. */
export function chooseMenuItem(
  key: string,
  id: string,
  onError: (error: unknown) => void,
  ask: typeof request = request,
  board: MenuBoard = NO_BOARD,
): Promise<void> {
  const params: CardMenuParams = { key, id };
  return ask("board.menu_choose", params).then((choice) => {
    afterMenuChoice(choice as CardMenuChoice, key, board);
  }, onError);
}

export interface CardMenuProps {
  menu: OpenCardMenu;
  board: MenuBoard;
  onError: (error: unknown) => void;
  onClose: () => void;
}

export function CardMenu({ menu, board, onError, onClose }: CardMenuProps) {
  return (
    <AnchoredMenu
      label={`${menu.key} actions`}
      anchor={menu.anchor}
      items={menu.items}
      onChoose={(id) => {
        void chooseMenuItem(menu.key, id, onError, request, board);
      }}
      onClose={(how) => {
        onClose();
        // Esc, Tab or a choice hands the keyboard back to the card.
        if (how === "key")
          document.querySelector<HTMLElement>(`[data-card="${CSS.escape(menu.key)}"]`)?.focus();
      }}
    />
  );
}
