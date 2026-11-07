// The sheet's Overview (SHT-03, RPT-02; d0109, d0078, d0061, d0099; T-0465; *25 → Opening a ticket* `s25-03`,
// *Editing the description* `s25-04`, *Attachments* `s25-05`). On the left, top to bottom: what the ticket needs from
// you (the question with its options or the action bar, the callouts, "Manual checks for you" above Approve at
// Review, d0125; `NeedsYou`; USG-07's "Mark as usage
// limit" while the agent is Idle, the Inbox's row action before T-0459), the description in place
// (`DescriptionSection`; ⋯ Edit ticket… swaps in the whole form, `EditForm`), the PLAN under it (`PlanSection`, d0133) and at a check of planning WHAT PLANNING DID (`PlanningDid`, d0129), the agent's recorded decisions, the
// DEPENDENCIES with Waits for and the read-only Waiting for it (`screens/dependencies`, DPV-04), then the comments and their box. On the right, 300 px, STAGES (the
// ticket's choices and path, `ChoicesAndPath`, `sheet.view`'s words), then the Attachments column: every
// report of the ticket with its size, who added it and in which stage, and its comment's first line; a row opens the
// report view. Add (the system picker, several files), files dropped anywhere on Overview, or files pasted outside a
// text field are added to the comment box's chips (tuq 003). Its words are Rust's (`overview.view`, `editor.open`).
import { type ClipboardEvent, useCallback, useEffect, useRef, useState } from "react";

import { useRegistries } from "../../app/registry";
import type { TicketProps } from "../../app/registry";
import { SHEET_SECTIONS } from "../../app/registry/sheet";
import { ticketEditRequests } from "../../app/selection";
import { useStore } from "../../app/store";
import { Button } from "../../components";
import type { AttachmentRow } from "../../generated/AttachmentRow";
import type { EditorView } from "../../generated/EditorView";
import type { OverviewView } from "../../generated/OverviewView";
import type { SheetView } from "../../generated/SheetView";
import { asRequestError, useServiceView } from "../../service";
import { pickAttachmentFiles, request } from "../../tauri";
import { dropTargets } from "../../terminal/drops";
import { Icon } from "../../theme/Icon";
import { type Added, addFiles, addPaths, clipboardFiles } from "../editor/attachmentAdding";
import { attachReport, COMMENT_EVENTS, CommentThread, Mark } from "../editor/CommentThread";
import { EditForm, takeEditRequest } from "../editor/EditForm";
import { AttachmentImage, openImage } from "../editor/ImageViewer";
import { EDITOR_EVENTS } from "../editor/logic";
import { openReport } from "../editor/reportOpener";
import { BeforeYou } from "./BeforeYou";
import { ManualChecks } from "./ManualChecks";
import { ChoicesAndPath, NeedsYou } from "./NeedsYou";
import { PlanSection } from "./Plan";
import { PlanningDid } from "./PlanningDid";
import { TicketDecisions } from "../sections/Decisions";
import { useSheet } from "../sheet/Sheet";
import { DescriptionSection, SavedNote } from "./Description";

/** Whether a paste lands in something that takes text itself (a field, the description editor). */
function inTextField(target: EventTarget): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.closest("input, textarea, [contenteditable='true']") !== null)
  );
}

/** One attachment: "test-run.txt 3 KB · CX Rust developer · Dev", its comment's line; a click opens a text one in
 * the report view (CMT-12) and an image full size, shown by its thumbnail (tuq 004). Other files have no preview in
 * the app (tuq 002): their row is plain, not a button, as the thread's plain chip. */
function Attachment({ ticket, row }: { ticket: string; row: AttachmentRow }) {
  const { thumbnail } = row;
  const contents = <AttachmentContents ticket={ticket} row={row} />;
  if (row.kind !== "text" && thumbnail === null) {
    return (
      <div className="overview-attachment" data-mine={row.mine || undefined} data-kind={row.kind}>
        {contents}
      </div>
    );
  }
  return (
    <button
      type="button"
      className="overview-attachment"
      data-mine={row.mine || undefined}
      data-kind={row.kind}
      onClick={() => {
        if (thumbnail !== null) openImage(ticket, row.n, thumbnail);
        else openReport({ kind: "comment", ticket, n: row.n, attachment: row.attachment });
      }}
    >
      {contents}
    </button>
  );
}

function AttachmentContents({ ticket, row }: { ticket: string; row: AttachmentRow }) {
  const { thumbnail } = row;
  return (
    <>
      <span className="overview-attachment-icon">
        {thumbnail !== null ? (
          <AttachmentImage
            ticket={ticket}
            n={row.n}
            thumbnail={thumbnail}
            className="comment-thumbnail-image"
            decorative
          />
        ) : (
          <Icon name={row.kind === "text" ? "file-text" : "file"} size={15} />
        )}
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
    </>
  );
}

/** STAGES at the top of the right column: the ticket's choices, then its path; nothing without either. */
export function Stages({ view }: { view: SheetView }) {
  if (view.choices === null && view.path === null) return null;
  return (
    <section className="overview-stages" aria-label={view.stages_heading}>
      <div className="overview-label">
        <span className="editor-section-name">{view.stages_heading.toUpperCase()}</span>
      </div>
      <ChoicesAndPath view={view} />
    </section>
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
        <Attachment key={`${String(row.n)}-${String(row.attachment)}`} ticket={ticket} row={row} />
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

  // Files for the comment box, added to what it holds (tuq 003); why any can't be attached shows here.
  const attach = useCallback(
    ({ added, refused }: Added) => {
      setError(refused.length > 0 ? refused.join(" ") : null);
      if (added.length > 0) attachReport(ticket, added);
    },
    [ticket],
  );
  const attachPaths = useCallback(
    (paths: readonly string[]) => {
      if (paths.length > 0) void addPaths(paths).then(attach);
    },
    [attach],
  );
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    return dropTargets.register(el, {
      takesTyping: () => false,
      paste: () => undefined,
      takeFiles: attachPaths,
    });
  }, [attachPaths]);
  const onPaste = (e: ClipboardEvent<HTMLDivElement>) => {
    const files = clipboardFiles(e.clipboardData);
    if (files.length === 0 || inTextField(e.target)) return;
    e.preventDefault();
    void addFiles(files).then(attach);
  };

  const overviewTabs = sheet?.view.sheet.tabs.find((t) => t.tab === "overview")?.contents ?? [];
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
              overview?.key === ticket && (
                <>
                  {overview.before_you !== null && <BeforeYou view={overview.before_you} />}
                  {overview.manual_checks !== null && (
                    <ManualChecks ticket={ticket} view={overview.manual_checks} />
                  )}
                </>
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
              {overview.plan !== null && (
                <PlanSection ticket={ticket} view={editor} plan={overview.plan} overviewTabs={overviewTabs} />
              )}
              {overview.planning !== null && <PlanningDid view={overview.planning} />}
            </>
          )
        )}
        <TicketDecisions ticket={ticket} />
        {SHEET_SECTIONS.map((id) => {
          const Section = app.sheetSections.get(id)?.Component;
          return Section ? <Section key={id} ticket={ticket} /> : null;
        })}
        <CommentThread ticket={ticket} />
      </div>
      <div className="overview-side">
        {sheet && <Stages view={sheet.view} />}
        {overview?.key === ticket && (
          <Attachments
            ticket={ticket}
            view={overview}
            error={error}
            onAdd={() => {
              pickAttachmentFiles(overview.pick_title).then(attachPaths, () => undefined);
            }}
          />
        )}
      </div>
    </div>
  );
}
