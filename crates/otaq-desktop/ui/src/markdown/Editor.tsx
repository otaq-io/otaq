// The description editor (T-0249, d0078): one Tiptap editor for every description in the app, editing it in the form
// and the Ticket tab and showing it in read mode (`editable={false}`). The stored value is Markdown both ways.
//
// It comes in two layers:
// - `useMarkdownEditor` owns the Tiptap instance: loading `value`, reporting `onChange`, editable, placeholder, label and
//   the `onKeyDown` pass-through. A screen that shows the same editor in two places (the full-window view, T-0260) calls
//   it once and keeps the instance: text, caret and undo travel with it.
// - `MarkdownSurface` draws an instance: the field (in edit mode), the toolbar slot and the content. `MarkdownEditor` is
//   both together, for the screens that show one editor in one place.
import type { Editor, EditorEvents } from "@tiptap/core";
import { EditorContent, EditorContext, useEditor } from "@tiptap/react";
import { type ReactNode, type RefObject, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { ChangeTracker, loadMarkdown, serializeMarkdown } from "./changes";
import { markdownExtensions } from "./extensions";
import { focusPast, inKeyLayer, routeKey } from "./keys";
import { LinkLayer } from "./link/LinkLayer";
import "./markdown.css";
import { PasteNote } from "./paste/PasteNote";
import { SlashMenu } from "./slash/SlashMenu";
import { TableBar } from "./table/TableBar";

export interface MarkdownEditorOptions {
  /**
   * The description as Markdown. The editor is controlled: the parent stores what `onChange` reports and passes it back.
   * A `value` the editor didn't report (another ticket, an agent's update) is loaded in its place, without an undo step.
   */
  value: string;
  /**
   * Called with the Markdown after the user changes the text. Never called for a description nobody edited, and a text
   * brought back to what was loaded (typed, then undone) reports `value`'s original string, byte for byte.
   */
  onChange?: (markdown: string) => void;
  /** Off for read mode: nothing can be typed and task boxes don't react. Default on. */
  editable?: boolean;
  /** The muted text shown while the description is empty (edit mode only). */
  placeholder?: string;
  /** The accessible name of the text ("What and why"). */
  label: string;
  /**
   * Sees every key pressed in the text before the editor does. Return `true`, or call `preventDefault()`, when the key
   * is handled: the editor then leaves it alone. The event still bubbles to the page. Tab, Esc and ⌘↵ are already the
   * editor's own (`textKey`): ⌘↵ goes on to the form without adding a line break.
   */
  onKeyDown?: (event: KeyboardEvent) => boolean | undefined;
}

/** The Tiptap instance for one description; it stays the same for the life of the calling component. */
export function useMarkdownEditor({
  value,
  onChange,
  editable = true,
  placeholder = "",
  label,
  onKeyDown,
}: MarkdownEditorOptions): Editor {
  // The latest callbacks, read by the editor's handlers, so the instance never has to be rebuilt for them.
  const latest = useRef({ onChange, onKeyDown, placeholder });
  useLayoutEffect(() => {
    latest.current = { onChange, onKeyDown, placeholder };
  });
  // eslint-disable-next-line react-hooks/refs -- the ref is read each time the placeholder is drawn, never during render
  const [extensions] = useState(() => markdownExtensions({ placeholder: () => latest.current.placeholder }));
  // The instance, for the key handler below (it is created after its props).
  const instance = useRef<Editor | null>(null);
  const editorProps = useMemo(
    () => ({
      // Prose: no autocorrect or capitals (#41), misspellings still underlined; code blocks turn spell check off.
      attributes: {
        "aria-label": label,
        "aria-multiline": "true",
        "aria-readonly": String(!editable),
        autocorrect: "off",
        autocapitalize: "off",
      },
      handleKeyDown: (_view: unknown, event: KeyboardEvent) =>
        latest.current.onKeyDown?.(event) === true ||
        event.defaultPrevented ||
        (instance.current !== null && textKey(instance.current, event)),
    }),
    [editable, label],
  );
  // Created empty: `value` is loaded below, the same way as every later value.
  const editor = useEditor({
    extensions,
    editable,
    editorProps,
    immediatelyRender: true,
    shouldRerenderOnTransaction: false,
  });
  useLayoutEffect(() => {
    instance.current = editor;
  }, [editor]);

  // What the parent holds, and how the loaded description saves untouched (set when it's loaded).
  const [tracker] = useState(() => new ChangeTracker(value, value));
  const loadedInto = useRef<Editor | null>(null);

  // Loads `value` into a new instance, and any value the editor didn't report itself. Before paint, and before the
  // user can touch anything, so the baseline is the untouched document.
  useLayoutEffect(() => {
    if (loadedInto.current === editor && value === tracker.value) return;
    loadedInto.current = editor;
    loadMarkdown(editor, value);
    tracker.reload(value, serializeMarkdown(editor));
  }, [editor, tracker, value]);

  useEffect(() => {
    const onUpdate = ({ editor: changed, transaction }: EditorEvents["update"]) => {
      // Only a change the user made counts. The editor tidying its own document (the trailing paragraph it adds after
      // the first click, a table fixed up) comes as an appended transaction to one that changed nothing.
      if (!transaction.docChanged) return;
      const next = tracker.report(serializeMarkdown(changed));
      if (next !== null) latest.current.onChange?.(next);
    };
    editor.on("update", onUpdate);
    return () => {
      editor.off("update", onUpdate);
    };
  }, [editor, tracker]);

  useEffect(() => {
    editor.setEditable(editable, false);
  }, [editor, editable]);

  return editor;
}

/** The list item or code block the caret is in, the innermost one. */
function blockAt(editor: Editor): { block: "list" | "code"; item: string } | null {
  const { $from } = editor.state.selection;
  for (let depth = $from.depth; depth > 0; depth--) {
    const name = $from.node(depth).type.name;
    if (name === "codeBlock") return { block: "code", item: name };
    if (name === "listItem" || name === "taskItem") return { block: "list", item: name };
  }
  return null;
}

/** Tab, Esc and ⌘↵ in the editor's text (`routeKey`); whether the key was taken. */
function textKey(editor: Editor, event: KeyboardEvent): boolean {
  if (!editor.isEditable) return false;
  const at = blockAt(editor);
  const route = routeKey(event, { layer: false, focus: "text", block: at?.block ?? null });
  const field = editor.view.dom.closest<HTMLElement>(".md-editor");
  switch (route.kind) {
    case "indent":
      // Taken even when the item can't move (the first item can't nest), so Tab never leaves from a list.
      if (at !== null && route.out) editor.commands.liftListItem(at.item);
      else if (at !== null) editor.commands.sinkListItem(at.item);
      return true;
    case "leave":
      field?.focus();
      return true;
    case "focus":
      if (field !== null) focusPast(field, route.back);
      return true;
    case "save":
      // The form's `useFormKeys` saves; Tiptap would add a line break.
      return true;
    case "code":
    case "enter":
    case "discard":
    case "collapse":
    case "none":
      return false;
  }
}

/**
 * The field's own keys, for what has focus inside it apart from the text: the field itself after Esc (Tab leaves, ↵
 * goes back into the text, Esc goes on to the form) and a text area in it (the Markdown source view: Esc leaves it,
 * Tab moves on). Popovers and menus in it keep their keys.
 */
function useFieldKeys(editor: Editor, field: RefObject<HTMLDivElement | null>) {
  // In edit mode the field is focusable but out of the Tab order: Esc in the text moves focus here (d0078 D4).
  useEffect(() => {
    const el = field.current;
    if (!el) return;
    if (editor.isEditable) el.setAttribute("tabindex", "-1");
    else el.removeAttribute("tabindex");
  });
  useEffect(() => {
    const el = field.current;
    if (!el) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !editor.isEditable) return;
      const target = e.target instanceof HTMLElement ? e.target : null;
      if (target === null || editor.view.dom.contains(target) || inKeyLayer(target, el)) return;
      const focus = target === el ? "field" : target.matches("textarea, input") ? "text" : null;
      if (focus === null) return;
      const route = routeKey(e, { layer: false, focus });
      if (route.kind === "leave") el.focus();
      else if (route.kind === "focus") focusPast(el, route.back);
      // ↵ goes back to the text shown: the Markdown source while it shows (T-0251), else the editor.
      else if (route.kind === "enter") (el.querySelector<HTMLElement>(".md-source") ?? editor.view).focus();
      else return;
      e.preventDefault();
    };
    el.addEventListener("keydown", onKey);
    return () => {
      el.removeEventListener("keydown", onKey);
    };
  }, [editor, field]);
}

interface MarkdownSurfaceProps {
  editor: Editor;
  /** Drawn inside the field, above the text, in edit mode: the toolbar (T-0250). */
  toolbar?: ReactNode;
  /** Drawn in place of the text while given: the Markdown source (T-0251). The text stays mounted, hidden. */
  source?: ReactNode;
  className?: string;
}

/**
 * Draws a description editor. In edit mode it is a field (border, focus ring, padding); in read mode it is the text
 * alone, set in the label row's column. Everything inside it can reach the instance with Tiptap's `useCurrentEditor`.
 */
export function MarkdownSurface({ editor, toolbar, source, className }: MarkdownSurfaceProps) {
  const context = useMemo(() => ({ editor }), [editor]);
  const field = useRef<HTMLDivElement>(null);
  useFieldKeys(editor, field);
  return (
    <EditorContext.Provider value={context}>
      <div
        ref={field}
        className={className === undefined ? "md-editor" : `md-editor ${className}`}
        data-source={source === undefined ? undefined : true}
      >
        {toolbar}
        <EditorContent editor={editor} className="md-content" />
        {source}
        <PasteNote />
        <TableBar />
        <LinkLayer />
        <SlashMenu />
      </div>
    </EditorContext.Provider>
  );
}

export interface MarkdownEditorProps extends MarkdownEditorOptions {
  /** Drawn inside the field, above the text, in edit mode: the toolbar (T-0250). */
  toolbar?: ReactNode;
  className?: string;
}

/** `<MarkdownEditor>`: a description, editable or read-only, in one place. */
export function MarkdownEditor({ toolbar, className, ...options }: MarkdownEditorProps) {
  const editor = useMarkdownEditor(options);
  return (
    <MarkdownSurface
      editor={editor}
      {...(toolbar === undefined ? {} : { toolbar })}
      {...(className === undefined ? {} : { className })}
    />
  );
}
