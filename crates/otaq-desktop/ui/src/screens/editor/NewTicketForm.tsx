// The full new-ticket form (*New ticket · More options… · the full form* and *A ticket’s own stages*): what the box
// for new work's More options… opens with what was typed. Title; Description with its line under it; Priority (a
// select, P3 · Normal until picked) beside Waits for (a search that opens the project's tickets); Attachments, several
// files of any type (tuq 001, 002) dropped on the form, pasted (a screenshot, files copied in Finder) or picked with a
// click, each a chip with ✕, a file over 20 MB refused by name when it is added (tuq 005); then two folds: "Stages for
// this ticket" (a picker per stage: "Uses the stage’s agent", another agent, or "Skip this stage" only where the flow
// lets tickets skip it; only you set them) and "Base branch" with the branch it starts from. Create posts the
// attachments as the ticket's first comment, yours. The words and rules are Rust's (`EditorView`).
import {
  type ClipboardEvent,
  type ReactNode,
  type RefObject,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";

import { PLAIN_TEXT, Popover, Select, StageChoices } from "../../components";
import type { DependencySearchRow } from "../../generated/DependencySearchRow";
import type { EditorAttachmentsField } from "../../generated/EditorAttachmentsField";
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorRefusal } from "../../generated/EditorRefusal";
import type { EditorView } from "../../generated/EditorView";
import type { MenuItem } from "../../generated/MenuItem";
import { pickAttachmentFiles } from "../../tauri";
import { dropTargets } from "../../terminal/drops";
import { Icon } from "../../theme/Icon";
import { LinkedRow, PickerBody, useWaitSearch } from "../dependencies/Dependencies";
import { type Added, addFiles, addPaths, clipboardFiles, thumbnailOf } from "./attachmentAdding";
import { FileChip } from "./CommentBox";
import { type BoxValue, EMPTY_BOX, withAdded, withoutAttachment, withThumb } from "./commentRules";
import { BaseSelect, Description, RefusalBox } from "./Fields";
import { refusalFor, shownPriority } from "./logic";
import "./new_ticket.css";

/** The paths the form sends for what the attachments place holds, in its order (every chip here is a file). */
export function attachmentPaths(value: BoxValue): string[] {
  return value.attachments.flatMap((a) => (a.kind === "file" ? [a.path] : []));
}

/** The priorities as the select's items ("P3 · Normal"), the shown one checked. */
export function priorityItems(view: EditorView, shown: number): MenuItem[] {
  return view.priorities.map((p) => ({
    id: String(p.number),
    label: p.view.label,
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked: p.number === shown,
    header: false,
  }));
}

export interface FormAttachments {
  value: BoxValue;
  /** Why the last files added couldn't be (over 20 MB, gone), each naming its file. */
  refused: readonly string[];
  /** Files by their paths (the picker, a drop). */
  addPaths: (paths: readonly string[]) => void;
  /** A paste with files (a screenshot, files copied in Finder) anywhere in the form adds them. */
  onPaste: (e: ClipboardEvent<HTMLElement>) => void;
  remove: (id: number) => void;
}

/**
 * The new-ticket form's attachments: files dropped on `root` (the form) or pasted in it, or picked; `onPaths` gets
 * the form's paths after each change. Images get their thumbnail once read.
 */
export function useFormAttachments(
  root: RefObject<HTMLElement | null>,
  enabled: boolean,
  onPaths: (paths: string[]) => void,
): FormAttachments {
  const [value, setValue] = useState<BoxValue>(EMPTY_BOX);
  const [refused, setRefused] = useState<readonly string[]>([]);
  // Files arrive after a moment (a request, a picker): they join what the place holds then.
  const latest = useRef(value);
  const change = useCallback(
    (next: BoxValue) => {
      const paths = attachmentPaths(latest.current).join("\n");
      latest.current = next;
      setValue(next);
      if (attachmentPaths(next).join("\n") !== paths) onPaths(attachmentPaths(next));
    },
    [onPaths],
  );
  const take = useCallback(
    ({ added, refused: why }: Added) => {
      change(withAdded(latest.current, added));
      setRefused(why);
    },
    [change],
  );
  const add = useCallback(
    (paths: readonly string[]) => {
      if (paths.length > 0) void addPaths(paths).then(take);
    },
    [take],
  );
  useEffect(() => {
    const el = root.current;
    if (!el || !enabled) return;
    return dropTargets.register(el, {
      takesTyping: () => false,
      paste: () => undefined,
      takeFiles: add,
    });
  }, [root, enabled, add]);
  // Each image's thumbnail, read once from its file.
  const drawing = useRef(new Set<number>());
  useEffect(() => {
    for (const a of value.attachments) {
      if (a.kind !== "file" || !a.image || a.thumb !== null || drawing.current.has(a.id)) continue;
      drawing.current.add(a.id);
      thumbnailOf(a.path).then(
        (thumb) => {
          change(withThumb(latest.current, a.id, thumb));
        },
        () => undefined,
      );
    }
  }, [value.attachments, change]);
  const onPaste = (e: ClipboardEvent<HTMLElement>) => {
    const files = clipboardFiles(e.clipboardData);
    // The description's editor takes what it takes itself first.
    if (!enabled || files.length === 0 || e.defaultPrevented) return;
    e.preventDefault();
    void addFiles(files).then(take);
  };
  return {
    value,
    refused,
    addPaths: add,
    onPaste,
    remove: (id) => {
      change(withoutAttachment(latest.current, id));
      setRefused([]);
    },
  };
}

/** "Attachments · optional": the place to drop, paste or pick files (a click opens the picker), then the chips. */
export function AttachmentsPlace({
  field,
  attachments,
}: {
  field: EditorAttachmentsField;
  attachments: Pick<FormAttachments, "value" | "refused" | "addPaths" | "remove">;
}) {
  const { value, refused } = attachments;
  return (
    <div className="editor-field">
      <div className="editor-label">
        <span>{field.label}</span>
        <span className="editor-hint">{field.hint}</span>
      </div>
      <button
        type="button"
        className="new-ticket-drop"
        onClick={() => {
          pickAttachmentFiles(field.pick_title).then(attachments.addPaths, () => undefined);
        }}
      >
        <Icon name="paperclip" size={14} />
        <span>{field.drop}</span>
      </button>
      {value.attachments.length > 0 && (
        <div className="comment-box-chips">
          {value.attachments.map((a) =>
            a.kind === "file" ? (
              <FileChip
                key={a.id}
                file={a}
                view={field.chips}
                onRemove={() => {
                  attachments.remove(a.id);
                }}
              />
            ) : null,
          )}
        </div>
      )}
      {refused.map((why) => (
        <span key={why} className="field-note" data-error role="alert">
          {why}
        </span>
      ))}
      {value.attachments.length > 0 && <span className="field-note">{field.posted}</span>}
    </div>
  );
}

/**
 * "Waits for": a search that opens the project's tickets under it; each picked one a row with its ✕. The keys go with
 * Create, which links them; a link the service refuses shows under the form, the ticket created.
 */
function WaitsForSearch({
  view,
  form,
  onChange,
}: {
  view: EditorView;
  form: EditorForm;
  onChange: (form: EditorForm) => void;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  // The titles of the picked tickets, as the search showed them.
  const [titles, setTitles] = useState<Readonly<Record<string, string>>>({});
  const rows = useWaitSearch(open, { mode: "new", project: view.project, picked: form.waits_for }, query);
  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
  }, []);
  const field = view.waits_for;
  if (field === null) return null;
  const pick = (row: DependencySearchRow) => {
    setTitles((t) => ({ ...t, [row.key]: row.title }));
    onChange({ ...form, waits_for: [...form.waits_for, row.key] });
    close();
  };
  return (
    <div className="editor-field new-ticket-waits">
      <div className="editor-label">
        <span>{field.label}</span>
        <span className="editor-hint">{field.hint}</span>
      </div>
      <button
        ref={anchor}
        type="button"
        className="select new-ticket-search"
        aria-label={`${field.label}: ${field.add}`}
        aria-expanded={open}
        onClick={() => {
          if (open) close();
          else setOpen(true);
        }}
      >
        <span className="select-value">
          <Icon name="search" size={13} />
          <span>{field.search}</span>
        </span>
        <Icon name="chevron-down" size={14} />
      </button>
      {form.waits_for.map((key) => (
        <LinkedRow
          key={key}
          row={{ key, title: titles[key] ?? "" }}
          remove={field.remove}
          opens={false}
          onRemove={() => {
            onChange({ ...form, waits_for: form.waits_for.filter((k) => k !== key) });
          }}
        />
      ))}
      {open && (
        <Popover label={field.add} anchor={anchor} align="start" onClose={close}>
          <PickerBody
            search={field.search}
            rule={null}
            query={query}
            onQuery={setQuery}
            rows={rows}
            refusal={null}
            onPick={pick}
          />
        </Popover>
      )}
    </div>
  );
}

/** A fold: its chevron and heading, the words beside it, and what it holds once opened. */
function Fold({
  heading,
  aside,
  mono = false,
  open,
  onToggle,
  children,
}: {
  heading: string;
  aside: string | null;
  mono?: boolean;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div className="new-ticket-fold" data-open={open || undefined}>
      <button type="button" className="new-ticket-fold-head" aria-expanded={open} onClick={onToggle}>
        <Icon name={open ? "chevron-down" : "chevron-right"} size={14} />
        <span className="new-ticket-fold-heading">{heading}</span>
        {aside !== null && (
          <span className="new-ticket-fold-aside" data-mono={mono || undefined}>
            {aside}
          </span>
        )}
      </button>
      {open && <div className="new-ticket-fold-body">{children}</div>}
    </div>
  );
}

export interface NewTicketFieldsProps {
  view: EditorView;
  form: EditorForm;
  onChange: (form: EditorForm) => void;
  titleMissing: boolean;
  refusal: EditorRefusal | null;
  attachments: FormAttachments;
  /** Saves the form: the full-window view's Save. */
  onSave: () => void;
}

export function NewTicketFields({
  view,
  form,
  onChange,
  titleMissing,
  refusal,
  attachments,
  onSave,
}: NewTicketFieldsProps) {
  const t = view.texts;
  const priority = shownPriority(view, form);
  const [stagesOpen, setStagesOpen] = useState(false);
  const [baseOpen, setBaseOpen] = useState(false);
  const baseRefused = refusalFor(refusal, "base");
  const titleError = titleMissing && form.title.trim() === "";
  const picks = Object.fromEntries(form.choices.map((c) => [c.stage_id, c.choice]));
  const titleId = useId();
  return (
    <div className="editor-fields new-ticket-fields">
      <div className="editor-field">
        <label className="editor-label" htmlFor={titleId}>
          {t.title}
        </label>
        <input
          {...PLAIN_TEXT}
          id={titleId}
          className="text-input editor-title"
          value={form.title}
          placeholder={t.title_placeholder}
          aria-invalid={titleError || undefined}
          // The form opens to be typed in: the title gets the keyboard.
          // eslint-disable-next-line jsx-a11y/no-autofocus -- opened by the user's own More options…
          autoFocus
          onChange={(e) => {
            onChange({ ...form, title: e.target.value });
          }}
        />
        {titleError && (
          <span className="field-note" data-error>
            {t.title_required}
          </span>
        )}
      </div>
      <div className="editor-field">
        <div className="editor-label">{t.body}</div>
        <Description
          view={view}
          body={form.body}
          title={form.title}
          onChange={(body) => {
            onChange({ ...form, body });
          }}
          onSave={onSave}
        />
        <span className="field-note">{t.body_hint}</span>
      </div>
      <div className="new-ticket-row">
        <div className="editor-field new-ticket-priority">
          <div className="editor-label">{t.priority}</div>
          <Select
            label={t.priority}
            value={view.priorities.find((p) => p.number === priority)?.view.label ?? ""}
            items={priorityItems(view, priority)}
            onChange={(id) => {
              onChange({ ...form, priority: Number(id) });
            }}
          />
        </div>
        <WaitsForSearch view={view} form={form} onChange={onChange} />
      </div>
      {view.attachments !== null && <AttachmentsPlace field={view.attachments} attachments={attachments} />}
      {view.choices !== null && (
        <Fold
          heading={view.choices.heading}
          aside={view.choices.hint}
          open={stagesOpen}
          onToggle={() => {
            setStagesOpen((o) => !o);
          }}
        >
          <div className="new-ticket-stages">
            <StageChoices
              view={view.choices}
              look="form"
              headless
              picks={picks}
              onPick={(stage, choice) => {
                onChange({
                  ...form,
                  choices: [...form.choices.filter((c) => c.stage_id !== stage), { stage_id: stage, choice }],
                });
              }}
            />
          </div>
        </Fold>
      )}
      <Fold
        heading={t.base}
        aside={form.base_branch}
        mono
        open={baseOpen || baseRefused !== null}
        onToggle={() => {
          setBaseOpen((o) => !o);
        }}
      >
        <div className="new-ticket-base">
          <BaseSelect view={view} form={form} onChange={onChange} />
          <span className="field-note">{t.base_hint}</span>
          <RefusalBox refusal={baseRefused} />
        </div>
      </Fold>
      <RefusalBox refusal={refusalFor(refusal, "form")} />
    </div>
  );
}
