// One editor's interaction state (T-0228): the form as typed, the save in flight, the title error once a save was
// tried, the refusal under its field and a new ticket's report. Shared by the modal and the Ticket tab. A new ticket's
// Waits for is in the form; an existing ticket's dependencies are Overview's (DPV-04).
import { useCallback, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorRefusal } from "../../generated/EditorRefusal";
import type { EditorSaved } from "../../generated/EditorSaved";
import type { EditorTarget } from "../../generated/EditorTarget";
import type { EditorView } from "../../generated/EditorView";
import { request } from "../../tauri";
import { canSave } from "./logic";

const ignore = () => undefined;

type Saved = Extract<EditorSaved, { outcome: "saved" }>;

/**
 * Creates (or saves) the form and starts the ticket in one request, `editor.create_and_start`: the ticket goes into its
 * flow's first stage, or waits at its top saying why; a refused start comes back as a refusal in words, the ticket left
 * created in Backlog (`created`). The box for new work's ⌘↵ sends the same request.
 */
export function createAndStart(
  target: EditorTarget,
  form: EditorForm,
  ask: typeof request = request,
): Promise<EditorSaved> {
  return ask("editor.create_and_start", { target, form }) as Promise<EditorSaved>;
}

export interface Editor {
  form: EditorForm;
  setForm: (form: EditorForm) => void;
  /** Changes some fields of the form as it is now (a report that arrives after a request, RPT-03). */
  patchForm: (fields: Partial<EditorForm>) => void;
  saving: boolean;
  /** Which request is in flight, for its button's loader. */
  sending: "save" | "start" | null;
  titleMissing: boolean;
  refusal: EditorRefusal | null;
  /** Saves; `done` gets the outcome once it is saved. */
  save: (done: (saved: Saved) => void) => void;
  /** Create & start: saves, then starts the ticket; `done` gets the outcome once it is saved and started. */
  start: (done: (saved: Saved) => void) => void;
  /**
   * A new ticket was created but a link, a stage choice or its start was refused: the editor goes on as that
   * ticket's.
   */
  created: string | null;
}

export function useEditor(view: EditorView, target: EditorTarget, initial?: EditorForm): Editor {
  const [form, setForm] = useState<EditorForm>(initial ?? view.form);
  const [sending, setSending] = useState<"save" | "start" | null>(null);
  const [titleMissing, setTitleMissing] = useState(false);
  const [refusal, setRefusal] = useState<EditorRefusal | null>(null);
  const [created, setCreated] = useState<string | null>(null);
  // Stable: a report's chip arrives after a request and must not depend on the render it started in.
  const patchForm = useCallback((fields: Partial<EditorForm>) => {
    setForm((f) => ({ ...f, ...fields }));
    setRefusal((r) => (r?.field === "form" ? null : r));
  }, []);
  const saving = sending !== null;
  const send = (what: "save" | "start", done: (saved: Saved) => void) => {
    setTitleMissing(true);
    if (!canSave(form, saving)) return;
    setSending(what);
    setRefusal(null);
    const to: EditorTarget = created === null ? target : { mode: "edit", key: created };
    const sent =
      what === "start"
        ? createAndStart(to, form)
        : (request("editor.save", { target: to, form }) as Promise<EditorSaved>);
    sent
      .then((out) => {
        if (out.outcome === "saved") done(out);
        else if (out.outcome === "needs_identity") {
          // Nothing was created: git's name and email first, then Create & start again.
          openOverlay("git_identity", {
            form: out.form,
            save: out.save,
            cancel: out.cancel,
            then: () => {
              send(what, done);
            },
          });
        } else if (out.outcome === "refused") {
          setRefusal(out.refusal);
          if (out.created !== null) setCreated(out.created);
        }
      }, ignore)
      .finally(() => {
        setSending(null);
      });
  };
  return {
    form,
    setForm: (next) => {
      setForm(next);
      if (refusal?.field === "form" || refusal?.field === "base") setRefusal(null);
    },
    patchForm,
    saving,
    sending,
    titleMissing,
    refusal,
    created,
    save: (done) => {
      send("save", done);
    },
    start: (done) => {
      send("start", done);
    },
  };
}
