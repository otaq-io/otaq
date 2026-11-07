// The project rail (UI-01, ATT-10, PRJ-01, SHB-02; d0024, d0058, d0087, d0113): every project in the user's order with
// its agents working and items needing you, quiet when idle; a project that newly needs you while another is shown
// rings for 10 s. The 64 px column of tiles at every width. Add project after the tiles; at its foot Noyan (tinted
// while its panel is open, a dot while a reply waits; ⌘L), Terminal (the drawer on the last-used terminal, as ⌃`)
// and the app-settings gear (as ⌘,), which has a dot while an update is ready (UPD-04), its tooltip saying which.
// Cleanup lives in Settings (Data), not on the rail. A project in Flow setup is marked "Flow setup" under its name
// (ASK-04, T-0436); selecting it opens its Flow setup page. Rows, order and wording are `shell.rail`'s.
import { useCallback, useEffect, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { type Registries, useRegistries } from "../../app/registry";
import { shellLayout } from "../../app/layout";
import { selectProject, useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { AnchoredMenu, type Anchor, Tooltip } from "../../components";
import type { RailButton } from "../../generated/RailButton";
import type { RailItem } from "../../generated/RailItem";
import type { RailView } from "../../generated/RailView";
import type { ShellParams } from "../../generated/ShellParams";
import type { IconName } from "../../generated/IconName";
import type { NoyanButton } from "../../generated/NoyanButton";
import type { NoyanButtonParams } from "../../generated/NoyanButtonParams";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { chosenAgent, lookedCount, shownAgents, toggleAgentPanel } from "../agent_panel/logic";
import { updatesStore, watchUpdates } from "../settings/updates";
import { railMenuChoice } from "./projectMenu";

/** The service events that change the rail's projects or counts. */
export const RAIL_EVENTS = [
  "projects_changed",
  "ticket_changed",
  "ticket_deleted",
  "session_state",
  "needs_you_changed",
  // The user's keys in its tooltips (KEY-08).
  "settings_changed",
] as const;

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

/** A button at the rail's bottom (a plain function, so the rail's elements show its click without rendering); `dot`
 * marks it, with that line under its tooltip. */
function bottomButton(button: RailButton, icon: IconName, onClick: () => void, dot: string | null = null) {
  const label = dot === null ? button.label : `${button.label} · ${dot}`;
  return (
    <Tooltip
      key={button.label}
      text={button.label}
      keys={button.keys ?? undefined}
      detail={dot === null ? undefined : [dot]}
    >
      <button type="button" className="rail-bottom" aria-label={label} onClick={onClick}>
        <Icon name={icon} size={16} />
        {dot !== null && <span className="rail-update-dot" />}
      </button>
    </Tooltip>
  );
}

/** The Noyan button: opens the panel (and focuses it), or closes it; its look is `noyan_button.view`'s. */
export function RailAgentButton({ button }: { button: NoyanButton }) {
  return (
    <Tooltip text={button.label} detail={[button.hint]}>
      <button
        type="button"
        className="rail-agent"
        aria-label={button.dot_label === null ? button.label : `${button.label}, ${button.dot_label}`}
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
  actions?: RailActions;
  /** The open project's Noyan button; none before it loaded. */
  agent?: NoyanButton | undefined;
  /** The project menu open from a right-click on a tile (#11), and its owner's setter. */
  menu?: TileMenu | null;
  onMenu?: (menu: TileMenu | null) => void;
  /** An update is ready: Settings' dot and its tooltip line ("Otaq 0.2.0 is ready"). */
  updateDot?: string | null;
}

/** A right-click on a tile: that project's menu at the pointer (#11). */
export interface TileMenu {
  project: RailItem;
  anchor: Anchor;
}

/** The 64 px rail of tiles. */
export function RailTiles({ view, actions, agent, menu = null, onMenu, updateDot = null }: RailProps) {
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
      {bottomButton(view.bottom.settings, "settings", () => actions?.onSettings(), updateDot)}
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

/** The service events that change the Noyan button's tint or dot. */
export const NOYAN_BUTTON_EVENTS = [
  "project_agent_changed",
  "projects_changed",
  // The user's keys in its tooltip (KEY-08).
  "settings_changed",
] as const;

/** The shown project's Noyan button: follows its conversations, the panel and the conversation the user looked at. */
function useNoyanButton(project: string | null): NoyanButton | undefined {
  const open = useStore(shellLayout).agentOpen;
  const shownByProject = useStore(shownAgents);
  const shown = project === null ? null : chosenAgent(shownByProject, project);
  const looked = useStore(lookedCount);
  const load = useCallback(async () => {
    if (project === null) return undefined;
    const params: NoyanButtonParams = { project, open, ...(shown !== null && { shown }) };
    return (await request("noyan_button.view", params)) as NoyanButton;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new look is a new load: the reply just seen goes
  }, [project, open, shown, looked]);
  return useServiceView(load, NOYAN_BUTTON_EVENTS).view;
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
  const agent = useNoyanButton(project);
  const updateDot = useStore(updatesStore)?.dot ?? null;
  useEffect(() => watchUpdates(updatesStore.set), []);
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
      actions={railActions(app)}
      agent={agent}
      menu={menu}
      onMenu={setMenu}
      updateDot={updateDot}
    />
  );
}
