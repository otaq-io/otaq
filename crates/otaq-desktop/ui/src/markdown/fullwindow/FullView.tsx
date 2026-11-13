// The full-window view of a description (T-0260, TKT-12, d0078 D11), drawn from *crewboard.pen → 08 · Ticket editor →
// Full-window edit / read · Light / Dark*. Expand (Lucide "maximize") opens it over the whole Otaq window; Collapse
// ("minimize") returns. In edit mode it shows the same editor instance as the field, so the text, caret, selection,
// undo history and the Markdown toggle carry over both ways; in read mode the rendered description.
//
// It is the `full_view` overlay (slot.ts): `FullViewHost` is what the overlay host draws, an empty box over the
// window; `useFullView` opens it for one owner, and `<FullView>` draws the owner's description into it. Esc collapses it
// once no popover is open (`ESC_LAYERS`), ⌘↵ saves in edit mode. There is no shortcut to open it (d0078).
//
// Nothing here or around it animates or transforms: the link popover, the table bar and the code language picker are
// placed against the window (position: fixed), which an ancestor's transform, filter or contain would break.
import {
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useEffectEvent,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { closeOverlay, openOverlay } from "../../app/overlays";
import { type OverlayProps, RegistriesContext } from "../../app/registry";
import { useStore } from "../../app/store";
import { IconButton, inOverlayHost, Tooltip } from "../../components";
import { DRAG_REGION } from "../../tauri";
import { inKeyLayer, routeKey } from "../keys";
import { ToolButton } from "../toolbar/Toolbar";
import { fullViewSlot, slotFor } from "./slot";
import "./fullview.css";

/** The overlay host's part: the box over the window that the owner draws into. */
export function FullViewHost({ params }: OverlayProps<"full_view">) {
  const { owner } = params;
  const register = useCallback(
    (element: HTMLDivElement) => {
      fullViewSlot.set({ owner, element });
      return () => {
        if (fullViewSlot.get()?.element === element) fullViewSlot.set(null);
      };
    },
    [owner],
  );
  return <div className="md-fullview-host" ref={register} />;
}

export interface FullViewControl {
  /** The box to draw the view into while it is open for this owner; `null` while collapsed. */
  slot: HTMLElement | null;
  expand: () => void;
  collapse: () => void;
}

/** The full view for one owner (a form, the Ticket tab). It closes when the owner goes away. */
export function useFullView(): FullViewControl {
  const owner = useId();
  const slot = slotFor(useStore(fullViewSlot), owner);
  const open = slot !== null;
  useEffect(
    () => () => {
      if (fullViewSlot.get()?.owner === owner) closeOverlay("full_view");
    },
    [owner],
  );
  return {
    slot,
    expand: () => {
      openOverlay("full_view", { owner });
    },
    collapse: () => {
      if (open) closeOverlay("full_view");
    },
  };
}

/** Expand at the toolbar's right end (the toolbar's `expand` slot): like its other buttons, it keeps the caret. */
export function ExpandButton({ label, onExpand }: { label: string; onExpand: () => void }) {
  return <ToolButton label={label} icon="maximize" onPress={onExpand} />;
}

/** Esc collapses the view, once a popover or menu in it has closed; ⌘↵ saves (edit mode). */
function useViewKeys(root: React.RefObject<HTMLElement | null>, onCollapse: () => void, onSave?: () => void) {
  const collapse = useEffectEvent(onCollapse);
  const save = useEffectEvent(() => onSave?.());
  const saves = onSave !== undefined;
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const onKey = (e: KeyboardEvent) => {
      const route = routeKey(e, {
        layer: inKeyLayer(e.target, el),
        focus: "form",
        fullView: true,
        taken: e.defaultPrevented,
      });
      if (route.kind === "collapse") collapse();
      else if (route.kind === "save" && saves) save();
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    // Capturing: the view takes Esc before the text does (the text would give up focus instead, T-0259). A popover
    // takes its own Esc earlier still, on the document.
    el.addEventListener("keydown", onKey, true);
    return () => {
      el.removeEventListener("keydown", onKey, true);
    };
  }, [root, saves]);
}

export interface FullViewProps {
  /** `useFullView().slot`. */
  slot: HTMLElement;
  /** "SHOP-47"; `null` for a ticket not created yet. */
  ticketKey: string | null;
  /** "What and why". */
  name: string;
  /** The ticket's title, muted after the name. */
  title: string;
  /** "Editing", before Collapse (edit mode). */
  state?: string;
  /** Icons before Collapse: Copy as Markdown (read mode). */
  tools?: ReactNode;
  /** Collapse's tooltip and name. */
  collapseLabel: string;
  onCollapse: () => void;
  /** ⌘↵ (edit mode). */
  onSave?: () => void;
  /** Edit mode: the description field, which `fullViewToolbar` and the `md-fullview-field` class fit to the view. */
  editor?: ReactNode;
  /** Read mode: the rendered description, set in the reading column. */
  children?: ReactNode;
  /** The pinned bottom row: its hints and Save / Cancel (edit mode). */
  footer?: ReactNode;
}

/** The view, drawn into the overlay's box. */
export function FullView({
  slot,
  ticketKey,
  name,
  title,
  state,
  tools,
  collapseLabel,
  onCollapse,
  onSave,
  editor,
  children,
  footer,
}: FullViewProps) {
  const root = useRef<HTMLDivElement>(null);
  useViewKeys(root, onCollapse, onSave);
  // Focus goes back where it was (read mode's Expand) when the view closes; the edit owner puts the caret back itself.
  const [before] = useState(() =>
    document.activeElement instanceof HTMLElement ? document.activeElement : null,
  );
  useLayoutEffect(() => {
    const el = root.current;
    if (el && !el.contains(document.activeElement)) el.focus();
    return () => {
      if (before?.isConnected === true && before.tabIndex >= 0) before.focus();
    };
  }, [before]);
  const [scrolled, setScrolled] = useState(false);
  // Inside a provider only in the app: the title bar's ends (traffic lights, caption buttons) are its screen's.
  const app = useContext(RegistriesContext);
  const Start = app?.slots.get("title_bar_start")?.Component;
  const End = app?.slots.get("title_bar_end")?.Component;
  const heading = ticketKey === null ? name : `${ticketKey} · ${name}`;
  return inOverlayHost(
    <div
      ref={root}
      className="md-fullview"
      role="dialog"
      aria-modal="true"
      aria-label={heading}
      tabIndex={-1}
      data-mode={editor === undefined ? "read" : "edit"}
      data-scrolled={scrolled || undefined}
      onScrollCapture={(e) => {
        const el = e.target as HTMLElement;
        if (el.classList.contains("md-content")) setScrolled(el.scrollTop > 0);
      }}
    >
      <div className="md-fullview-header" {...DRAG_REGION}>
        {Start && <Start />}
        {ticketKey !== null && (
          <>
            <span className="md-fullview-key">{ticketKey}</span>
            <span className="md-fullview-dot">·</span>
          </>
        )}
        <span className="md-fullview-name" data-first={ticketKey === null || undefined}>
          {name}
        </span>
        <span className="md-fullview-title">{title}</span>
        <span className="md-fullview-spacer" />
        {state !== undefined && <span className="md-fullview-state">{state}</span>}
        {tools}
        <Tooltip text={collapseLabel}>
          <IconButton icon="minimize" label={collapseLabel} onClick={onCollapse} />
        </Tooltip>
        {End && <End />}
      </div>
      {editor ?? (
        <div className="md-fullview-read">
          <div className="md-fullview-column">{children}</div>
        </div>
      )}
      {footer !== undefined && <div className="md-fullview-footer">{footer}</div>}
    </div>,
    slot,
  );
}

/** The toolbar as the view pins it: in a band across the window, the toolbar itself as wide as the column. */
export function fullViewToolbar(toolbar: ReactNode): ReactNode {
  return <div className="md-fullview-band">{toolbar}</div>;
}
