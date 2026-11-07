// The editor as a modal (UI-05, TKT-03; d0019; *08 · Ticket editor → Full form · filled, Validation · empty form,
// Dark · full form*). The box for new work's More options… opens the full form with what was typed (`screens/
// new_work`), and New ticket from this file with the file's path. The full form saves with ⌘↵ or its button; a new ticket's Create & start (⇧⌘↵, or the button beside Create)
// creates it and starts it, and a refused start keeps the form open on the created ticket, saying why. Esc discards. `edit_ticket` opens the same form on a ticket.
import { useCallback, useEffect, useEffectEvent, useRef } from "react";

import type { OverlayProps } from "../../app/registry";
import { useSelection } from "../../app/selection";
import { Button, Dialog, Kbd } from "../../components";
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorTarget } from "../../generated/EditorTarget";
import type { EditorView } from "../../generated/EditorView";
import { inKeyLayer, routeKey } from "../../markdown";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { EditorFields } from "./Fields";
import { isStartKey, SAVE_KEYS, START_KEYS } from "./logic";
import { NewTicketReport, useReportAttach } from "./NewTicketReport";
import { useEditor } from "./useEditor";

/**
 * Calls `onSave` on ⌘↵ anywhere inside `root` (the description too, where ↵ is a new line), and `onDiscard` on Esc
 * (d0078 D4): only once nothing inside has it, so an open popover or menu closes first and the description's text
 * gives up focus first (`routeKey`). With `onStart` (the new-ticket form's Create & start), ⇧⌘↵ calls it instead.
 */
export function useFormKeys(
  root: React.RefObject<HTMLElement | null>,
  onSave: () => void,
  onDiscard?: () => void,
  onStart?: () => void,
) {
  const save = useEffectEvent(onSave);
  const discard = useEffectEvent(() => onDiscard?.());
  const start = useEffectEvent(() => onStart?.());
  const discards = onDiscard !== undefined;
  const starts = onStart !== undefined;
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
      if (route.kind === "save" && starts && isStartKey(e)) start();
      else if (route.kind === "save") save();
      else if (route.kind === "discard") discard();
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    el.addEventListener("keydown", onKey);
    return () => {
      el.removeEventListener("keydown", onKey);
    };
  }, [root, discards, starts]);
}

export function FullForm({
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
  const t = view.texts;
  // A new ticket's Create & start, beside Create: only you start a ticket, here with one click (or ⇧⌘↵).
  const startLabel = editor.created === null ? t.create_and_start : t.save_and_start;
  const start =
    t.create_and_start === null
      ? undefined
      : () => {
          editor.start(close);
        };
  useFormKeys(root, save, undefined, start);
  // A new ticket's report (RPT-03).
  const takesReport = view.report !== null;
  const { patchForm } = editor;
  const report = useReportAttach(root, takesReport, patchForm);
  return (
    <Dialog
      title={`${view.heading} ${t.in_word} ${view.project_name}`}
      width={760}
      hint={t.discard_word}
      onCancel={close}
      onConfirm={save}
      footer={
        <>
          <span className="dialog-hint">
            <Kbd keys={SAVE_KEYS} />
            <span>{t.save_word}</span>
            {start !== undefined && (
              <>
                <Kbd keys={START_KEYS} />
                <span>{t.start_word}</span>
              </>
            )}
          </span>
          <span className="editor-spacer" />
          <Button variant="ghost" label={t.cancel} onClick={close} />
          {start !== undefined && startLabel !== null && (
            <Button
              variant="secondary"
              label={startLabel}
              icon="play"
              loading={editor.sending === "start"}
              disabled={editor.sending === "save"}
              onClick={start}
            />
          )}
          <Button
            label={editor.created === null ? t.primary : view.texts.edit}
            icon="check"
            loading={editor.sending === "save"}
            disabled={editor.sending === "start"}
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

export function NewTicket({ params, close }: OverlayProps<"new_ticket">) {
  const { project } = useSelection();
  const title = params?.title;
  const body = params?.body;
  const target: EditorTarget = { mode: "create", project };
  const load = useCallback(() => {
    const open: EditorTarget = {
      mode: "create",
      project,
      ...(title !== undefined && { title }),
      ...(body !== undefined && { body }),
    };
    return request("editor.open", open) as Promise<EditorView>;
  }, [project, title, body]);
  const { view, error } = useServiceView(load, []);
  if (error) return null;
  if (!view) return null;
  return <FullForm view={view} target={target} close={close} />;
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
