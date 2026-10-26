// The review bar (SHT-06, T-0468; `s25-09`): under every tab of the sheet while the ticket waits in your check. "Your
// check · Security passed · comment on any line to send it back" with Send back… and Approve ⌘↵; with line comments,
// "1 line comment, 1 being written · they go to Dev with Send back" with Approve and Send back with comments. Its
// buttons open today's dialogs (`send_back`, the Approve choice `local_merge`), or in another flow Approve is the
// human check's Pass (`review.pass`, d0093). What it says is Rust's (`files.bar`).
import { useCallback } from "react";

import { openOverlay } from "../../app/overlays";
import { useStore } from "../../app/store";
import { Button, Kbd } from "../../components";
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

/** The bar for a loaded view (also what the markup tests render). */
export function ReviewBarBody({ ticket, bar }: { ticket: string; bar: FilesReviewBar }) {
  return (
    <div className="files-bar" role="region" aria-label={bar.title}>
      <Icon name="eye" size={14} />
      <span className="files-bar-title">{bar.title}</span>
      <span className="files-bar-line">{bar.line}</span>
      <span className="files-spacer" />
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
