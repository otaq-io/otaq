// The box for new work (*New ticket · one box for new work*): the `new_work` slot, docked at the bottom
// centre of the board and the List, "New ticket, or ask Noyan…" with ⌘N. A click, ⌘N or the toolbar's New ticket opens
// it: it grows upward over a light scrim, the title line on top and "Add details (optional)" under it. ↵ Create, ⌘↵
// Create & start, ⇧↵ Ask Noyan to shape it, ⌥↵ a new line (from the title line, into the details); More options…
// opens the full form with what was typed, and the box keeps it until the form creates the ticket (a Cancel hands it
// back). Esc or a click outside closes it and keeps what was typed. It isn't there in Files or the project's Settings,
// while Noyan's panel is open, or when `new_work.view` says it isn't shown.
import { type RefObject, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import { shellLayout } from "../../app/layout";
import { openOverlay } from "../../app/overlays";
import { useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { Button, Kbd, OneTimeHint, PLAIN_TEXT } from "../../components";
import type { NewWorkView } from "../../generated/NewWorkView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import {
  askNoyan,
  boxKey,
  boxOnScreen,
  boxTakenByForm,
  createTicket,
  EMPTY,
  boardAway,
  formOf,
  hasWords,
  intoTitle,
  openRequests,
  type Typed,
} from "./logic";

/** The service events that change what the box says (the user's ⌘N) or whether it shows (it appears once the first
 * ticket was approved, a ticket change). */
export const NEW_WORK_EVENTS = ["settings_changed", "ticket_changed"] as const;

const ignore = () => undefined;

/** Which action is on its way, for its button's loader. */
export type Sending = "create" | "start" | "ask" | null;

export interface NewWorkBodyProps {
  view: NewWorkView;
  typed: Typed;
  open: boolean;
  /** What was refused, under the fields. */
  note?: string | null;
  sending?: Sending;
  /** Draws the scrim over the window while open, and takes the keyboard (the gallery draws the box alone). */
  scrim?: boolean;
  /** Changes with each ask to open the box: the title line takes the keyboard again. */
  focusKey?: number;
  /** The closed box's element, for a hint next to it. */
  closedRef?: RefObject<HTMLButtonElement | null>;
  onTyped?: (typed: Typed) => void;
  onOpen?: () => void;
  onClose?: () => void;
  onCreate?: () => void;
  onStart?: () => void;
  onAsk?: () => void;
  onMore?: () => void;
}

/** The box for a loaded view (also what the gallery draws). */
export function NewWorkBody({
  view,
  typed,
  open,
  note = null,
  sending = null,
  scrim = true,
  focusKey = 0,
  closedRef,
  onTyped,
  onOpen,
  onClose,
  onCreate,
  onStart,
  onAsk,
  onMore,
}: NewWorkBodyProps) {
  const title = useRef<HTMLInputElement>(null);
  const details = useRef<HTMLTextAreaElement>(null);
  const [inDetails, setInDetails] = useState(false);
  // The details grow with their text, up to their CSS max-height.
  useLayoutEffect(() => {
    const el = details.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${String(el.scrollHeight)}px`;
  }, [typed.details, open]);
  // Opened (or asked again): the keyboard goes to the title line.
  useEffect(() => {
    if (open && scrim) title.current?.focus();
  }, [open, scrim, focusKey]);

  const onKey = (
    e: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>,
    line: "title" | "details",
  ) => {
    const what = boxKey(e.nativeEvent);
    if (what === null) {
      // Backspace at the start of empty details goes back to the end of the title line.
      if (line === "details" && e.key === "Backspace" && typed.details === "") {
        e.preventDefault();
        const field = title.current;
        field?.focus();
        field?.setSelectionRange(field.value.length, field.value.length);
      }
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    if (sending !== null) return;
    if (what === "leave") onClose?.();
    else if (what === "create") onCreate?.();
    else if (what === "start") onStart?.();
    else if (what === "ask") onAsk?.();
    else if (line === "title") {
      // A new line from the title line: the details take the keyboard at their start.
      details.current?.focus();
      details.current?.setSelectionRange(0, 0);
    } else {
      e.currentTarget.setRangeText(
        "\n",
        e.currentTarget.selectionStart ?? 0,
        e.currentTarget.selectionEnd ?? 0,
        "end",
      );
      onTyped?.({ ...typed, details: e.currentTarget.value });
    }
  };

  if (!open)
    return (
      <button ref={closedRef} type="button" className="new-work-closed" onClick={onOpen}>
        <Icon name="plus" size={15} />
        <span className="new-work-closed-text" data-typed={typed.title !== "" || undefined}>
          {typed.title === "" ? view.placeholder : typed.title}
        </span>
        {view.keys !== null && <Kbd keys={view.keys} />}
      </button>
    );

  const words = hasWords(typed);
  const titled = formOf(typed).title !== "";
  return (
    <>
      {scrim && (
        <div
          className="new-work-scrim"
          role="presentation"
          onMouseDown={(e) => {
            e.preventDefault();
            onClose?.();
          }}
        />
      )}
      <div className="new-work-open" role="dialog" aria-label={view.placeholder}>
        <div className="new-work-fields">
          <input
            {...PLAIN_TEXT}
            ref={title}
            className="new-work-title"
            aria-label={view.placeholder}
            placeholder={view.placeholder}
            value={typed.title}
            onChange={(e) => {
              onTyped?.({ ...typed, title: e.target.value });
            }}
            onPaste={(e) => {
              const text = e.clipboardData.getData("text/plain");
              if (!text.includes("\n")) return;
              e.preventDefault();
              const el = e.currentTarget;
              const before = el.value.slice(0, el.selectionStart ?? el.value.length);
              const after = el.value.slice(el.selectionEnd ?? el.value.length);
              onTyped?.(intoTitle(typed, `${before}${text}${after}`));
            }}
            onKeyDown={(e) => {
              onKey(e, "title");
            }}
          />
          <textarea
            {...PLAIN_TEXT}
            ref={details}
            className="new-work-details"
            aria-label={view.details}
            placeholder={view.details}
            rows={1}
            value={typed.details}
            onChange={(e) => {
              onTyped?.({ ...typed, details: e.target.value });
            }}
            onFocus={() => {
              setInDetails(true);
            }}
            onBlur={() => {
              setInDetails(false);
            }}
            onKeyDown={(e) => {
              onKey(e, "details");
            }}
          />
          {note !== null && (
            <span className="field-note" data-error>
              {note}
            </span>
          )}
        </div>
        <div className="new-work-actions">
          <button type="button" className="new-work-more" onClick={onMore}>
            {view.more_options}
          </button>
          {inDetails && (
            <span className="new-work-hint">
              <Kbd keys={view.new_line.keys} />
              <span>{view.new_line.label}</span>
            </span>
          )}
          <span className="new-work-spacer" />
          <button
            type="button"
            className="new-work-ask"
            disabled={!words || sending !== null}
            onClick={onAsk}
          >
            <Icon name={sending === "ask" ? "loader-circle" : "bot"} size={14} />
            <span>{view.ask.label}</span>
            <Kbd keys={view.ask.keys} />
          </button>
          <span className="new-work-action">
            <Button
              variant="secondary"
              label={view.create.label}
              loading={sending === "create"}
              disabled={!titled || (sending !== null && sending !== "create")}
              onClick={onCreate}
            />
            <Kbd keys={view.create.keys} />
          </span>
          <span className="new-work-action">
            <Button
              label={view.start.label}
              loading={sending === "start"}
              disabled={!titled || (sending !== null && sending !== "start")}
              onClick={onStart}
            />
            <Kbd keys={view.start.keys} />
          </span>
        </div>
      </div>
    </>
  );
}

/** One project's box: what is typed, the asks to open it, and the three ways out. */
function ProjectNewWork({ project, view }: { project: string; view: NewWorkView }) {
  const [typed, setTyped] = useState<Typed>(EMPTY);
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [sending, setSending] = useState<Sending>(null);
  const asks = useStore(openRequests);
  const seen = useRef(asks);
  useEffect(() => {
    if (asks === seen.current) return;
    seen.current = asks;
    setOpen(true);
  }, [asks]);
  // The full form More options… opened created its ticket: what was typed is in it now.
  const taken = useStore(boxTakenByForm);
  const seenTaken = useRef(taken);
  useEffect(() => {
    if (taken === seenTaken.current) return;
    seenTaken.current = taken;
    setTyped(EMPTY);
  }, [taken]);
  useEffect(() => {
    boxOnScreen.set(true);
    return () => {
      boxOnScreen.set(false);
    };
  }, []);
  // The box's one-time hint, the first time it appears (after the first approve), next to the closed box.
  const closed = useRef<HTMLButtonElement | null>(null);

  const close = () => {
    setOpen(false);
    setNote(null);
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  };
  const create = (start: boolean) => {
    if (sending !== null || formOf(typed).title === "") return;
    setSending(start ? "start" : "create");
    setNote(null);
    createTicket(project, typed, start)
      .then((out) => {
        if (out.clear) setTyped(EMPTY);
        if (out.note !== null) setNote(out.note);
        else if (out.clear) close();
      }, ignore)
      .finally(() => {
        setSending(null);
      });
  };
  return (
    <>
      <NewWorkBody
        view={view}
        typed={typed}
        open={open}
        closedRef={closed}
        note={note}
        sending={sending}
        focusKey={asks}
        onTyped={(next) => {
          setTyped(next);
          setNote(null);
        }}
        onOpen={() => {
          setOpen(true);
        }}
        onClose={close}
        onCreate={() => {
          create(false);
        }}
        onStart={() => {
          create(true);
        }}
        onAsk={() => {
          if (sending !== null || !hasWords(typed)) return;
          setSending("ask");
          askNoyan(project, typed)
            .then((sent) => {
              if (!sent) return;
              setTyped(EMPTY);
              setOpen(false);
              setNote(null);
            }, ignore)
            .finally(() => {
              setSending(null);
            });
        }}
        onMore={() => {
          const form = formOf(typed);
          // The box keeps what was typed until the form creates the ticket: a Cancel there hands it back.
          close();
          openOverlay("new_ticket", {
            ...(form.title !== "" && { title: form.title }),
            ...(form.body !== "" && { body: form.body }),
            fromBox: true,
          });
        }}
      />
      {!open && <OneTimeHint hint="new_work" anchor={closed} align="end" />}
    </>
  );
}

/** The `new_work` slot: the shown project's box, on its board and List only. */
export function NewWork() {
  const { project } = useSelection();
  const panelOpen = useStore(shellLayout).agentOpen;
  const away = useStore(boardAway);
  const load = useCallback(() => request("new_work.view") as Promise<NewWorkView>, []);
  const { view } = useServiceView(load, NEW_WORK_EVENTS);
  if (project === null || !view?.shown || panelOpen || away) return null;
  return <ProjectNewWork key={project} project={project} view={view} />;
}
