// The project rail (UI-01, ATT-10, PRJ-01, SHB-02; d0024, d0058, d0087, d0113; *02 → One project, Another project
// needs you, Ten projects · long names*, *24 → Row · The Project agent*): every project in the user's order with its
// agents working and items needing you, quiet when idle; a project that newly needs you while another is shown rings
// for 10 s. The 64 px column of tiles at every width. Add project after the tiles; at the bottom the Project agent
// button (tinted while its panel is open, a dot while a reply waits; ⌘L), then Terminals (the drawer on the last-used
// terminal, as ⌃`), Cleanup and Settings (as ⌘,) (T-0329, T-0458). A project in Flow setup is marked "Flow setup" under its name (ASK-04,
// T-0436); selecting it opens its Flow setup page. Rows, order and wording are `shell.rail`'s.
import { useCallback, useEffect, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { type Registries, useRegistries } from "../../app/registry";
import { selectProject, useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { AnchoredMenu, type Anchor, Tooltip } from "../../components";
import type { CleanupBroom } from "../../generated/CleanupBroom";
import type { RailButton } from "../../generated/RailButton";
import type { RailItem } from "../../generated/RailItem";
import type { RailView } from "../../generated/RailView";
import type { ShellParams } from "../../generated/ShellParams";
import type { IconName } from "../../generated/IconName";
import type { AskAgentButton } from "../../generated/AskAgentButton";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { askBar } from "../ask_bar/logic";
import { toggleAgentPanel } from "../pane/logic";
import { cleanupScans } from "../recovery/logic";
import { railMenuChoice } from "./projectMenu";

/** The service events that change the rail's projects or counts. */
export const RAIL_EVENTS = [
  "projects_changed",
  "ticket_changed",
  "ticket_deleted",
  "session_state",
  "inbox_changed",
  // The user's keys in its tooltips (KEY-08).
  "settings_changed",
] as const;

function Broom({ broom }: { broom: CleanupBroom | undefined }) {
  const count = broom?.count ?? 0;
  return (
    <Tooltip text={broom?.tooltip ?? "Cleanup"} detail={broom ? [broom.detail] : undefined}>
      <button
        type="button"
        className="rail-broom"
        aria-label={broom?.tooltip ?? "Cleanup"}
        onClick={() => {
          openOverlay("cleanup");
        }}
      >
        <span className="rail-broom-icon">
          <Icon name="brush-cleaning" size={16} />
          {count > 0 && <span className="rail-broom-dot" />}
        </span>
      </button>
    </Tooltip>
  );
}

function choose(item: RailItem) {
  selectProject(item.key);
}

function addProject() {
  openOverlay("add_project");
}

/** What the rail's bottom buttons do: the same as their keys, so the drawer opens on the same terminal. */
export interface RailActions {
  onTerminals: () => void;
  onSettings: () => void;
}

export function railActions(app: Registries): RailActions {
  return {
    onTerminals: () => app.shortcuts.run({ action: "terminals" }),
    onSettings: () => app.shortcuts.run({ action: "settings" }),
  };
}

/** A button at the rail's bottom (a plain function, so the rail's elements show its click without rendering). */
function bottomButton(button: RailButton, icon: IconName, onClick: () => void) {
  return (
    <Tooltip key={button.label} text={button.label} keys={button.keys ?? undefined}>
      <button type="button" className="rail-bottom" aria-label={button.label} onClick={onClick}>
        <Icon name={icon} size={16} />
      </button>
    </Tooltip>
  );
}

/** The Project agent button: opens the panel (and focuses it), or closes it; its look is the Ask bar's view's. */
export function RailAgentButton({ button }: { button: AskAgentButton }) {
  return (
    <Tooltip text={button.label} detail={[button.hint]}>
      <button
        type="button"
        className="rail-agent"
        aria-label={button.label}
        aria-pressed={button.tinted}
        data-tinted={button.tinted || undefined}
        onClick={() => {
          toggleAgentPanel();
        }}
      >
        <Icon name={button.icon} size={17} />
        {button.dot && <span className="rail-agent-dot" />}
      </button>
    </Tooltip>
  );
}

export interface RailProps {
  view: RailView;
  broom?: CleanupBroom | undefined;
  actions?: RailActions;
  /** The open project's Project agent button; none before its Ask bar loaded. */
  agent?: AskAgentButton | undefined;
  /** The project menu open from a right-click on a tile (#11), and its owner's setter. */
  menu?: TileMenu | null;
  onMenu?: (menu: TileMenu | null) => void;
}

/** A right-click on a tile: that project's menu at the pointer (#11). */
export interface TileMenu {
  project: RailItem;
  anchor: Anchor;
}

/** The 64 px rail of tiles. */
export function RailTiles({ view, broom, actions, agent, menu = null, onMenu }: RailProps) {
  const setMenu = (next: TileMenu | null) => onMenu?.(next);
  return (
    <nav className="rail" aria-label="Projects">
      {view.items.map((item) => (
        <Tooltip key={item.key} text={item.title} detail={item.detail} note={item.note ?? undefined}>
          <button
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ project: item, anchor: { left: e.clientX, top: e.clientY, width: 0, height: 0 } });
            }}
            type="button"
            className="rail-item"
            data-selected={item.selected || undefined}
            data-idle={item.idle || undefined}
            data-flow-setup={item.flow_setup !== null || undefined}
            aria-current={item.selected || undefined}
            aria-label={item.title}
            onClick={() => {
              choose(item);
            }}
          >
            {item.selected && <span className="rail-bar" />}
            <span className="rail-tile" data-ringing={item.ringing || undefined}>
              {item.key}
              {item.needs_you > 0 && <span className="rail-needs">{item.needs_you}</span>}
              {item.paused !== null ? (
                // A paused project's badge counts what Resume resumes, in amber (PAU-08, `s24-08`).
                <span className="rail-working" data-paused>
                  <Icon name="activity" size={8} />
                  {item.paused > 0 && item.paused}
                </span>
              ) : (
                item.agents_working > 0 && (
                  <span className="rail-working">
                    <Icon name="activity" size={8} />
                    {item.agents_working}
                  </span>
                )
              )}
            </span>
            <span className="rail-name">{item.name}</span>
            {item.flow_setup !== null && <span className="rail-flow-setup">{item.flow_setup}</span>}
          </button>
        </Tooltip>
      ))}
      <Tooltip text="Add project">
        <button type="button" className="rail-add" aria-label="Add project" onClick={addProject}>
          <Icon name="plus" size={16} />
        </button>
      </Tooltip>
      <div className="rail-fill" />
      {agent && (
        <>
          <RailAgentButton button={agent} />
          <div className="rail-rule" />
        </>
      )}
      {bottomButton(view.bottom.terminals, "square-terminal", () => actions?.onTerminals())}
      <Broom broom={broom} />
      {bottomButton(view.bottom.settings, "settings", () => actions?.onSettings())}
      {menu !== null && (
        <AnchoredMenu
          anchor={menu.anchor}
          label={menu.project.title}
          head={{ title: menu.project.name, detail: menu.project.path }}
          items={view.project_menu}
          width={240}
          onChoose={(id) => {
            setMenu(null);
            railMenuChoice(menu.project.key, id);
          }}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
    </nav>
  );
}

/** The rail slot: follows the service and keeps the page's selection on a project that exists. */
export function ShellRail() {
  const app = useRegistries();
  const { project } = useSelection();
  const [tick, setTick] = useState(0);
  const load = useCallback(async () => {
    const params: ShellParams = { project };
    return { asked: project, rail: (await request("shell.rail", params)) as RailView };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new tick is a new load: a ring that is over stops drawing
  }, [project, tick]);
  const { view } = useServiceView(load, RAIL_EVENTS);
  // Asked again after each Cleanup scan (T-0235): deleting leftovers takes the dot away.
  const scans = useStore(cleanupScans);
  const loadBroom = useCallback(
    () => request("shell.cleanup") as Promise<CleanupBroom>,
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new scan is a new load
    [scans],
  );
  const broom = useServiceView(loadBroom, []).view;
  const agent = useStore(askBar)?.button;
  const [menu, setMenu] = useState<TileMenu | null>(null);

  // The first project when nothing is selected yet, or when the shown one was removed. Only an answer for the
  // selection now counts: one asked before a click would switch back.
  useEffect(() => {
    if (view && view.asked === project && view.rail.selected !== null && view.rail.selected !== project) {
      selectProject(view.rail.selected);
    }
  }, [view, project]);

  const ringEnds = view?.rail.ring_ends_in_ms;
  useEffect(() => {
    if (ringEnds === undefined || ringEnds === null) return;
    const timer = window.setTimeout(() => {
      setTick((n) => n + 1);
    }, ringEnds + 50);
    return () => {
      window.clearTimeout(timer);
    };
  }, [ringEnds]);

  if (view === undefined) return null;
  return (
    <RailTiles
      view={view.rail}
      broom={broom}
      actions={railActions(app)}
      agent={agent}
      menu={menu}
      onMenu={setMenu}
    />
  );
}
