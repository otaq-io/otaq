// The editor's form (UI-05, TKT-03, TKT-09, DEP-02, DEP-04; d0019, d0051, d0065, d0067; *08 · Ticket editor → Full
// form · filled, Validation · empty form, Row · base branch per ticket, Row · epics and base branches*): Title,
// Priority, Type and Epic (when creating), What and why, Start from and merge into, External ID. Dependencies are the
// sheet's (DPV-04, `screens/dependencies`). Text fields are the webview's own (selection, IME,
// undo); What and why is the Markdown editor with its toolbar (TKT-10; d0078 D5; *08 → b · Filled · edit mode,
// i · At max height, scrolled*). Used by the modal (new ticket, ⌘N) and the Ticket tab.
import { type ReactNode, useEffect, useRef, useState } from "react";

import { Button, Field, Kbd, Segmented, Select, StageChoices, PLAIN_TEXT } from "../../components";
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorRefusal } from "../../generated/EditorRefusal";
import type { EditorView } from "../../generated/EditorView";
import type { MenuItem } from "../../generated/MenuItem";
import {
  ExpandButton,
  FullView,
  fullViewToolbar,
  MarkdownSurface,
  MarkdownToolbar,
  openingMode,
  rememberMode,
  SourceArea,
  SourceToggle,
  useFocusOnLeave,
  useFullView,
  useMarkdownEditor,
  useSourceMode,
} from "../../markdown";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { isSaveKey, refusalFor, SAVE_KEYS, shownPriority } from "./logic";

const ignore = () => undefined;

export function RefusalBox({ refusal }: { refusal: EditorRefusal | null }) {
  if (refusal === null) return null;
  return (
    <div className="editor-refusal" role="alert">
      <Icon name={refusal.icon} size={14} />
      <div>
        <p className="editor-refusal-title">{refusal.title}</p>
        <p className="editor-refusal-message">{refusal.message}</p>
      </div>
    </div>
  );
}

/**
 * "Stage choices" under Priority (AGM-09; *08 → Row · Stage choices (update AG §9)*): a select per stage with options,
 * each on its stage's own agent until picked; none on the default flow and once Type is Epic. The picks stay in the
 * form and go with Create or Save (`form.choices`).
 */
function ChoicesField({
  view,
  form,
  onChange,
}: {
  view: EditorView;
  form: EditorForm;
  onChange: (form: EditorForm) => void;
}) {
  if (view.choices === null || form.epic_kind) return null;
  const picks = Object.fromEntries(form.choices.map((c) => [c.stage_id, c.choice]));
  return (
    <div className="editor-field">
      <StageChoices
        view={view.choices}
        look="form"
        picks={picks}
        onPick={(stage, choice) => {
          onChange({
            ...form,
            choices: [...form.choices.filter((c) => c.stage_id !== stage), { stage_id: stage, choice }],
          });
        }}
      />
    </div>
  );
}

function BaseSelect({
  view,
  form,
  onChange,
}: {
  view: EditorView;
  form: EditorForm;
  onChange: (f: EditorForm) => void;
}) {
  const [items, setItems] = useState<readonly MenuItem[]>([]);
  const editable = view.base.kind === "editable";
  useEffect(() => {
    if (!editable) return;
    let live = true;
    (request("editor.branches", { project: view.project }) as Promise<MenuItem[]>).then((list) => {
      if (live) setItems(list);
    }, ignore);
    return () => {
      live = false;
    };
  }, [editable, view.project]);
  if (view.base.kind !== "editable") {
    return (
      <Select
        label={view.texts.base}
        value={view.base.kind === "locked" ? `${form.base_branch ?? ""} · ${view.base.text}` : view.base.text}
        icon="git-branch"
        mono
        trailing="lock"
        disabled
        items={[]}
        onChange={ignore}
      />
    );
  }
  const shown = items.find((i) => i.id === form.base_branch)?.label ?? form.base_branch ?? "";
  return (
    <Select
      label={view.texts.base}
      value={shown}
      icon="git-branch"
      mono
      trailing="chevrons-up-down"
      search={view.texts.base_search}
      items={items.map((i) => ({ ...i, checked: !i.header && i.id === form.base_branch }))}
      onChange={(id) => {
        onChange({ ...form, base_branch: id });
      }}
    />
  );
}

/**
 * What and why: the description editor with its toolbar. The form keeps the stored string until the user edits the
 * text (the editor reports nothing for an untouched description), so an untouched open → Save stores it byte for
 * byte. The field grows up to its max height, then the text scrolls under the pinned toolbar (editor.css).
 *
 * The toolbar's "Markdown" toggle shows the raw source instead (d0078 D9): it opens in the mode last chosen, and a
 * switch is remembered for every editor (`editor.source_mode`). Switching alone never changes the form.
 *
 * Expand opens the same editor over the whole window (d0078 D11, `useFullView`): the instance and the toggle's mode
 * live here, so only where they're drawn changes, and text, caret, undo and mode carry over both ways. Cancel there
 * collapses and the form keeps the edits; Save collapses and saves the form.
 */
export function Description({
  view,
  body,
  title,
  onChange,
  onSave,
  caret,
  placeholder,
}: {
  view: EditorView;
  body: string;
  /** The ticket's title as typed, for the full view's header. */
  title: string;
  onChange: (body: string) => void;
  onSave: () => void;
  /**
   * Overview's click to edit (SHT-03): the caret goes to this position of the text once it shows (where the read view
   * was clicked; both are loaded from the same Markdown), and the text takes the keyboard.
   */
  caret?: number;
  /** The muted text while it is empty, when not the form's ("Type / for blocks"). */
  placeholder?: string;
}) {
  const t = view.texts;
  const mode = useSourceMode(body, openingMode(view.source_mode));
  const editor = useMarkdownEditor({
    value: mode.editorValue,
    onChange,
    placeholder: placeholder ?? t.body_placeholder,
    label: t.body,
    // ⌘↵ saves the form (`useFormKeys` on the form's root): the editor leaves it, or Tiptap would add a line break.
    onKeyDown: (e) => isSaveKey(e) || undefined,
  });
  useFocusOnLeave(editor, mode);
  const placed = useRef(false);
  useEffect(() => {
    if (caret === undefined || placed.current || mode.on) return;
    placed.current = true;
    const at = Math.min(Math.max(caret, 0), editor.state.doc.content.size);
    editor.chain().focus(at, { scrollIntoView: false }).run();
  }, [editor, caret, mode.on]);
  const full = useFullView();
  const expanded = full.slot !== null;
  // The caret goes where the text now shows, after each Expand and Collapse (the view moved, so the focus was lost).
  // A frame later, once the moved text has settled in its new place.
  const shownIn = useRef(expanded);
  useEffect(() => {
    if (shownIn.current === expanded) return;
    shownIn.current = expanded;
    const frame = requestAnimationFrame(() => {
      if (editor.isDestroyed) return;
      const source = editor.view.dom.closest(".md-editor")?.querySelector<HTMLElement>(".md-source");
      if (source) source.focus({ preventScroll: true });
      else editor.view.focus();
    });
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [editor, expanded]);
  // The toolbar's hairline shadow once the text scrolls under it.
  const [scrolled, setScrolled] = useState(false);
  // The field's height while the text shows over the window, so the form behind keeps its layout.
  const [held, setHeld] = useState<number | undefined>(undefined);
  const wrapper = useRef<HTMLDivElement>(null);
  const toggle = (on: boolean) => {
    mode.toggle();
    setScrolled(false);
    rememberMode(on);
    request("editor.source_mode", { on }).catch(ignore);
  };
  const surface = (inView: boolean) => (
    <MarkdownSurface
      editor={editor}
      {...(inView ? { className: "md-fullview-field" } : {})}
      toolbar={
        inView
          ? fullViewToolbar(toolbarFor(false))
          : toolbarFor(
              <ExpandButton
                label={t.expand}
                onExpand={() => {
                  setHeld(wrapper.current?.offsetHeight);
                  full.expand();
                }}
              />,
            )
      }
      {...(mode.on
        ? {
            source: <SourceArea value={body} label={t.body} takeFocus={mode.switched} onChange={onChange} />,
          }
        : {})}
    />
  );
  const toolbarFor = (expand: ReactNode) => (
    <MarkdownToolbar
      disabled={mode.on}
      expand={expand}
      sourceToggle={<SourceToggle on={mode.on} label={t.markdown} onChange={toggle} />}
    />
  );
  return (
    <>
      <div
        ref={wrapper}
        className="editor-description"
        data-scrolled={scrolled || undefined}
        style={expanded ? { height: held } : undefined}
        onScrollCapture={(e) => {
          const el = e.target as HTMLElement;
          if (el.classList.contains("md-content")) setScrolled(el.scrollTop > 0);
        }}
      >
        {!expanded && surface(false)}
      </div>
      {full.slot !== null && (
        <FullView
          slot={full.slot}
          ticketKey={view.key}
          name={t.body}
          title={title}
          state={t.editing}
          collapseLabel={t.collapse}
          onCollapse={full.collapse}
          onSave={() => {
            full.collapse();
            onSave();
          }}
          editor={surface(true)}
          footer={
            <>
              <span className="dialog-hint">
                <Kbd keys={SAVE_KEYS} />
                <span>{t.save_word}</span>
                <Kbd keys="esc" />
                <span>{t.collapse_word}</span>
              </span>
              <Button variant="ghost" label={t.cancel} onClick={full.collapse} />
              <Button
                label={t.primary}
                onClick={() => {
                  full.collapse();
                  onSave();
                }}
              />
            </>
          }
        />
      )}
    </>
  );
}

export interface FieldsProps {
  view: EditorView;
  form: EditorForm;
  onChange: (form: EditorForm) => void;
  titleMissing: boolean;
  refusal: EditorRefusal | null;
  autoFocusTitle?: boolean;
  /** Saves the form: the full-window view's Save and ⌘↵ (T-0260). */
  onSave: () => void;
}

export function EditorFields({
  view,
  form,
  onChange,
  titleMissing,
  refusal,
  autoFocusTitle = false,
  onSave,
}: FieldsProps) {
  const t = view.texts;
  const priority = shownPriority(view, form);
  const epicChild = form.epic !== null;
  const epicLabel = view.epic_menu.find((i) => i.id === (form.epic ?? ""))?.label ?? form.epic ?? t.epic_none;
  return (
    <div className="editor-fields">
      <Field label={t.title} error={titleMissing && form.title.trim() === "" ? t.title_required : null}>
        {(id) => (
          <input
            {...PLAIN_TEXT}
            id={id}
            className="text-input editor-title"
            value={form.title}
            placeholder={t.title_placeholder}
            aria-invalid={(titleMissing && form.title.trim() === "") || undefined}
            // The form opens to be typed in: the title gets the keyboard.
            // eslint-disable-next-line jsx-a11y/no-autofocus -- opened by the user's own ⌘N, Edit or ⇧↵
            autoFocus={autoFocusTitle}
            onChange={(e) => {
              onChange({ ...form, title: e.target.value });
            }}
          />
        )}
      </Field>
      <div className="editor-field">
        <div className="editor-label">
          <span>{t.priority}</span>
          <span className="editor-hint">{view.follows ?? t.priority_hint}</span>
        </div>
        <Segmented
          label={t.priority}
          options={view.priorities.map((p) => `${p.view.short} ${p.view.word}`)}
          selected={Math.max(
            view.priorities.findIndex((p) => p.number === priority),
            0,
          )}
          onSelect={(i) => {
            const picked = view.priorities[i];
            if (picked) onChange({ ...form, priority: picked.number });
          }}
        />
      </div>
      <ChoicesField view={view} form={form} onChange={onChange} />
      {view.create && (
        <div className="editor-row">
          <div className="editor-field">
            <div className="editor-label">
              <span>{t.kind}</span>
            </div>
            <Segmented
              label={t.kind}
              options={[t.kind_ticket, t.kind_epic]}
              selected={form.epic_kind ? 1 : 0}
              onSelect={(i) => {
                // An epic has no stage choices: its picks don't carry over.
                if ((i === 1) !== form.epic_kind) {
                  onChange({ ...form, epic_kind: i === 1, epic: i === 1 ? null : form.epic });
                }
              }}
            />
          </div>
          {!form.epic_kind && view.epic_menu.length > 1 && (
            <div className="editor-field editor-grow">
              <div className="editor-label">
                <span>{t.epic}</span>
                <span className="editor-hint">{t.epic_hint}</span>
              </div>
              <Select
                label={t.epic}
                value={epicLabel}
                icon="layers"
                items={view.epic_menu.map((i) => ({ ...i, checked: i.id === (form.epic ?? "") }))}
                onChange={(id) => {
                  onChange({ ...form, epic: id === "" ? null : id });
                }}
              />
            </div>
          )}
        </div>
      )}
      <div className="editor-field">
        <div className="editor-label">
          <span>{t.body}</span>
          <span className="editor-hint">{t.body_hint}</span>
        </div>
        <Description
          view={view}
          body={form.body}
          title={form.title}
          onChange={(body) => {
            onChange({ ...form, body });
          }}
          onSave={onSave}
        />
      </div>
      {!(view.create && epicChild) && (
        <div className="editor-field">
          <div className="editor-label">
            <span>{t.base}</span>
            <span className="editor-hint">{t.base_hint}</span>
          </div>
          <BaseSelect view={view} form={form} onChange={onChange} />
          <RefusalBox refusal={refusalFor(refusal, "base")} />
        </div>
      )}
      {view.external_id_shown && !epicChild && (
        <div className="editor-field">
          <div className="editor-label">
            <span>{t.external_id}</span>
            <span className="editor-hint">{t.external_id_hint}</span>
          </div>
          <input
            {...PLAIN_TEXT}
            className="text-input"
            data-mono
            value={form.external_id}
            placeholder={t.external_id_placeholder}
            aria-label={t.external_id}
            onChange={(e) => {
              onChange({ ...form, external_id: e.target.value });
            }}
          />
        </div>
      )}
      <RefusalBox refusal={refusalFor(refusal, "form")} />
    </div>
  );
}
