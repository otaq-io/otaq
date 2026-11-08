// A new user's first run on the board. "Your first ticket" in Backlog: prefilled with a tiny, safe task, its title and
// description editable in place, the tip, the line on time and whose plan it uses, then Create & start (macOS's
// notifications question with Otaq's line on why, then the ticket created and started) and Not now (an empty Backlog
// with its plain Add ticket row). The one first-run hint whose moment is on the board, next to the card or Edit flow
// it explains (the first question, the first permission prompt, the first checker fail, the first review, and "That
// was your flow" after the first approve). After the first approve, the quiet usage-data banner. Every word and every
// rule of when something shows is Rust's (`board.view`'s `first_ticket` and `hint`, `usage.dialog`).
import { useCallback, useEffect, useMemo, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { useShortcut } from "../../app/registry";
import { useStore } from "../../app/store";
import { Banner, Button, HintPopover, TextArea, TextInput } from "../../components";
import type { BoardHint } from "../../generated/BoardHint";
import type { FirstTicketView } from "../../generated/FirstTicketView";
import type { UsageDataAsk } from "../../generated/UsageDataAsk";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import {
  answerFirstTicket,
  dismissHint,
  hintTarget,
  notificationsLine,
  pressHint,
  startFirstTicket,
} from "./first_run";

export function FirstTicketCard({
  view,
  project,
  onAnswered,
}: {
  view: FirstTicketView;
  project: string;
  /** The card was answered: the board asks for its view again. */
  onAnswered: () => void;
}) {
  const [title, setTitle] = useState(view.title);
  const [body, setBody] = useState(view.body);
  const [starting, setStarting] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const start = () => {
    if (title.trim() === "") {
      setNote(view.no_title);
      return;
    }
    setStarting(true);
    setNote(null);
    startFirstTicket(project, title, body).then(
      (saved) => {
        setStarting(false);
        if (saved.outcome === "needs_identity") {
          // Nothing was created: git's name and email first, then Create & start again; the card stays meanwhile.
          openOverlay("git_identity", {
            form: saved.form,
            save: saved.save,
            cancel: saved.cancel,
            then: start,
          });
          return;
        }
        if (saved.outcome === "refused") setNote(saved.refusal.message);
        if (saved.outcome === "title_missing") setNote(view.no_title);
        onAnswered();
      },
      (error: unknown) => {
        setStarting(false);
        setNote(asRequestError(error).message);
      },
    );
  };
  return (
    <section className="first-ticket" aria-label={view.label}>
      <div className="first-ticket-head">
        <span className="first-ticket-label">{view.label}</span>
        <span className="first-ticket-status">{view.status}</span>
      </div>
      <TextInput value={title} onChange={setTitle} label={view.title_label} />
      <TextArea value={body} onChange={setBody} label={view.body_label} rows={2} grow />
      <p className="first-ticket-tip">{view.tip}</p>
      <p className="first-ticket-cost">{view.cost}</p>
      {note !== null && (
        <p className="first-ticket-note" role="alert">
          {note}
        </p>
      )}
      <div className="first-ticket-buttons">
        <Button label={starting ? view.starting : view.start} loading={starting} onClick={start} />
        <Button
          label={view.not_now}
          variant="ghost"
          disabled={starting}
          onClick={() => {
            answerFirstTicket().then(onAnswered, () => undefined);
          }}
        />
      </div>
    </section>
  );
}

/** The first-run hint next to what it explains, while that is on screen (it follows it, and waits while it is
 * away: a card folded away, the box for new work not docked yet); its button dismisses it for good. One whose button
 * does more ("That was your flow"'s Edit flow) has Got it beside it, and Esc dismisses it too. */
export function BoardHintLayer({
  hint,
  project,
  board,
  onDismissed,
}: {
  hint: BoardHint;
  project: string;
  board: HTMLElement | null;
  onDismissed: () => void;
}) {
  const [gone, setGone] = useState<string | null>(null);
  const key = `${hint.hint.id}:${hint.anchor.kind === "card" ? hint.anchor.key : ""}`;
  // Looked up each time it is placed: the card may move or fold away, the box may dock later.
  const anchor = useMemo(
    () => ({
      get current(): HTMLElement | null {
        return board ? hintTarget(hint.anchor, board) : null;
      },
    }),
    [board, hint],
  );
  const shown = board !== null && gone !== key;
  const quiet = hint.hint.dismiss !== null;
  const dismissOnly = useCallback(() => {
    setGone(key);
    dismissHint(hint).then(onDismissed, () => undefined);
  }, [hint, key, onDismissed]);
  // Esc is the board's Close (no ticket is open while a board hint shows): it dismisses a hint that has Got it.
  useShortcut(
    "close_sheet",
    useCallback(() => {
      if (!shown || !quiet) return false;
      dismissOnly();
      return true;
    }, [shown, quiet, dismissOnly]),
  );
  if (!shown) return null;
  return (
    <HintPopover
      key={key}
      anchor={anchor}
      title={hint.hint.title}
      text={hint.hint.text}
      button={hint.hint.button}
      dismiss={hint.hint.dismiss}
      onDismiss={() => {
        setGone(key);
        pressHint(hint, project).then(onDismissed, () => undefined);
      }}
      onDismissOnly={dismissOnly}
    />
  );
}

/** Otaq's line beside macOS's notifications question on the first Create & start, at the window's top right. */
export function NotificationsNote() {
  const line = useStore(notificationsLine);
  if (line === null) return null;
  const corner = { left: window.innerWidth - 24, top: 48, width: 0, height: 0 };
  return (
    <HintPopover
      anchor={corner}
      align="end"
      title={line.title}
      text={line.text}
      button={line.button}
      onDismiss={() => {
        notificationsLine.set(null);
      }}
    />
  );
}

/** "Help improve Otaq?": asked once, after the first approve; `seen` changes whenever the board's view does. */
export function UsageBanner({ seen }: { seen: unknown }) {
  const [ask, setAsk] = useState<UsageDataAsk | null>(null);
  useEffect(() => {
    let live = true;
    request("usage.dialog").then(
      (answer) => {
        if (live) setAsk(answer as UsageDataAsk);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [seen]);
  if (!ask?.show) return null;
  const choose = (on: boolean) => {
    setAsk(null);
    request("usage.choose", { on }).catch(() => undefined);
  };
  return (
    <Banner
      tone="info"
      icon="info"
      title={ask.banner.title}
      text={ask.banner.text}
      actions={[
        {
          label: ask.banner.decline_label,
          onClick: () => {
            choose(false);
          },
        },
        {
          label: ask.banner.accept_label,
          primary: true,
          onClick: () => {
            choose(true);
          },
        },
      ]}
    />
  );
}
