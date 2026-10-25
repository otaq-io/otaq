// The ticket's PLAN under the description (d0133; T-0540; *30 → The Plan section* `s30-01`): written by the planning
// stage (Milestone plan) or Noyan, read and edited like the description (d0078). Without a plan, one quiet dashed line
// with Write a plan. With one, its header ("Written by Milestone plan · 2:40 pm", "… · edited by you · 2:58 pm",
// "… · writing now", "… · approved at Spec review") and the Markdown as written, its Pieces list included; while the
// planning stage writes it, "writing the next piece…" under it. A click (or Write a plan) opens the same editor as the
// description, with Discard and Save on the label row; ⌘S inside it and ⌘↵ save, Esc discards (d0078 D4). Leaving
// with changes asks first ("Discard your plan changes?": Save, then go on; Discard changes; Keep editing), after the
// description's own question when both changed. Saving sends only the Plan (`editor.save` with the target `plan`); an
// untouched one is never saved. Once the ticket split its work among helpers (d0133, `s30-03`), each piece's status sits
// in its own column beside the text ("Status · from Otaq", the sentence on hover): Otaq's record, never part of the
// Plan, so an edit never saves it. Every word is Rust's (`PlanSectionView`, `plan.rs`, `pieces.rs`).
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { guardLeaving } from "../../app/selection";
import { toneInk } from "../../cards/look";
import { Button, Tooltip } from "../../components";
import type { EditorRefusal } from "../../generated/EditorRefusal";
import type { EditorSaved } from "../../generated/EditorSaved";
import type { EditorView } from "../../generated/EditorView";
import type { PlanSectionView } from "../../generated/PlanSectionView";
import type { PlanStatusesView } from "../../generated/PlanStatusesView";
import type { SheetContent } from "../../generated/SheetContent";
import { MarkdownEditor } from "../../markdown";
import { request } from "../../tauri";
import { Description as EditPlan, RefusalBox } from "../editor/Fields";
import { useFormKeys } from "../editor/Modal";
import { SavedNote } from "./Description";
import { asksFirst, descriptionState, isSaveDescriptionKey } from "./draft";

const ignore = () => undefined;

const MAC = typeof navigator !== "undefined" && navigator.userAgent.includes("Mac");

/** The open editor's frame: Esc discards once the text gave up focus, ⌘↵ saves too (d0078 D4). */
function Editing({
  onSave,
  onDiscard,
  children,
}: {
  onSave: () => void;
  onDiscard: () => void;
  children: ReactNode;
}) {
  const root = useRef<HTMLDivElement>(null);
  useFormKeys(root, onSave, onDiscard);
  return (
    <div ref={root} className="overview-edit">
      {children}
    </div>
  );
}

/** Otaq's status of each piece, beside the Plan's text (d0133 audit answer 1). */
function Statuses({ statuses }: { statuses: PlanStatusesView }) {
  return (
    <aside className="overview-plan-statuses" aria-label={statuses.label}>
      <Tooltip text={statuses.hover}>
        <span className="overview-plan-statuses-label">{statuses.label}</span>
      </Tooltip>
      {statuses.rows.map((row) => (
        <div key={row.name} className="overview-plan-status">
          <span className="overview-plan-status-name">{row.name}</span>
          <span
            className="overview-plan-status-chip"
            style={{ color: toneInk(row.status.tone).fg, background: toneInk(row.status.tone).bg }}
          >
            {row.status.text}
          </span>
        </div>
      ))}
    </aside>
  );
}

export interface PlanSectionProps {
  ticket: string;
  /** The ticket's editor view: the editor's words and the form a save sends with. */
  view: EditorView;
  plan: PlanSectionView;
  /** The selection values Overview shows: moving between them never asks. */
  overviewTabs: readonly SheetContent[];
}

export function PlanSection({ ticket, view, plan, overviewTabs }: PlanSectionProps) {
  const saved = plan.markdown;
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [refusal, setRefusal] = useState<EditorRefusal | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const state = descriptionState(saved, draft);

  const discard = useCallback(() => {
    setDraft(null);
    setRefusal(null);
  }, []);

  const latest = useRef({ draft, state, saving });
  useEffect(() => {
    latest.current = { draft, state, saving };
  });
  // Stores the Plan; resolves whether the editor closed saved. A refusal stays in the editor, shown under it.
  const store = useCallback((): Promise<boolean> => {
    const { draft: d, state: s, saving: busy } = latest.current;
    if (d === null || busy) return Promise.resolve(false);
    if (s !== "changed") {
      setDraft(null);
      return Promise.resolve(true);
    }
    setSaving(true);
    setRefusal(null);
    const params = { target: { mode: "plan", key: ticket }, form: { ...view.form, body: d } };
    return (request("editor.save", params) as Promise<EditorSaved>)
      .then(
        (out) => {
          if (out.outcome === "saved") {
            setDraft(null);
            setNote(out.note);
            return true;
          }
          if (out.outcome === "refused") setRefusal(out.refusal);
          return false;
        },
        () => false,
      )
      .finally(() => {
        setSaving(false);
      });
  }, [ticket, view.form]);
  const save = useCallback(() => {
    store().catch(ignore);
  }, [store]);

  // Leaving with changes asks first (T-0511's rules).
  useEffect(() => {
    if (state !== "changed") return;
    return guardLeaving((next, go) => {
      if (!asksFirst(state, ticket, next, overviewTabs)) return false;
      openOverlay("confirm", {
        title: plan.leave.title,
        body: plan.leave.body,
        confirmLabel: plan.leave.save,
        cancelLabel: plan.leave.keep,
        destructive: false,
        extra: {
          label: plan.leave.discard,
          onClick: () => {
            discard();
            go();
          },
        },
        onConfirm: () =>
          store().then((ok) => {
            if (ok) go();
          }),
      });
      return true;
    });
  }, [state, ticket, overviewTabs, plan.leave, discard, store]);

  // ⌘S saves while the caret is in the Plan's editor (the description's own ⌘S saves the description).
  const root = useRef<HTMLElement>(null);
  const editing = draft !== null;
  useEffect(() => {
    if (!editing) return;
    const onKey = (e: KeyboardEvent) => {
      if (!isSaveDescriptionKey(e, MAC)) return;
      if (!(e.target instanceof Node) || !root.current?.contains(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      save();
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
    };
  }, [editing, save]);

  // Another ticket: whatever was open closes.
  const [shownFor, setShownFor] = useState(ticket);
  if (shownFor !== ticket) {
    setShownFor(ticket);
    setDraft(null);
    setNote(null);
    setRefusal(null);
  }

  const start = () => {
    setNote(null);
    setDraft(saved);
  };
  return (
    <section ref={root} className="overview-plan" data-state={state} aria-label={plan.label}>
      <div className="editor-section">
        <span className="editor-section-name">{plan.label.toUpperCase()}</span>
        {editing ? (
          <span className="overview-plan-by">{plan.editing}</span>
        ) : (
          plan.written_by !== null && <span className="overview-plan-by">{plan.written_by}</span>
        )}
        <span className="editor-spacer" />
        {editing ? (
          <>
            <Button compact variant="ghost" label={plan.bar.discard} onClick={discard} />
            <Button compact icon="check" label={plan.bar.save} loading={saving} onClick={save} />
          </>
        ) : (
          plan.empty === null && <span className="overview-hint">{plan.click_to_edit}</span>
        )}
      </div>
      {note !== null && <SavedNote ticket={ticket} note={note} openTerminal={view.texts.open_terminal} />}
      {editing ? (
        <Editing onSave={save} onDiscard={discard}>
          <EditPlan
            view={view}
            body={draft}
            title={view.read?.title ?? ""}
            name={plan.label}
            onChange={(text) => {
              setDraft((d) => (d === null ? d : text));
            }}
            onSave={save}
            placeholder={plan.placeholder}
          />
          <RefusalBox refusal={refusal} />
        </Editing>
      ) : plan.empty !== null ? (
        <div className="overview-plan-empty">
          <span>{plan.empty.line}</span>
          <Button compact variant="secondary" icon="plus" label={plan.empty.write} onClick={start} />
        </div>
      ) : (
        <div
          className="overview-read"
          role="button"
          tabIndex={0}
          aria-label={`${plan.label}: ${plan.click_to_edit}`}
          onClick={(e) => {
            // A link in the text opens as a link, not the editor.
            if (e.target instanceof Element && e.target.closest("a")) return;
            start();
          }}
          onKeyDown={(e) => {
            if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
            e.preventDefault();
            start();
          }}
        >
          <div className="overview-plan-text">
            <MarkdownEditor
              value={plan.markdown}
              editable={false}
              label={plan.label}
              className="editor-read-body"
            />
            {plan.writing_next !== null && <p className="overview-plan-next">{plan.writing_next}</p>}
          </div>
          {plan.statuses !== null && <Statuses statuses={plan.statuses} />}
        </div>
      )}
    </section>
  );
}
