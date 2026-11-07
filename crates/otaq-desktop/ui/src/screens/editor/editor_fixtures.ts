// The editor's view for the tests that draw it (the form, read mode, Overview's description; T-0465): SHOP-12
// "Dark mode" in words as Rust gives them (`editor.open`).
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorReadView } from "../../generated/EditorReadView";
import type { EditorAttachmentsField } from "../../generated/EditorAttachmentsField";
import type { EditorTexts } from "../../generated/EditorTexts";
import type { EditorView } from "../../generated/EditorView";
import type { PriorityView } from "../../generated/PriorityView";

export const priority = (number: number, word: string): PriorityView => ({
  number,
  short: `P${String(number)}`,
  word,
  label: `P${String(number)} · ${word}`,
  icon: "equal",
  ink: "muted",
});

export const TEXTS: EditorTexts = {
  title: "Title",
  title_placeholder: "Short, imperative title",
  title_required: "Add a title",
  priority: "Priority",
  priority_hint: "3 Normal by default · decides start order among ready tickets",
  body: "What and why",
  plan: "Description · the plan",
  body_hint: "Markdown · paste keeps formatting",
  body_placeholder: "Context, constraints, links…",
  base: "Start from and merge into",
  base_hint: "project default · local and remote branches · locked once the ticket starts",
  base_search: { placeholder: "Search branches…", empty: "No matching branches" },
  save_word: "create",
  discard_word: "discard",
  close_word: "close",
  cancel: "Cancel",
  close: "Close",
  primary: "Create",
  create_and_start: "Create & start",
  start_word: "create & start",
  save_and_start: "Save & start",
  edit: "Edit",
  back: "Back",
  in_word: "in",
  open_terminal: "Open terminal",
  marker: "Updated by agent",
  compare: "Compare",
  before: "BEFORE",
  after: "AFTER",
  nothing_written: "Nothing written yet.",
  copy_markdown: "Copy as Markdown",
  copied: "Copied",
  expand: "Expand",
  collapse: "Collapse",
  editing: "Editing",
  collapse_word: "collapse (after closing a popover)",
  compare_view: "Show the description",
  rendered: "Rendered",
  markdown: "Markdown",
};

export const FORM: EditorForm = {
  title: "",
  body: "",
  priority: null,
  base_branch: "main",
  choices: [],
  waits_for: [],
};

/** The new-ticket form's attachments place, as `attachments_field()` words it. */
export const ATTACHMENTS_FIELD: EditorAttachmentsField = {
  label: "Attachments",
  hint: "optional",
  drop: "Drop files or paste screenshots",
  pick_title: "Attach files to the new ticket",
  posted: "Posted as your first comment · agents read it before they start",
  chips: {
    label: null,
    placeholder: "Add a comment for the next stage…",
    submit: null,
    footer: "",
    footer_icon: "message-square",
    pasted: "Pasted text",
    remove_file: "Remove the file",
    open: "Open",
    remove: "Remove the pasted text",
    attach: "Attach",
    attach_title: "Attach files to the comment",
    optional: true,
    limits: { chars: 2000, report_bytes: 20 * 1024 * 1024, report_label: "20 MB" },
    tell_agent: null,
  },
};

export const VIEW: EditorView = {
  create: true,
  key: null,
  project: "SHOP",
  project_name: "shop-web",
  heading: "New ticket",
  texts: TEXTS,
  form: FORM,
  priorities: [priority(1, "Urgent"), priority(2, "High"), priority(3, "Normal"), priority(4, "Low")].map(
    (view) => ({
      number: view.number,
      view,
    }),
  ),
  shown_priority: 3,
  base: { kind: "editable" },
  save_hint: null,
  save_warning: null,
  unsaved_warning: null,
  read: null,
  source_mode: false,
  choices: null,
  attachments: ATTACHMENTS_FIELD,
  waits_for: {
    label: "Waits for",
    hint: "optional · it starts once they merge",
    add: "Add",
    search: "Search by key or title",
    remove: "Remove",
  },
};

export const READ: EditorReadView = {
  origin: "Created by you  ·  edited by the agent · 4m ago",
  origin_agent: false,
  title: "Dark mode",
  body_markdown: "Add a toggle.\n\n## Done when\n\n- Toggle works\n\n```\nnpm test\n```",
  body_empty: false,
  priority: priority(3, "Normal"),
  priority_menu: [],
  base: "main",
  marked: ["body"],
};
