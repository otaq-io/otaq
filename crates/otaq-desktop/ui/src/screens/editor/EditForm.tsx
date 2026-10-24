// The whole-form editor in place (UI-05, SES-08; d0019, d0048; *08 → Editing while the agent works*): what ⋯ Edit
// ticket… (through `ticketEditRequests`) opens in the sheet's Overview, in place of the
// description (T-0465: the description itself edits in place, `screens/overview`). Saving a working ticket tells its
// agent what changed: Overview shows the note, with Open terminal. While it validates, the form warns first that
// saving cancels the validation (VAL-08, *Editing while it validates*).
import { useEffect, useRef } from "react";

import { ticketEditRequests } from "../../app/selection";
import { Button, Kbd } from "../../components";
import type { EditorView } from "../../generated/EditorView";
import { Icon } from "../../theme/Icon";
import { EditorFields } from "./Fields";
import { SAVE_KEYS } from "./logic";
import { useFormKeys } from "./Modal";
import { useEditor } from "./useEditor";

/** Takes the edit request meant for `ticket`, once (a later visit to the tab doesn't open the form again). */
export function takeEditRequest(ticket: string): boolean {
  if (ticketEditRequests.get()?.ticket !== ticket) return false;
  ticketEditRequests.set(null);
  return true;
}

/** Gives the description field the height of the tab's scroll area as `--editor-fill`: it may grow until it fills the tab
 * (d0078 D5). */
export function useFillHeight(scroll: React.RefObject<HTMLElement | null>) {
  useEffect(() => {
    const el = scroll.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const fit = () => {
      el.style.setProperty("--editor-fill", `${String(el.clientHeight)}px`);
    };
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, [scroll]);
}

export function EditForm({ view, onDone }: { view: EditorView; onDone: (note: string | null) => void }) {
  const editor = useEditor(view, { mode: "edit", key: view.key ?? "" });
  const root = useRef<HTMLDivElement>(null);
  const scroll = useRef<HTMLDivElement>(null);
  useFillHeight(scroll);
  const save = () => {
    editor.save((saved) => {
      onDone(saved.note);
    });
  };
  // Esc in a field discards (the sheet's Esc waits while a field has the keyboard). In the description it steps back
  // first (d0078 D4): an open popover or menu closes, then the text gives up focus, then the form discards.
  useFormKeys(root, save, () => {
    onDone(null);
  });
  const t = view.texts;
  const bodyChanged = editor.form.body !== view.form.body;
  return (
    <div ref={root} className="editor-tab-form">
      <div ref={scroll} className="editor-tab-scroll">
        {view.save_warning !== null && (
          <div className="editor-warning" role="status">
            <Icon name="triangle-alert" size={14} />
            <p>{view.save_warning}</p>
          </div>
        )}
        <EditorFields
          view={view}
          form={editor.form}
          onChange={editor.setForm}
          titleMissing={editor.titleMissing}
          refusal={editor.refusal}
          autoFocusTitle
          onSave={save}
        />
      </div>
      <div className="editor-footer">
        <span className="dialog-hint">
          <Kbd keys={SAVE_KEYS} />
          <span>{t.save_word}</span>
          <Kbd keys="esc" />
          <span>{t.discard_word}</span>
        </span>
        {view.save_hint !== null && <span className="editor-save-hint">{view.save_hint}</span>}
        {view.unsaved_warning !== null && bodyChanged && (
          <span className="editor-unsaved">
            <Icon name="pencil" size={11} />
            {view.unsaved_warning}
          </span>
        )}
        <span className="editor-spacer" />
        <Button
          variant="ghost"
          label={t.cancel}
          onClick={() => {
            onDone(null);
          }}
        />
        <Button label={t.primary} icon="check" loading={editor.saving} onClick={save} />
      </div>
    </div>
  );
}
