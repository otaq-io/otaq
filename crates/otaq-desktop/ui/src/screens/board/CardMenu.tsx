// The card's right-click menu (*03 · Board & columns → Pinned tickets*; TKT-06, TKT-09, TKT-08, CLS-01; d0047, d0065,
// d0060, d0033): only the items `card_menu.rs` allows for the ticket, worded there (`board.menu`). Pin / Unpin and the
// priorities are sent at once (`board.menu_choose`); Delete… and Close ticket… open their confirmations by overlay
// id. A disabled item (Delete… on a started ticket) can't be chosen.
import { AnchoredMenu, type Anchor } from "../../components";
import type { CardMenuChoice } from "../../generated/CardMenuChoice";
import type { CardMenuParams } from "../../generated/CardMenuParams";
import type { MenuItem } from "../../generated/MenuItem";
import { openOverlay } from "../../app/overlays";
import { request } from "../../tauri";

/** An open card menu: whose, where, and its items. */
export interface OpenCardMenu {
  key: string;
  anchor: Anchor;
  items: readonly MenuItem[];
}

/** What the page does with Rust's answer to a choice. */
export function afterMenuChoice(choice: CardMenuChoice, key: string): void {
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
  }
}

/** Sends a choice and carries out the answer; a refusal reaches `onError`. */
export function chooseMenuItem(
  key: string,
  id: string,
  onError: (error: unknown) => void,
  ask: typeof request = request,
): Promise<void> {
  const params: CardMenuParams = { key, id };
  return ask("board.menu_choose", params).then((choice) => {
    afterMenuChoice(choice as CardMenuChoice, key);
  }, onError);
}

export interface CardMenuProps {
  menu: OpenCardMenu;
  onError: (error: unknown) => void;
  onClose: () => void;
}

export function CardMenu({ menu, onError, onClose }: CardMenuProps) {
  return (
    <AnchoredMenu
      label={`${menu.key} actions`}
      anchor={menu.anchor}
      items={menu.items}
      onChoose={(id) => {
        void chooseMenuItem(menu.key, id, onError, request);
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
