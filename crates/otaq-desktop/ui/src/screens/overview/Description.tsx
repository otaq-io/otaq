// The description in place (SHT-03, d0078; T-0465; *25 → Editing the description* `s25-04`): shown as it reads, with
// "Click to edit" on hover; a click turns it into the editor (toolbar, `/` for blocks) with the caret where it was
// clicked, and "Editing · Esc to discard, ⌘S to save" on its label row. Once the text differs from what is saved,
// the sheet's tab row shows "Description changed · Discard · Save ⌘S" (`descriptionDraft`, `SaveBar`); switching
// tabs, moving to another ticket or closing the sheet then asks first (`guardLeaving`): Save stores it and then goes
// where the user was going, a failed save staying in the editor with its error; Discard changes goes at once; Keep
// editing stays (Q415, `overview::leave_step`). Esc steps back as in every
// description editor (d0078 D4: a popover, then the text, then it discards). Saving sends only the description
// (`editor.save` with the target `description`); an untouched one is never saved. Its words are Rust's
// (`OverviewView`, `EditorView`).
import type { Editor } from "@tiptap/core";
import { type MouseEvent, type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { guardLeaving, requestTerminalFocus, showTab } from "../../app/selection";
import { Button } from "../../components";
import type { EditorCompareEntry } from "../../generated/EditorCompareEntry";
import type { EditorField } from "../../generated/EditorField";
import type { EditorRefusal } from "../../generated/EditorRefusal";
import type { EditorSaved } from "../../generated/EditorSaved";
import type { EditorView } from "../../generated/EditorView";
import type { OverviewView } from "../../generated/OverviewView";
import type { SheetContent } from "../../generated/SheetContent";
import { FullView, useFullView } from "../../markdown";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { Description as EditDescription, RefusalBox } from "../editor/Fields";
import { useFormKeys } from "../editor/Modal";
import {
  Compare,
  CopyMarkdown,
  DescriptionTools,
  LabelRow,
  Description as ReadDescription,
} from "../editor/ReadMode";
import { asksFirst, descriptionDraft, descriptionState, isSaveDescriptionKey } from "./draft";

const ignore = () => undefined;

const MAC = typeof navigator !== "undefined" && navigator.userAgent.includes("Mac");

/** The text being edited, and where the caret goes once the editor shows. */
interface Draft {
  text: string;
  caret?: number;
}

/** The position in the read view's text under a click, from the Tiptap instance on its element. */
function caretAt(e: MouseEvent<HTMLElement>): number | undefined {
  const dom = e.currentTarget.querySelector<HTMLElement & { editor?: Editor }>(".ProseMirror");
  return dom?.editor?.view.posAtCoords({ left: e.clientX, top: e.clientY })?.pos;
}

/** What a save tells the ticket's working agent ("SHOP-19's agent will be told what changed."), with Open terminal. */
export function SavedNote({
  ticket,
  note,
  openTerminal,
}: {
  ticket: string;
  note: string;
  openTerminal: string;
}) {
  return (
    <div className="editor-note">
      <Icon name="info" size={14} />
      <div>
        <p>{note}</p>
        <Button
          variant="secondary"
          icon="square-terminal"
          label={openTerminal}
          onClick={() => {
            showTab("agent");
            requestTerminalFocus(ticket);
          }}
        />
      </div>
    </div>
  );
}

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

export interface DescriptionSectionProps {
  ticket: string;
  view: EditorView;
  overview: OverviewView;
  /** The selection values Overview shows (`null` and the Ticket content): moving between them never asks. */
  overviewTabs: readonly SheetContent[];
}

export function DescriptionSection({ ticket, view, overview, overviewTabs }: DescriptionSectionProps) {
  const read = view.read;
  const saved = read?.body_markdown ?? "";
  const [draft, setDraft] = useState<Draft | null>(null);
  const [saving, setSaving] = useState(false);
  const [refusal, setRefusal] = useState<EditorRefusal | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [compare, setCompare] = useState<readonly EditorCompareEntry[] | null>(null);
  const state = descriptionState(saved, draft?.text ?? null);
  const full = useFullView();
  const t = view.texts;

  const discard = useCallback(() => {
    setDraft(null);
    setRefusal(null);
  }, []);

  const latest = useRef({ draft, state, saving });
  useEffect(() => {
    latest.current = { draft, state, saving };
  });
  // Stores the description; resolves whether the editor closed saved (`overview::after_save`). A refusal stays in
  // the editor, shown under it.
  const store = useCallback((): Promise<boolean> => {
    const { draft: d, state: s, saving: busy } = latest.current;
    if (d === null || busy) return Promise.resolve(false);
    // Untouched: nothing is saved, the editor just closes.
    if (s !== "changed") {
      setDraft(null);
      return Promise.resolve(true);
    }
    setSaving(true);
    setRefusal(null);
    const params = { target: { mode: "description", key: ticket }, form: { ...view.form, body: d.text } };
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

  // Leaving with changes asks first (SHT-03).
  useEffect(() => {
    if (state !== "changed") return;
    return guardLeaving((next, go) => {
      if (!asksFirst(state, ticket, next, overviewTabs)) return false;
      // Save · Discard changes · Keep editing (Q415): Save, the default, stores it first and only then goes on.
      openOverlay("confirm", {
        title: overview.leave.title,
        body: overview.leave.body,
        confirmLabel: overview.leave.save,
        cancelLabel: overview.leave.keep,
        destructive: false,
        extra: {
          label: overview.leave.discard,
          onClick: () => {
            discard();
            go();
          },
        },
        onConfirm: () =>
          store().then((saved) => {
            if (saved) go();
          }),
      });
      return true;
    });
  }, [state, ticket, overviewTabs, overview.leave, discard, store]);

  // The tab row's bar, while the editor is open.
  useEffect(() => {
    if (draft === null) return;
    descriptionDraft.set({
      ticket,
      bar: overview.bar,
      changed: state === "changed",
      saving,
      save,
      discard,
    });
  }, [draft, ticket, overview.bar, state, saving, save, discard]);
  useEffect(
    () => () => {
      descriptionDraft.set(null);
    },
    [],
  );
  useEffect(() => {
    if (draft === null) descriptionDraft.set(null);
  }, [draft]);

  // ⌘S saves from anywhere on the page while the editor is open (the page's own dispatcher swallows it otherwise).
  const editing = draft !== null;
  useEffect(() => {
    if (!editing) return;
    const onKey = (e: KeyboardEvent) => {
      if (!isSaveDescriptionKey(e, MAC)) return;
      e.preventDefault();
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
    setCompare(null);
    setRefusal(null);
  }

  if (read === null) return null;
  const onCompare = (field: EditorField | null) => {
    (request("editor.compare", { key: ticket, field }) as Promise<EditorCompareEntry[]>).then(
      setCompare,
      ignore,
    );
  };
  const start = (caret?: number) => {
    setNote(null);
    setCompare(null);
    setDraft(caret === undefined ? { text: saved } : { text: saved, caret });
  };
  const hint = (
    <span className="overview-hint" data-state={state}>
      {state === "reading" ? overview.click_to_edit : overview.editing}
    </span>
  );
  return (
    <section className="overview-description" data-state={state} aria-label={overview.description}>
      <LabelRow
        name={overview.description}
        field="body"
        read={read}
        texts={t}
        onCompare={onCompare}
        hint={hint}
        tools={
          state === "reading" ? (
            <DescriptionTools stored={read.body_markdown} texts={t} onExpand={full.expand} />
          ) : undefined
        }
      />
      {note !== null && <SavedNote ticket={ticket} note={note} openTerminal={t.open_terminal} />}
      {compare !== null && (
        <div className="overview-compare">
          <Compare entries={compare} texts={t} />
          <Button
            variant="secondary"
            label={t.close}
            onClick={() => {
              setCompare(null);
            }}
          />
        </div>
      )}
      {draft === null ? (
        // The read view is a big click target: a click (or ↵ / Space on it) opens the editor there.
        <div
          className="overview-read"
          role="button"
          tabIndex={0}
          aria-label={`${overview.description}: ${overview.click_to_edit}`}
          onClick={(e) => {
            // A link in the text opens as a link, not the editor.
            if (e.target instanceof Element && e.target.closest("a")) return;
            start(caretAt(e));
          }}
          onKeyDown={(e) => {
            if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
            e.preventDefault();
            start();
          }}
        >
          <ReadDescription read={read} texts={t} label={overview.description} />
        </div>
      ) : (
        <Editing onSave={save} onDiscard={discard}>
          <EditDescription
            view={view}
            body={draft.text}
            title={read.title}
            onChange={(text) => {
              setDraft((d) => (d === null ? d : { ...d, text }));
            }}
            onSave={save}
            {...(draft.caret === undefined ? {} : { caret: draft.caret })}
            placeholder={overview.placeholder}
          />
          <RefusalBox refusal={refusal} />
        </Editing>
      )}
      {full.slot !== null && draft === null && (
        <FullView
          slot={full.slot}
          ticketKey={ticket}
          name={t.body}
          title={read.title}
          tools={
            <span className="editor-tools">
              <CopyMarkdown stored={read.body_markdown} texts={t} />
            </span>
          }
          collapseLabel={t.collapse}
          onCollapse={full.collapse}
        >
          <ReadDescription read={read} texts={t} label={t.body} />
        </FullView>
      )}
    </section>
  );
}
