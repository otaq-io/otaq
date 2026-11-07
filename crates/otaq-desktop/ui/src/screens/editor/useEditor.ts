// One editor's interaction state (T-0228): the form as typed, the save in flight, the title error once a save was
// tried, the refusal under its field and a new ticket's report. Shared by the modal and the Ticket tab. A new ticket's
// Waits for is in the form; an existing ticket's dependencies are Overview's (DPV-04).
import { useCallback, useState } from "react";

import type { EditorForm } from "../../generated/EditorForm";
import type { EditorRefusal } from "../../generated/EditorRefusal";
import type { EditorSaved } from "../../generated/EditorSaved";
import type { EditorTarget } from "../../generated/EditorTarget";
import type { EditorView } from "../../generated/EditorView";
import { request } from "../../tauri";
import { canSave } from "./logic";

const ignore = () => undefined;

export interface Editor {
  form: EditorForm;
  setForm: (form: EditorForm) => void;
  /** Changes some fields of the form as it is now (a report that arrives after a request, RPT-03). */
  patchForm: (fields: Partial<EditorForm>) => void;
  saving: boolean;
  titleMissing: boolean;
  refusal: EditorRefusal | null;
  /** Saves; `done` gets the outcome once it is saved. */
  save: (done: (saved: Extract<EditorSaved, { outcome: "saved" }>) => void) => void;
  /** A new ticket was created but a link or a stage choice was refused: the editor goes on as that ticket's. */
  created: string | null;
}

export function useEditor(view: EditorView, target: EditorTarget, initial?: EditorForm): Editor {
  const [form, setForm] = useState<EditorForm>(initial ?? view.form);
  const [saving, setSaving] = useState(false);
  const [titleMissing, setTitleMissing] = useState(false);
  const [refusal, setRefusal] = useState<EditorRefusal | null>(null);
  const [created, setCreated] = useState<string | null>(null);
  // Stable: a report's chip arrives after a request and must not depend on the render it started in.
  const patchForm = useCallback((fields: Partial<EditorForm>) => {
    setForm((f) => ({ ...f, ...fields }));
    setRefusal((r) => (r?.field === "form" ? null : r));
  }, []);
  return {
    form,
    setForm: (next) => {
      setForm(next);
      if (refusal?.field === "form" || refusal?.field === "base") setRefusal(null);
    },
    patchForm,
    saving,
    titleMissing,
    refusal,
    created,
    save: (done) => {
      setTitleMissing(true);
      if (!canSave(form, saving)) return;
      setSaving(true);
      setRefusal(null);
      const to: EditorTarget = created === null ? target : { mode: "edit", key: created };
      (request("editor.save", { target: to, form }) as Promise<EditorSaved>)
        .then((out) => {
          if (out.outcome === "saved") done(out);
          else if (out.outcome === "refused") {
            setRefusal(out.refusal);
            if (out.created !== null) setCreated(out.created);
          }
        }, ignore)
        .finally(() => {
          setSaving(false);
        });
    },
  };
}
