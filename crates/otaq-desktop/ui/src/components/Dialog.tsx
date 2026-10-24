// Modal dialogs (*01 · Components → Overlays & feedback → Confirm dialog*): a dimmed backdrop over the whole window
// that takes every press (a press on it cancels), drawn in the overlay host wherever the dialog is opened from, so no
// panel's stacking or a size container's frame can shrink it or let a press through to what is behind; and a raised box with a title, a body and a footer with the `esc cancel` hint. Focus moves
// in when it opens (to the default button), Tab and ⇧Tab stay inside, Esc cancels, and focus goes back where it was
// when it closes. ↵ presses the default button: confirm on an ordinary dialog, Cancel on a destructive one
// (`dialog.rs`). A dialog with a text area to send (Local merge's message) also takes ⌘↵ / Ctrl+↵ from anywhere
// inside it (`onSubmit`).
import { type ReactNode, type RefObject, useCallback, useEffect, useId, useRef } from "react";
import { createPortal } from "react-dom";

import type { IconName } from "../generated/IconName";
import { inKeyLayer } from "../markdown/keys";
import { Button } from "./Button";
import { Kbd } from "./Kbd";
import { useEnterExit } from "./motion";
import { defaultOutcome, dialogKey, type DialogOutcome, isSubmitKey, trapFocus } from "./logic";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [contenteditable="true"], [tabindex]:not([tabindex="-1"])';

/** Keeps Tab inside `root` and turns Esc, ↵ (and ⌘↵ with `onSubmit`) into the dialog's outcomes, while mounted. */
function useModalKeys(
  root: RefObject<HTMLElement | null>,
  destructive: boolean,
  onOutcome: (outcome: DialogOutcome) => void,
  onSubmit: (() => void) | undefined,
): void {
  // Focus goes back where it was when the dialog closes.
  useEffect(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    return () => {
      before?.focus();
    };
  }, []);
  useEffect(() => {
    const box = root.current;
    if (!box) return;
    const onKey = (e: KeyboardEvent) => {
      // A key something inside took (the description's Tab and Esc, d0078 D3 / D4), or one a menu or popover in the
      // dialog handles itself (its Esc closes it, not the dialog).
      if (e.defaultPrevented || inKeyLayer(e.target, box)) return;
      if (e.key === "Tab") {
        // Elements taken out of the Tab order (the description's toolbar) stay out of it here too.
        const items = [...box.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.tabIndex >= 0);
        const current = items.indexOf(document.activeElement as HTMLElement);
        items[trapFocus(items.length, current, e.shiftKey)]?.focus();
        e.preventDefault();
        return;
      }
      // ⌘↵ wherever the focus is in the dialog, its text area or a button (the page's dispatcher leaves it to a
      // dialog, d0097): handled here, not by the field, so nothing else in the dialog can keep it from sending.
      if (onSubmit && isSubmitKey(e) && !e.isComposing) {
        e.preventDefault();
        e.stopPropagation();
        onSubmit();
        return;
      }
      const target = e.target as HTMLElement | null;
      // ↵ on a button presses that button; in a text area or the description editor it is a new line.
      if (
        e.key === "Enter" &&
        (target?.closest('button, textarea, [contenteditable="true"]') || e.isComposing)
      )
        return;
      const outcome = dialogKey(destructive, e.key);
      if (outcome === null) return;
      e.preventDefault();
      // The dialog's own keys: nothing behind it acts on them.
      e.stopPropagation();
      onOutcome(outcome);
    };
    box.addEventListener("keydown", onKey);
    return () => {
      box.removeEventListener("keydown", onKey);
    };
  }, [root, destructive, onOutcome, onSubmit]);
}

export interface DialogProps {
  title: string;
  children: ReactNode;
  /** The footer's buttons, after the `esc` hint. */
  footer: ReactNode;
  /** The word after the `esc` hint: "cancel", or "close" for a dialog that only shows something. */
  hint?: string;
  /** 440 px by default. */
  width?: number;
  /** A fixed height (any CSS length); the body scrolls inside it. By default the dialog fits its content. */
  height?: string;
  /** What stays above the scrolling body, under the title: a description, a filter. */
  head?: ReactNode;
  /** More hints after the `esc` one, such as `⌘F filter`. */
  hints?: ReactNode;
  /** A destructive dialog: ↵ cancels. */
  destructive?: boolean;
  onConfirm?: () => void;
  /** ⌘↵ / Ctrl+↵ anywhere in the dialog (in its text area too, where ↵ is a new line). */
  onSubmit?: () => void;
  onCancel: () => void;
  /** The element focused when it opens; the first focusable one otherwise. */
  initialFocus?: RefObject<HTMLElement | null>;
}

const DIALOG_BOX = { selector: ".dialog", preset: "dialog" } as const;

/** The element the frame's overlay host draws into (`app/Frame.tsx`), above every panel; local menus and popovers may
 * portal there too. Absent outside the frame (a test, the gallery). */
export const OVERLAY_HOST_ID = "overlay-host";

/** `modal` in the overlay host, or in place where there is none (or no page at all: a static render). */
export function inOverlayHost(modal: ReactNode): ReactNode {
  const host = typeof document === "undefined" ? null : document.getElementById(OVERLAY_HOST_ID);
  return host === null ? modal : createPortal(modal, host);
}

/** The modal shell; `ConfirmDialog` is the common case. */
export function Dialog({
  title,
  children,
  footer,
  hint = "cancel",
  width = 440,
  height,
  head,
  hints,
  destructive = false,
  onConfirm,
  onSubmit,
  onCancel,
  initialFocus,
}: DialogProps) {
  const titleId = useId();
  const box = useRef<HTMLDivElement>(null);
  const backdrop = useRef<HTMLDivElement>(null);
  // The backdrop fades and the box scales, in and out (OT-6).
  useEnterExit(backdrop, "backdrop", DIALOG_BOX);
  const onOutcome = useCallback(
    (outcome: DialogOutcome) => {
      if (outcome === "confirm") onConfirm?.();
      else onCancel();
    },
    [onConfirm, onCancel],
  );
  useModalKeys(box, destructive, onOutcome, onSubmit);
  useEffect(() => {
    (initialFocus?.current ?? box.current?.querySelector<HTMLElement>(FOCUSABLE) ?? box.current)?.focus();
  }, [initialFocus]);
  return inOverlayHost(
    <div
      ref={backdrop}
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        ref={box}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        style={{ width, height }}
      >
        <div className="dialog-content">
          <h2 id={titleId} className="dialog-title">
            {title}
          </h2>
          {head}
          <div className="dialog-body">{children}</div>
        </div>
        <div className="dialog-footer">
          <span className="dialog-hint">
            <Kbd keys="esc" />
            <span>{hint}</span>
            {hints}
          </span>
          {footer}
        </div>
      </div>
    </div>,
  );
}

export interface ConfirmDialogProps {
  title: string;
  /** The body: a paragraph, or anything else the dialog shows. */
  body: ReactNode;
  /** `null`: no confirm button, only the cancel one ("Keep it and cancel" for a draft the tool can't clear). */
  confirmLabel: string | null;
  /** The cancel button's label; "Cancel" when not given. */
  cancelLabel?: string;
  confirmIcon?: IconName;
  /** A destructive confirm button, with Cancel as the default. */
  destructive?: boolean;
  /** Cancel as the default (focused, pressed by ↵) while the confirm button stays primary (Park all, EPC-14). */
  cancelDefault?: boolean;
  confirmDisabled?: boolean;
  /** A second action in place of the Cancel button ("Start anyway"); Esc and the backdrop still cancel. */
  secondary?: { label: string; onClick: () => void };
  /** A third button before Cancel ("Discard changes" between Keep editing and Save, Q415). */
  extra?: { label: string; onClick: () => void };
  hint?: string;
  width?: number;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Title, body, Cancel and the confirm button; the default button is focused and outlined. */
export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  cancelLabel = "Cancel",
  confirmIcon,
  destructive = false,
  cancelDefault: cancelFirst = false,
  confirmDisabled = false,
  secondary,
  extra,
  hint,
  width,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const confirm = useRef<HTMLButtonElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const only = confirmLabel === null;
  const cancelDefault = only || cancelFirst || defaultOutcome(destructive) === "cancel";
  return (
    <Dialog
      title={title}
      hint={hint}
      width={width}
      // ↵ follows the default button.
      destructive={destructive || cancelFirst || only}
      onConfirm={() => {
        if (!confirmDisabled && !only) onConfirm();
      }}
      onCancel={onCancel}
      initialFocus={cancelDefault || confirmDisabled ? cancel : confirm}
      footer={
        <>
          {extra && <Button variant="secondary" label={extra.label} onClick={extra.onClick} />}
          {secondary ? (
            <Button ref={cancel} variant="secondary" label={secondary.label} onClick={secondary.onClick} />
          ) : (
            <Button
              ref={cancel}
              variant={only ? "primary" : cancelDefault ? "secondary" : "ghost"}
              label={cancelLabel}
              onClick={onCancel}
            />
          )}
          {only ? null : (
            <Button
              ref={confirm}
              variant={destructive ? "destructive" : "primary"}
              label={confirmLabel}
              icon={confirmIcon}
              disabled={confirmDisabled}
              onClick={onConfirm}
            />
          )}
        </>
      }
    >
      {typeof body === "string" ? <p className="dialog-text">{body}</p> : body}
    </Dialog>
  );
}
