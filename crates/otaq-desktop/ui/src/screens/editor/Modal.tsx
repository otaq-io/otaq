// The editor as a modal (UI-05, TKT-03; d0019; *08 · Ticket editor → Quick-create in Backlog, Full form · filled,
// Validation · empty form, Dark · full form*). ⌘N, the board's + and Add ticket to epic open **quick-create**, a
// title field at the top of the Backlog column: ↵ creates, ⇧↵ opens the full form with the title kept, Esc
// cancels. The full form saves with ⌘↵ or its button; Esc discards. `edit_ticket` opens the same form on a ticket.
import { useCallback, useEffect, useEffectEvent, useRef, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { useSelection } from "../../app/selection";
import { Button, Dialog, Kbd, PLAIN_TEXT } from "../../components";
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorTarget } from "../../generated/EditorTarget";
import type { EditorView } from "../../generated/EditorView";
import { inKeyLayer, routeKey } from "../../markdown";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { EditorFields } from "./Fields";
import { quickKey, SAVE_KEYS } from "./logic";
import { NewTicketReport, useReportAttach } from "./NewTicketReport";
import { useEditor } from "./useEditor";

/**
 * Calls `onSave` on ⌘↵ anywhere inside `root` (the description too, where ↵ is a new line), and `onDiscard` on Esc
 * (d0078 D4): only once nothing inside has it, so an open popover or menu closes first and the description's text
 * gives up focus first (`routeKey`).
 */
export function useFormKeys(
  root: React.RefObject<HTMLElement | null>,
  onSave: () => void,
  onDiscard?: () => void,
) {
  const save = useEffectEvent(onSave);
  const discard = useEffectEvent(() => onDiscard?.());
  const discards = onDiscard !== undefined;
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const onKey = (e: KeyboardEvent) => {
      const route = routeKey(e, {
        layer: inKeyLayer(e.target, el),
        focus: "form",
        discards,
        taken: e.defaultPrevented,
      });
      if (route.kind === "save") save();
      else if (route.kind === "discard") discard();
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    el.addEventListener("keydown", onKey);
    return () => {
      el.removeEventListener("keydown", onKey);
    };
  }, [root, discards]);
}

function FullForm({
  view,
  target,
  initial,
  close,
}: {
  view: EditorView;
  target: EditorTarget;
  initial?: EditorForm;
  close: () => void;
}) {
  const editor = useEditor(view, target, initial);
  const root = useRef<HTMLDivElement>(null);
  const save = () => {
    editor.save(close);
  };
  useFormKeys(root, save);
  // A new ticket's report (RPT-03): not for an epic, whose lead reads no comments of its own.
  const takesReport = view.report !== null && !editor.form.epic_kind;
  const { patchForm } = editor;
  const report = useReportAttach(root, takesReport, patchForm);
  const t = view.texts;
  return (
    <Dialog
      title={`${view.heading} ${t.in_word} ${view.project_name}`}
      width={620}
      hint={t.discard_word}
      onCancel={close}
      onConfirm={save}
      footer={
        <>
          <span className="dialog-hint">
            <Kbd keys={SAVE_KEYS} />
            <span>{t.save_word}</span>
          </span>
          <span className="editor-spacer" />
          <Button variant="ghost" label={t.cancel} onClick={close} />
          <Button
            label={editor.created === null ? t.primary : view.texts.edit}
            icon="check"
            loading={editor.saving}
            onClick={save}
          />
        </>
      }
    >
      <div ref={root} className="editor-modal-body" onPaste={report.onPaste}>
        <EditorFields
          view={view}
          form={editor.form}
          onChange={editor.setForm}
          titleMissing={editor.titleMissing}
          refusal={editor.refusal}
          autoFocusTitle
          onSave={save}
        />
        {takesReport && view.report !== null && (
          <NewTicketReport field={view.report} project={view.project} attach={report} onSave={save} />
        )}
      </div>
    </Dialog>
  );
}

/** Where quick-create sits: over the top of the Backlog column when it is on screen, else near the top. */
function quickPlace(): React.CSSProperties {
  const column = document.querySelector<HTMLElement>('[data-column="backlog"]');
  const r = column?.getBoundingClientRect();
  if (!r) return { left: "50%", top: 96, transform: "translateX(-50%)" };
  return { left: r.left + 6, top: r.top + 34, width: r.width - 12 };
}

function QuickCreate({
  view,
  target,
  onFull,
  close,
}: {
  view: EditorView;
  target: EditorTarget;
  onFull: (form: EditorForm) => void;
  close: () => void;
}) {
  const editor = useEditor(view, target);
  const [place] = useState(quickPlace);
  const t = view.texts;
  const priority = view.priorities.find((p) => p.number === view.shown_priority);
  return (
    <div
      className="editor-quick-backdrop"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div className="editor-quick" role="dialog" aria-label={view.heading} style={place}>
        <input
          {...PLAIN_TEXT}
          className="editor-quick-title"
          value={editor.form.title}
          placeholder={t.title_placeholder}
          aria-label={t.title}
          // Quick-create is a title field opened by the user's ⌘N or +: typing goes straight in.
          // eslint-disable-next-line jsx-a11y/no-autofocus -- opened by the user's own click or key
          autoFocus
          onChange={(e) => {
            editor.setForm({ ...editor.form, title: e.target.value });
          }}
          onKeyDown={(e) => {
            const what = quickKey(e.nativeEvent);
            if (what === null) return;
            e.preventDefault();
            e.stopPropagation();
            if (what === "cancel") close();
            else if (what === "full") onFull(editor.form);
            else editor.save(close);
          }}
        />
        <div className="editor-quick-hints">
          <Kbd keys="↵" />
          <span>{t.quick_create}</span>
          <Kbd keys="⇧↵" />
          <span>{t.quick_full}</span>
          <span className="editor-spacer" />
          {priority && <span className="editor-quick-priority">{priority.view.short}</span>}
        </div>
        {editor.titleMissing && editor.form.title.trim() === "" && (
          <span className="field-note" data-error>
            {t.title_required}
          </span>
        )}
        {editor.refusal && (
          <span className="field-note" data-error>
            {editor.refusal.message}
          </span>
        )}
      </div>
    </div>
  );
}

export function NewTicket({ params, close }: OverlayProps<"new_ticket">) {
  const { project } = useSelection();
  const epic = params?.epic ?? null;
  const body = params?.body;
  const target: EditorTarget = { mode: "create", project, epic };
  const load = useCallback(
    () => request("editor.open", { mode: "create", project, epic, body }) as Promise<EditorView>,
    [project, epic, body],
  );
  const { view, error } = useServiceView(load, []);
  const [full, setFull] = useState<EditorForm | null>(null);
  if (error) return null;
  if (!view) return null;
  if (full !== null) return <FullForm view={view} target={target} initial={full} close={close} />;
  // With a description to start from (New ticket from this file), the whole form: quick-create has only a title.
  if (body !== undefined) return <FullForm view={view} target={target} close={close} />;
  return <QuickCreate view={view} target={target} onFull={setFull} close={close} />;
}

export function EditTicket({ params, close }: OverlayProps<"edit_ticket">) {
  const load = useCallback(
    () => request("editor.open", { mode: "edit", key: params.ticket }) as Promise<EditorView>,
    [params.ticket],
  );
  const { view } = useServiceView(load, []);
  if (!view) return null;
  return <FullForm view={view} target={{ mode: "edit", key: params.ticket }} close={close} />;
}
