// The app frame (*24 → Row · The two views*, *Row · The Project agent*; SHB-01, d0113): the header row (also the title
// bar), the banner area across the window that pushes everything under it down, then **rail | Project agent panel
// (only while open, 420 px until its edge is dragged, pushing the board; expanded, over the board) | main area**.
// The main area holds the board (no Board | Files tabs:
// the project Files view is the toolbar's third view, FIL-01); the ticket sheet opens over its right side (SHL-05) and the Ask bar
// floats at its bottom centre. The overlay and toast hosts go over everything. Each slot is filled by the screen
// registered for it; an empty slot shows its name so the layout can be checked before its screen exists.
import {
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import "./frame.css";
import { OVERLAY_HOST_ID, playEnterWhenReady, playLeave, usePresence } from "../components";
import type { LayoutView } from "../generated/LayoutView";
import { DRAG_REGION } from "../tauri";
import { useTheme } from "../theme/theme";
import {
  agentPanelWidth,
  keepAgentPanelWidth,
  layoutView,
  loadLayout,
  mainWidth,
  type ShellLayout,
  shellLayout,
  useWindowWidth,
} from "./layout";
import { closeOverlay, useOpenOverlays } from "./overlays";
import { type OverlayId, type OverlayProps, type SlotId, useRegistries } from "./registry";
import { useSelection } from "./selection";
import { useStore } from "./store";

interface SlotProps {
  id: SlotId;
  /** Shown while no screen fills the slot. */
  placeholder?: ReactNode;
}

function Slot({ id, placeholder }: SlotProps) {
  const entry = useRegistries().slots.get(id);
  if (entry) return <entry.Component />;
  return placeholder ?? null;
}

function Placeholder({ name }: { name: string }) {
  return <div className="frame-placeholder">{name}</div>;
}

/** The open overlays, over the whole window. */
function Overlays() {
  const app = useRegistries();
  const open = useOpenOverlays();
  return (
    <>
      {open.map((overlay) => {
        const entry = app.overlays.get(overlay.id);
        if (!entry) return null;
        // Each id's component takes that id's params; `OpenOverlay` pairs them, which TypeScript can't follow here.
        const Component = entry.Component as (props: OverlayProps<OverlayId>) => ReactNode;
        return (
          <Component
            key={overlay.id}
            params={overlay.params}
            close={() => {
              closeOverlay(overlay.id);
            }}
          />
        );
      })}
    </>
  );
}

function OverlayHost() {
  return (
    <div className="frame-layer" id={OVERLAY_HOST_ID} data-slot="overlays">
      <Overlays />
    </div>
  );
}

/**
 * The ticket sheet's layer: a drawer, sliding in from the right edge once the ticket has loaded and back out when it
 * closes, staying mounted until the slide ends (OT-6, 0095). Switching tickets keeps it mounted, so the slide doesn't play again; resizing
 * changes only its width, so the drag stays instant.
 */
function PanelLayer({ open }: { open: boolean }) {
  const layer = useRef<HTMLDivElement>(null);
  const { mounted, leaving, done } = usePresence(open);
  useLayoutEffect(
    () => (mounted && layer.current ? playEnterWhenReady(layer.current, "panel") : undefined),
    [mounted],
  );
  useLayoutEffect(() => (leaving ? playLeave(layer.current, "panel", done) : undefined), [leaving, done]);
  if (!mounted) return null;
  return (
    <div ref={layer} className="frame-sheet" data-slot="sheet">
      <Slot id="sheet" placeholder={<Placeholder name="Ticket sheet" />} />
    </div>
  );
}

/**
 * The panel's right edge (T-0502): dragging it sets the panel's width between Rust's bounds, kept when the drag ends;
 * a double-click goes back to the default width. The drag itself stays on the page.
 */
function AgentEdge({
  view,
  width,
  windowWidth,
  onDrag,
}: {
  view: LayoutView;
  width: number;
  windowWidth: number;
  onDrag: (width: number | null) => void;
}) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const dragged = useRef<number | null>(null);
  const down = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    start.current = { x: e.clientX, width };
    dragged.current = null;
  };
  const move = (e: ReactPointerEvent<HTMLDivElement>) => {
    const from = start.current;
    if (!from) return;
    dragged.current = agentPanelWidth(view, windowWidth, from.width + e.clientX - from.x);
    onDrag(dragged.current);
  };
  const up = () => {
    const kept = dragged.current;
    start.current = null;
    dragged.current = null;
    if (kept === null) return;
    keepAgentPanelWidth(kept).then(
      () => {
        onDrag(null);
      },
      (error: unknown) => {
        onDrag(null);
        console.error("the panel's width couldn't be kept", error);
      },
    );
  };
  return (
    <div
      className="frame-agent-edge"
      // A pointer's handle, as the flow editor's grips: the panel's width has no key of its own.
      aria-hidden="true"
      title={view.agent_panel_edge}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onDoubleClick={() => {
        keepAgentPanelWidth(null).catch((error: unknown) => {
          console.error("the panel's width couldn't be reset", error);
        });
      }}
    />
  );
}

/**
 * The Project agent panel's column (SHB-01, d0113): the panel between the rail and the main area while it is open,
 * pushing the board, its right edge dragged to resize it; nothing while it is closed. Expanded (T-0502), the column
 * keeps its width, so the board stays where it is, and the panel lies over the board up to the window's right edge. It
 * stays mounted closed, so its terminal keeps its place.
 */
function AgentColumn({
  view,
  open,
  expanded,
  windowWidth,
}: {
  view: LayoutView;
  open: boolean;
  expanded: boolean;
  windowWidth: number;
}) {
  // The width during a drag of the edge, until Rust's view has the kept one.
  const [dragging, setDragging] = useState<number | null>(null);
  const width = dragging ?? agentPanelWidth(view, windowWidth);
  const shown = expanded && windowWidth > 0 ? Math.max(windowWidth - view.rail, 0) : width;
  return (
    <div
      className="frame-agent"
      data-slot="agent_panel"
      data-open={open || undefined}
      data-expanded={expanded || undefined}
      style={expanded ? { width } : undefined}
    >
      <div className="frame-agent-body" style={{ width: shown }} hidden={!open}>
        <Slot id="agent_panel" />
        {!expanded && <AgentEdge view={view} width={width} windowWidth={windowWidth} onDrag={setDragging} />}
      </div>
    </div>
  );
}

export interface FrameBodyProps {
  /** `undefined` until `layout.view` answered: the Project agent panel waits for it. */
  view: LayoutView | undefined;
  layout: ShellLayout;
  ticketOpen: boolean;
  bannerFallback?: ReactNode;
  /** Gets the main area's element, to measure it. */
  mainRef?: (element: HTMLDivElement | null) => void;
  /** The window's width, the panel's bound (0: none, as while the markup tests render). */
  windowWidth?: number;
}

/**
 * Everything under the hosts: header, banners, rail, the Project agent panel while open, the main area with the
 * ticket sheet and the Ask bar over it.
 */
export function FrameBody({
  view,
  layout,
  ticketOpen,
  bannerFallback,
  mainRef,
  windowWidth = 0,
}: FrameBodyProps) {
  const expanded = layout.panelExpanded && ticketOpen;
  return (
    <>
      {/* The header row is the title bar: pressing its empty parts moves the window (T-0214). */}
      <div className="frame-header" data-slot="header" {...DRAG_REGION}>
        <Slot id="title_bar_start" />
        <div className="frame-header-content">
          <Slot id="header" placeholder={<Placeholder name="Header" />} />
        </div>
        <Slot id="title_bar_end" />
      </div>
      <div className="frame-rail" data-slot="rail">
        <Slot id="rail" placeholder={<Placeholder name="Rail" />} />
      </div>
      <div className="frame-banners" data-slot="banners">
        <Slot id="banners" placeholder={bannerFallback} />
      </div>
      {view && (
        <AgentColumn
          view={view}
          open={layout.agentOpen}
          expanded={layout.agentOpen && layout.agentExpanded}
          windowWidth={windowWidth}
        />
      )}
      <div className="frame-main" ref={mainRef} data-expanded={expanded || undefined}>
        <div className="frame-board" data-slot="board">
          <Slot id="board" placeholder={<Placeholder name="Board" />} />
        </div>
        <PanelLayer open={ticketOpen} />
        {/* Over the board, the List and the ticket, at the bottom centre; it hides itself while the panel is open. */}
        <div className="frame-ask" data-slot="ask_bar">
          <Slot id="ask_bar" />
        </div>
      </div>
    </>
  );
}

export interface FrameProps {
  /** Shown in the banner area while no banners screen is registered (the service's start error, Q40). */
  bannerFallback?: ReactNode;
}

export function Frame({ bannerFallback }: FrameProps) {
  // The stored theme on the root element, swapped live on every settings change (UIT-07).
  useTheme();
  const { ticket } = useSelection();
  const layout = useStore(shellLayout);
  const view = useStore(layoutView);
  const windowWidth = useWindowWidth();
  useEffect(loadLayout, []);
  // The panel's widest is the main area minus what the board keeps, so the frame measures it.
  const observer = useRef<ResizeObserver | null>(null);
  const mainRef = useCallback((element: HTMLDivElement | null) => {
    observer.current?.disconnect();
    observer.current = null;
    if (element === null) return;
    observer.current = new ResizeObserver(() => {
      mainWidth.set(element.clientWidth);
    });
    observer.current.observe(element);
  }, []);
  return (
    <div className="frame">
      <FrameBody
        view={view}
        layout={layout}
        ticketOpen={ticket !== null}
        bannerFallback={bannerFallback}
        mainRef={mainRef}
        windowWidth={windowWidth}
      />
      <OverlayHost />
      <div className="frame-layer" data-slot="toasts">
        <Slot id="toasts" />
      </div>
    </div>
  );
}
