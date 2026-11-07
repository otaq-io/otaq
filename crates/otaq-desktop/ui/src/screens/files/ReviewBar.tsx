// The review bar (SHT-06, T-0468; `s25-09`): under every tab of the sheet while the ticket waits in your check. At the
// review that ends the flow: Send back… ("Goes back to Dev with your notes.") on the left, the change's size and
// Approve named by what it does ("Approve & merge into main") on the right, with the other ways to finish in the menu
// beside it. With line comments, "1 line comment, 1 being written · they go to Dev with Send back" with Send back with
// comments. Its buttons open today's dialogs (`send_back`, the Approve choice's `local_merge` at that choice), or in
// another flow Approve is the human check's Pass (`review.pass`, d0093). What it says is Rust's (`files.bar`).
import { useCallback, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { useStore } from "../../app/store";
import { AnchoredMenu, Button, Kbd } from "../../components";
import type { ApproveChoice } from "../../generated/ApproveChoice";
import type { FilesApproveSplit } from "../../generated/FilesApproveSplit";
import type { FilesBarButton } from "../../generated/FilesBarButton";
import type { FilesBarParams } from "../../generated/FilesBarParams";
import type { FilesReviewBar } from "../../generated/FilesReviewBar";
import type { SheetTab } from "../../generated/SheetTab";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { passTicket } from "../review/CheckDialogs";
import { FILES_EVENTS } from "./FilesTab";
import { writing } from "./logic";

/** What `files.bar` is asked: the tab only when the bar sits under one. */
export function barParams(ticket: string, writes: boolean, tab: SheetTab | undefined): FilesBarParams {
  return tab === undefined ? { key: ticket, writing: writes } : { key: ticket, writing: writes, tab };
}

/** What a bar button opens. */
export function pressBar(ticket: string, button: FilesBarButton): void {
  switch (button.action) {
    case "approve":
      openOverlay("local_merge", { ticket, menu: true });
      return;
    case "pass":
      void passTicket(ticket, true);
      return;
    case "send_back":
      openOverlay("send_back", { ticket });
      return;
  }
}

/** Approve or a row of its menu: that choice's dialog, straight away. */
export function pressChoice(ticket: string, choice: ApproveChoice): void {
  openOverlay("local_merge", { ticket, choice });
}

/** Approve named by its action, and the button beside it that opens the other ways to finish. */
function ApproveButton({ ticket, approve }: { ticket: string; approve: FilesApproveSplit }) {
  const more = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  return (
    <span className="files-bar-approve">
      <Button
        compact
        icon={approve.choice === "open_pr" ? "git-pull-request" : "check"}
        label={approve.label}
        onClick={() => {
          pressChoice(ticket, approve.choice);
        }}
      />
      {approve.menu.length > 0 && (
        <button
          ref={more}
          type="button"
          className="files-bar-more"
          aria-label={approve.menu_label}
          aria-expanded={open}
          onClick={() => {
            setOpen((o) => !o);
          }}
        >
          <Icon name="chevron-down" size={13} />
        </button>
      )}
      {open && (
        <AnchoredMenu
          anchor={more}
          align="end"
          label={approve.menu_label}
          items={approve.menu}
          width={340}
          onChoose={(id) => {
            setOpen(false);
            pressChoice(ticket, id as ApproveChoice);
          }}
          onClose={(how) => {
            setOpen(false);
            if (how === "key") more.current?.focus();
          }}
        />
      )}
    </span>
  );
}

/** The bar for a loaded view (also what the markup tests render). */
export function ReviewBarBody({ ticket, bar }: { ticket: string; bar: FilesReviewBar }) {
  const approve = bar.approve;
  return (
    <div
      className="files-bar"
      role="region"
      aria-label={bar.title === "" ? (approve?.label ?? "") : bar.title}
    >
      {bar.title !== "" && (
        <>
          <Icon name="eye" size={14} />
          <span className="files-bar-title">{bar.title}</span>
          <span className="files-bar-line">{bar.line}</span>
          <span className="files-spacer" />
        </>
      )}
      {bar.buttons.map((button) => (
        <span key={button.action} className="files-bar-button">
          <Button
            compact
            variant={button.primary ? "primary" : "secondary"}
            icon={button.action === "send_back" ? "corner-up-left" : "check"}
            label={button.label}
            onClick={() => {
              pressBar(ticket, button);
            }}
          />
          {button.keys !== null && <Kbd keys={button.keys} />}
        </span>
      ))}
      {bar.send_back_note !== undefined && <span className="files-bar-line">{bar.send_back_note}</span>}
      {bar.title === "" && <span className="files-spacer" />}
      {bar.changes !== undefined && <span className="files-bar-line">{bar.changes}</span>}
      {approve !== undefined && <ApproveButton ticket={ticket} approve={approve} />}
    </div>
  );
}

/**
 * The bar under the sheet's tabs: nothing unless the ticket waits in your check. `tab` is the tab it sits under: at a
 * check of planning its words point at where each part is read (`s30-02`).
 */
export function ReviewBar({ ticket, tab }: { ticket: string; tab?: SheetTab }) {
  const writes = useStore(writing) === ticket;
  const load = useCallback(
    () => request("files.bar", barParams(ticket, writes, tab)) as Promise<FilesReviewBar | null>,
    [ticket, writes, tab],
  );
  const { view: bar } = useServiceView(load, FILES_EVENTS);
  if (!bar) return null;
  return <ReviewBarBody ticket={ticket} bar={bar} />;
}
