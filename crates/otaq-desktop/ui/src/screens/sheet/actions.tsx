// The ticket's actions (UI-04, T-0222; the sheet of SHT-01, T-0464): what its buttons, its "…" menu (`MoreMenu`) and
// a board card's button do (`usePanelActions`), held by the sheet and handed to Overview's bar (`overview/NeedsYou`).
// Everything it says comes from `panel.view`; it keeps only the open menu and the request on its way. A board card's
// button lands here too (`cardActions`): the sheet runs it once it shows that ticket.
import { useCallback, useEffect, useEffectEvent, useLayoutEffect, useRef, useState } from "react";

import { cardActions, takeCardAction } from "../../app/cardActions";
import { openOverlay } from "../../app/overlays";
import { useShortcut } from "../../app/registry";
import { openTicket, requestTerminalFocus, requestTicketEdit, showTab } from "../../app/selection";
import { useStore } from "../../app/store";
import { AnchoredMenu, IconButton } from "../../components";
import type { PanelButton } from "../../generated/PanelButton";
import type { PanelCardParams } from "../../generated/PanelCardParams";
import type { PanelSendAgainParams } from "../../generated/PanelSendAgainParams";
import type { PanelSent } from "../../generated/PanelSent";
import type { PanelView } from "../../generated/PanelView";
import { request } from "../../tauri";
import { startTicket } from "../board/Board";
import { passTicket } from "../review/CheckDialogs";
import { startRun } from "../session/logic";
import { actionRoute, agentSettings, cardRoute, CLOSE_TICKET, menuRoute } from "./actionRoutes";

/** The service events that change what the sheet shows. */
export const PANEL_EVENTS = [
  "ticket_changed",
  "ticket_deleted",
  "session_state",
  "inbox_changed",
  "usage_changed",
  "validation_cancelled",
  "lead_changed",
  "project_agent_changed",
  // The user's keys in its tooltips (KEY-08).
  "settings_changed",
] as const;

const ignore = () => undefined;

function copy(text: string) {
  navigator.clipboard.writeText(text).catch(ignore);
}

/** The unsent-text question before a send clears the terminal's typed line (SES-07, d0048). */
export function askUnsent(sent: PanelSent, again: () => Promise<unknown>) {
  if (sent.outcome !== "ask") return;
  openOverlay("confirm", {
    title: sent.title,
    body: sent.body,
    confirmLabel: sent.clear,
    cancelLabel: sent.cancel,
    destructive: false,
    onConfirm: again,
  });
}

/** The "…" menu (the sheet's header): the ticket's menu of its state, Edit ticket… first. */
export function MoreMenu({ view, onChoose }: { view: PanelView; onChoose: (item: string) => void }) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  return (
    <>
      <IconButton
        ref={anchor}
        icon="ellipsis"
        label={view.more_hint}
        shown={open ? "pressed" : undefined}
        onClick={() => {
          setOpen(!open);
        }}
      />
      {open && (
        <AnchoredMenu
          anchor={anchor}
          align="end"
          label={view.more_hint}
          items={view.menu}
          width={280}
          onChoose={(item) => {
            setOpen(false);
            onChoose(item);
          }}
          onClose={() => {
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

/** A card's answer button handed to the answer list of `question`: an option to pick, or Send answer. */
export type CardAnswer = { question: string } & ({ kind: "pick"; number: number } | { kind: "send_answer" });

/** What the ticket's buttons, its "…" menu and a board card's button do, shared by the sheet's header and Overview. */
export interface PanelActions {
  press: (button: PanelButton) => void;
  choose: (item: string) => void;
  /** The button whose request is on its way. */
  busy: string | null;
  /** The ticket whose Answer the agent box is open. */
  answering: string | null;
  setAnswering: (key: string | null) => void;
  /** A card's answer button, handed to the answer list. */
  cardAnswer: CardAnswer | null;
}

/**
 * The ticket's actions (T-0222), held by the sheet while it shows `view`: its buttons and "…" menu, Close ticket and
 * Re-run validation as keys while it offers them (0097: palette only unless the user gives them a key), and a board
 * card's button for this ticket (T-0220), run once the sheet shows it.
 */
export function usePanelActions(view: PanelView): PanelActions {
  const [busy, setBusy] = useState<string | null>(null);
  const [answering, setAnswering] = useState<string | null>(null);
  const askMarkClosed = () => {
    if (view.mark_closed === null) return;
    openOverlay("confirm", {
      title: view.mark_closed.title,
      body: view.mark_closed.body,
      confirmLabel: view.mark_closed.confirm,
      destructive: true,
      onConfirm: () => request("panel.mark_closed", { key: view.key }),
    });
  };
  const press = (button: PanelButton) => {
    const route = actionRoute(button.action);
    switch (route.kind) {
      case "service":
        setBusy(button.action);
        request("panel.action", { key: view.key, action: button.action })
          .catch(ignore)
          .finally(() => {
            setBusy(null);
          });
        return;
      case "start":
        setBusy(button.action);
        void startTicket(view.key).finally(() => {
          setBusy(null);
        });
        return;
      case "terminal":
        showTab("agent");
        requestTerminalFocus(view.key);
        return;
      case "send_again": {
        const send = (clear: boolean): Promise<unknown> =>
          (
            request("panel.send_again", {
              key: view.key,
              clear,
            } satisfies PanelSendAgainParams) as Promise<PanelSent>
          )
            .then((sent) => {
              askUnsent(sent, () => send(true));
            })
            .catch(ignore);
        setBusy(button.action);
        void send(false).finally(() => {
          setBusy(null);
        });
        return;
      }
      case "check":
        showTab("check");
        requestTerminalFocus(view.key);
        return;
      case "answer_agent":
        setAnswering(view.key);
        return;
      case "diff":
        showTab("files");
        return;
      case "edit":
        requestTicketEdit(view.key);
        return;
      case "parent":
        if (view.parent !== null) openTicket(view.parent);
        return;
      case "overlay":
        // With open recorded decisions, "Approve · keeps N decisions" opens the choice: Local merge / Open PR (DEC-05).
        openOverlay(
          route.id,
          button.action === "approve" && view.approve_opens_menu
            ? { ticket: view.key, menu: true }
            : { ticket: view.key },
        );
        return;
      case "pass":
        setBusy(button.action);
        void passTicket(view.key, view.approve_opens_menu).finally(() => {
          setBusy(null);
        });
        return;
      case "mark_closed":
        askMarkClosed();
        return;
      // Run (RUN-01): Session's Run shows while it starts, or the run-command dialog asks first (T-0232).
      case "run":
        showTab("run");
        void startRun(view.key);
        return;
      case "open_agent":
        openOverlay("settings", agentSettings(view.open_agent));
        return;
    }
  };
  const choose = (item: string) => {
    const route = menuRoute(item);
    switch (route.kind) {
      case "edit":
        requestTicketEdit(view.key);
        return;
      case "service":
        request("panel.menu", { key: view.key, item }).catch(ignore);
        return;
      case "copy":
        copy(route.what === "branch" ? (view.branch?.full ?? "") : view.attach_command);
        return;
      case "overlay":
        openOverlay(route.id, { ticket: view.key });
        return;
      case "close_epic":
        openOverlay("close_epic", { epic: view.key });
        return;
      case "unknown":
        return;
    }
  };
  const latest = useRef({ view, press, choose });
  useLayoutEffect(() => {
    latest.current = { view, press, choose };
  });
  useShortcut(
    "close_ticket",
    useCallback(() => {
      const item = latest.current.view.menu.find((i) => i.id === CLOSE_TICKET && !i.disabled);
      if (!item) return false;
      latest.current.choose(item.id);
      return true;
    }, []),
  );
  useShortcut(
    "rerun_validation",
    useCallback(() => {
      const button = latest.current.view.buttons.find((b) => b.action === "retry_validation");
      if (!button) return false;
      latest.current.press(button);
      return true;
    }, []),
  );
  const [cardAnswer, setCardAnswer] = useState<CardAnswer | null>(null);
  const pendingCard = useStore(cardActions);
  const runCard = useEffectEvent((kind: PanelCardParams["kind"]) => {
    const route = cardRoute(kind);
    switch (route.kind) {
      case "service": {
        const send = (clear: boolean): Promise<unknown> =>
          (
            request("panel.card", {
              key: view.key,
              kind,
              clear,
            } satisfies PanelCardParams) as Promise<PanelSent>
          )
            .then((sent) => {
              askUnsent(sent, () => send(true));
            })
            .catch(ignore);
        void send(false);
        return;
      }
      case "pick":
        if (view.answer)
          setCardAnswer({ question: view.answer.question, kind: "pick", number: route.number });
        return;
      case "send_answer":
        if (view.answer) setCardAnswer({ question: view.answer.question, kind: "send_answer" });
        return;
      case "overlay":
        openOverlay(route.id, { ticket: view.key });
        return;
      case "pass":
        void passTicket(view.key, view.approve_opens_menu);
        return;
      case "mark_closed":
        askMarkClosed();
        return;
      case "run_tab":
        showTab("run");
        return;
      case "open_agent":
        openOverlay("settings", agentSettings(view.open_agent));
        return;
      case "none":
        return;
    }
  });
  useEffect(() => {
    if (pendingCard?.ticket !== view.key) return;
    const taken = takeCardAction(view.key);
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the card's click arrives from outside React (a store), once
    if (taken) runCard(taken.kind);
  }, [pendingCard, view.key]);
  return { press, choose, busy, answering, setAnswering, cardAnswer };
}
