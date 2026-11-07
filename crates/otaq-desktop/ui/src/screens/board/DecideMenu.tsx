// Decide on a card whose check ran out of rounds ("Validate failed 3 times · decide"): a menu under the card with
// Rust's three ways on (`board.decide`, `decide_menu.rs`): Allow 1 more round and Send it on to <next stage> are the
// ticket's own actions (`sheet.action`), Stop here changes nothing. Its last line says where the rounds are set. Any
// other card's Decide opens the ticket, as before.
import type { Anchor } from "../../components";
import { Popover } from "../../components";
import type { DecideMenu as DecideMenuView } from "../../generated/DecideMenu";
import { request } from "../../tauri";

/** The door's request call, or a fake one in the tests. */
export type Ask = (method: string, params?: unknown) => Promise<unknown>;

/** The menu for `key`, or `null` when its Decide opens the ticket. */
export function decideMenu(key: string, ask: Ask = request): Promise<DecideMenuView | null> {
  return ask("board.decide", { key }) as Promise<DecideMenuView | null>;
}

/** A way on chosen: the ticket's action, or nothing for Stop here. */
export function decide(key: string, action: string, ask: Ask = request): Promise<unknown> {
  if (action === "stop") return Promise.resolve(null);
  return ask("sheet.action", { key, action });
}

export function DecideMenu({
  menu,
  anchor,
  onChose,
  onClose,
}: {
  menu: DecideMenuView;
  anchor: Anchor;
  onChose: (action: string) => void;
  onClose: () => void;
}) {
  return (
    <Popover label={menu.title} anchor={anchor} onClose={onClose}>
      <div className="board-decide">
        <span className="board-decide-title">{menu.title}</span>
        {menu.items.map((item) => (
          <button
            key={item.action}
            type="button"
            className="board-decide-item"
            onClick={() => {
              onChose(item.action);
            }}
          >
            <span className="board-decide-label">{item.label}</span>
            <span className="board-decide-hint">{item.hint}</span>
          </button>
        ))}
        <span className="board-decide-note">{menu.note}</span>
      </div>
    </Popover>
  );
}
