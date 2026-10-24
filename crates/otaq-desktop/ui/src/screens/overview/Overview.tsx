// The sheet's Overview (SHT-03, RPT-02; d0109, d0078, d0061, d0099; T-0465; *25 → Opening a ticket* `s25-03`,
// *Editing the description* `s25-04`, *Attachments* `s25-05`). On the left, top to bottom: what the ticket needs from
// you (the question with its options or the action bar, the callouts, "Manual checks for you" above Approve at
// Review, d0125; `NeedsYou`; USG-07's "Mark as usage
// limit" while the agent is Idle, the Inbox's row action before T-0459), the description in place
// (`DescriptionSection`; ⋯ Edit ticket… swaps in the whole form, `EditForm`), the agent's recorded decisions, the
// DEPENDENCIES with Blocked by and Blocks (`screens/dependencies`, DPV-04), an epic's TICKETS grid
// (SHT-07, `EpicGrid`; its lead's question is `NeedsYou`'s), then the comments and their box. On the right, 300 px, the Attachments column: every
// report of the ticket with its size, who added it and in which stage, and its comment's first line; a row opens the
// report view. Add (the system picker), a file dropped anywhere on Overview, or a file pasted outside a text field
// opens the comment box with it as a chip. Its words are Rust's (`overview.view`, `editor.open`).
import { type ClipboardEvent, useCallback, useEffect, useRef, useState } from "react";

import { useRegistries } from "../../app/registry";
import type { TicketProps } from "../../app/registry";
import { PANEL_SECTIONS } from "../../app/registry/panel";
import { ticketEditRequests } from "../../app/selection";
import { useStore } from "../../app/store";
import { Button } from "../../components";
import type { AttachmentRow } from "../../generated/AttachmentRow";
import type { EditorView } from "../../generated/EditorView";
import type { OverviewFileChip } from "../../generated/OverviewFileChip";
import type { OverviewView } from "../../generated/OverviewView";
import { asRequestError, useServiceView } from "../../service";
import { pickReportFile, request } from "../../tauri";
import { dropTargets } from "../../terminal/drops";
import { Icon } from "../../theme/Icon";
import { attachReport, COMMENT_EVENTS, CommentThread, Mark } from "../editor/CommentThread";
import { EditForm, takeEditRequest } from "../editor/EditForm";
import { EDITOR_EVENTS } from "../editor/logic";
import { openReport } from "../editor/reportOpener";
import { ManualChecks } from "./ManualChecks";
import { NeedsYou } from "./NeedsYou";
import { TicketDecisions } from "../sections/Decisions";
import { EpicGrid, useEpicOverview } from "../epic/EpicSheet";
import { useSheet } from "../sheet/Sheet";
import { DescriptionSection, SavedNote } from "./Description";

/** Whether a paste lands in something that takes text itself (a field, the description editor). */
function inTextField(target: EventTarget): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.closest("input, textarea, [contenteditable='true']") !== null)
  );
}

/** One report: "Report 3 KB · CX Rust developer · Dev", its comment's line; a click opens it (CMT-12). */
function Attachment({ ticket, row }: { ticket: string; row: AttachmentRow }) {
  return (
    <button
      type="button"
      className="overview-attachment"
      data-mine={row.mine || undefined}
      onClick={() => {
        openReport({ kind: "comment", ticket, n: row.n });
      }}
    >
      <span className="overview-attachment-icon">
        <Icon name="file-text" size={15} />
      </span>
      <span className="overview-attachment-text">
        <span className="overview-attachment-name">{row.name}</span>
        <span className="overview-attachment-meta">
          <span>{row.size}</span>
          <span aria-hidden>·</span>
          <Mark row={row} />
          <span className="overview-attachment-who">{row.who}</span>
        </span>
        {row.line !== "" && <span className="overview-attachment-line">{row.line}</span>}
      </span>
    </button>
  );
}

/** The Attachments column (RPT-02); the gallery draws it too. */
export function Attachments({
  ticket,
  view,
  error,
  onAdd,
}: {
  ticket: string;
  view: OverviewView;
  error: string | null;
  onAdd: () => void;
}) {
  return (
    <aside className="overview-attachments" aria-label={view.attachments_heading}>
      <div className="overview-label">
        <span className="editor-section-name">{view.attachments_heading.toUpperCase()}</span>
        <span className="editor-spacer" />
        <button type="button" className="overview-add" onClick={onAdd}>
          <Icon name="plus" size={12} />
          {view.add}
        </button>
      </div>
      {view.attachments.map((row) => (
        <Attachment key={row.n} ticket={ticket} row={row} />
      ))}
      <div className="overview-drop">
        <Icon name="upload" size={13} />
        <span>{view.drop}</span>
      </div>
      {error !== null && (
        <p className="comment-box-error" role="alert">
          {error}
        </p>
      )}
      <p className="overview-note">{view.note}</p>
    </aside>
  );
}

export function OverviewTab({ ticket }: TicketProps) {
  const sheet = useSheet();
  const app = useRegistries();
  const loadOverview = useCallback(
    () => request("overview.view", { key: ticket }) as Promise<OverviewView>,
    [ticket],
  );
  const { view: overview } = useServiceView(loadOverview, COMMENT_EVENTS);
  const loadEditor = useCallback(
    () => request("editor.open", { mode: "edit", key: ticket }) as Promise<EditorView>,
    [ticket],
  );
  const { view: editor } = useServiceView(loadEditor, EDITOR_EVENTS);
  const [error, setError] = useState<string | null>(null);

  // ⋯ Edit ticket… opens the whole form in place of the description.
  const [form, setForm] = useState<string | null>(null);
  const [formNote, setFormNote] = useState<string | null>(null);
  // Another ticket: the last one's note and error go.
  const [shownFor, setShownFor] = useState(ticket);
  if (shownFor !== ticket) {
    setShownFor(ticket);
    setFormNote(null);
    setError(null);
  }
  const pending = useStore(ticketEditRequests);
  useEffect(() => {
    if (pending?.ticket !== ticket || !takeEditRequest(ticket)) return;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the Edit click arrives from outside React (a store), once
    setForm(ticket);
    setFormNote(null);
  }, [pending, ticket]);

  // A file for the comment box: its chip, or why it can't be a report.
  const attachPath = useCallback(
    (path: string) => {
      setError(null);
      (request("overview.file", { path }) as Promise<OverviewFileChip>).then(
        (file) => {
          attachReport(ticket, { kind: "file", file });
        },
        (e: unknown) => {
          setError(asRequestError(e).message);
        },
      );
    },
    [ticket],
  );
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    return dropTargets.register(el, {
      takesTyping: () => false,
      paste: () => undefined,
      takeFiles: (paths) => {
        if (paths[0] !== undefined) attachPath(paths[0]);
      },
    });
  }, [attachPath]);
  const onPaste = (e: ClipboardEvent<HTMLDivElement>) => {
    const file = (e.clipboardData.files as FileList | undefined)?.[0];
    if (file === undefined || inTextField(e.target)) return;
    e.preventDefault();
    setError(null);
    file.text().then(
      (text) => {
        attachReport(ticket, { kind: "pasted", text });
      },
      () => undefined,
    );
  };

  const overviewTabs = sheet?.view.sheet.tabs.find((t) => t.tab === "overview")?.contents ?? [];
  const epic = useEpicOverview(ticket, sheet?.view.epic ?? false);
  const editing = form === ticket && editor?.key === ticket;
  return (
    // The paste of a file outside a text field is Overview's own (RPT-02); every other key stays with the page.
    <div ref={root} className="overview" onPaste={onPaste}>
      <div className="overview-main">
        {sheet && (
          <NeedsYou
            view={sheet.view}
            actions={sheet.actions}
            aboveBar={
              overview?.key === ticket &&
              overview.manual_checks !== null && (
                <ManualChecks ticket={ticket} view={overview.manual_checks} />
              )
            }
          />
        )}
        {overview?.key === ticket && overview.usage_limit !== null && (
          <div className="overview-usage-limit">
            <Button
              variant="secondary"
              icon={overview.usage_limit.icon}
              label={overview.usage_limit.label}
              onClick={() => {
                request("overview.usage_limit", { key: ticket }).catch((e: unknown) => {
                  setError(asRequestError(e).message);
                });
              }}
            />
          </div>
        )}
        {editing ? (
          <div className="overview-form">
            <EditForm
              key={ticket}
              view={editor}
              onDone={(note) => {
                setFormNote(note);
                setForm(null);
              }}
            />
          </div>
        ) : (
          editor?.key === ticket &&
          overview?.key === ticket && (
            <>
              {formNote !== null && (
                <SavedNote ticket={ticket} note={formNote} openTerminal={editor.texts.open_terminal} />
              )}
              <DescriptionSection
                ticket={ticket}
                view={editor}
                overview={overview}
                overviewTabs={overviewTabs}
              />
            </>
          )
        )}
        <TicketDecisions ticket={ticket} />
        {epic && <EpicGrid grid={epic.grid} />}
        {PANEL_SECTIONS.map((id) => {
          const Section = app.panelSections.get(id)?.Component;
          return Section ? <Section key={id} ticket={ticket} /> : null;
        })}
        <CommentThread ticket={ticket} />
      </div>
      {overview?.key === ticket && (
        <Attachments
          ticket={ticket}
          view={epic ? { ...overview, note: epic.attachments_note } : overview}
          error={error}
          onAdd={() => {
            pickReportFile(overview.pick_title).then(
              (path) => {
                if (path !== null) attachPath(path);
              },
              () => undefined,
            );
          }}
        />
      )}
    </div>
  );
}
