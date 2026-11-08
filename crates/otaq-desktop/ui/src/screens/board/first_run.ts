// A new user's first run on the board, without the DOM: the first ticket's Create & start and Not now, where a
// first-run hint points, and what its button does. Every word and every rule of when something shows is Rust's
// (`board.view`'s `first_ticket` and `hint`, `empty_board.rs`, `hint.rs`).
import { closeTicket, openTicket } from "../../app/selection";
import { createStore, type Store } from "../../app/store";
import type { BoardHint } from "../../generated/BoardHint";
import type { BoardHintAnchor } from "../../generated/BoardHintAnchor";
import type { EditorSaved } from "../../generated/EditorSaved";
import type { NotificationsLine } from "../../generated/NotificationsLine";
import { request } from "../../tauri";
import { createAndStart } from "../editor/useEditor";
import { openFlowBoard } from "../flow_board/logic";

export type Ask = (method: string, params?: unknown) => Promise<unknown>;

/** Otaq's line beside macOS's notifications question, while it is up; `null` otherwise. */
export const notificationsLine: Store<NotificationsLine | null> = createStore<NotificationsLine | null>(null);

/** The first ticket's card is answered for good (Create & start or Not now). */
export function answerFirstTicket(ask: Ask = request): Promise<unknown> {
  return ask("hints.dismiss", { hint: "first_ticket" });
}

/**
 * Create & start on the first ticket: macOS's notifications question comes up first while it was never asked, with
 * Otaq's one line on why (so it isn't missed once the agent runs); then the ticket is created and started. The card
 * is answered once the ticket exists, even when its start was refused (it waits in Backlog, saying why).
 */
export async function startFirstTicket(
  project: string,
  title: string,
  body: string,
  ask: Ask = request,
  line: Store<NotificationsLine | null> = notificationsLine,
): Promise<EditorSaved> {
  const asked = (await ask("notices.first_start").catch(() => null)) as NotificationsLine | null;
  if (asked !== null) line.set(asked);
  const saved = await createAndStart(
    { mode: "create", project },
    { title: title.trim(), body: body.trim(), priority: null, base_branch: null, choices: [], waits_for: [] },
    ask,
  );
  if (saved.outcome === "saved" || (saved.outcome === "refused" && saved.created !== null)) {
    await answerFirstTicket(ask).catch(() => undefined);
  }
  return saved;
}

/** The element a board hint points at: a card by its key, Edit flow at the end of the columns, or the box for new
 * work (docked over the board, outside it). */
export function hintTarget(
  anchor: BoardHintAnchor,
  root: ParentNode = document,
  page: ParentNode = document,
): HTMLElement | null {
  switch (anchor.kind) {
    case "edit_flow":
      return root.querySelector<HTMLElement>(".board-edit-flow");
    case "new_work":
      return page.querySelector<HTMLElement>(".new-work-closed");
    case "card":
      return root.querySelector<HTMLElement>(`[data-card="${CSS.escape(anchor.key)}"]`);
  }
}

/** Where a hint's button goes, besides dismissing it. */
export interface HintRoutes {
  openTicket: (key: string) => void;
  editFlow: (project: string) => void;
}

export const HINT_ROUTES: HintRoutes = {
  openTicket: (key) => {
    openTicket(key);
  },
  // The editor is the board itself: an open ticket closes so it doesn't stay over it.
  editFlow: (project) => {
    closeTicket();
    openFlowBoard({ project });
  },
};

/** The hint's button: it is dismissed for good, then Open <key> opens the ticket and Edit flow opens the editor. */
export function pressHint(
  hint: BoardHint,
  project: string,
  ask: Ask = request,
  routes: HintRoutes = HINT_ROUTES,
): Promise<unknown> {
  const dismissed = ask("hints.dismiss", { hint: hint.hint.id });
  switch (hint.action.kind) {
    case "open_ticket":
      routes.openTicket(hint.action.key);
      break;
    case "edit_flow":
      routes.editFlow(project);
      break;
    case "dismiss":
      break;
  }
  return dismissed;
}
