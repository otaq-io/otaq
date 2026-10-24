// The Terminals drawer (SHL-04, UI-09, RUN-06; d0068, d0019, d0087; *17 · Project terminals*): a bottom drawer across
// the window with one tab per Project terminal of the shown project, + (a new terminal straight away), and the shown
// tab's terminal (T-0216's component) with its hint until the first keystroke. Closing a terminal's tab ends its shell;
// hiding the drawer keeps them running. Project terminals only: agents live in the agent panel (d0087, T-0330). Everything it
// says comes from `drawer.view`.
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { IconButton, Tooltip } from "../../components";
import type { DrawerTab } from "../../generated/DrawerTab";
import type { DrawerView } from "../../generated/DrawerView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Terminal, type TerminalHandle } from "../../terminal/Terminal";
import type { TerminalState } from "../../terminal/state";
import { Icon } from "../../theme/Icon";
import { requestTab, shownTab, showsHint, tabRequests } from "./logic";

/** The service events that change the drawer's tabs (`settings_changed`: the hint's key for the agent panel, KEY-08). */
export const DRAWER_EVENTS = ["session_state", "projects_changed", "settings_changed"] as const;

const ignore = () => undefined;

function TabButton({
  tab,
  selected,
  closeLabel,
  onSelect,
  onClose,
}: {
  tab: DrawerTab;
  selected: boolean;
  closeLabel: string;
  onSelect: () => void;
  onClose: () => void;
}) {
  return (
    <div className="drawer-tab" data-selected={selected || undefined}>
      <button
        type="button"
        role="tab"
        aria-selected={selected}
        className="drawer-tab-button"
        onClick={onSelect}
      >
        <span className="drawer-tab-icon">
          <Icon name={tab.icon} size={13} />
        </span>
        <span className="drawer-tab-label">{tab.label}</span>
        {tab.note !== null && <span className="drawer-tab-note">{tab.note}</span>}
      </button>
      {selected && (
        <button type="button" className="drawer-tab-close" aria-label={closeLabel} onClick={onClose}>
          <Icon name="x" size={11} />
        </button>
      )}
    </div>
  );
}

export interface DrawerBodyProps {
  view: DrawerView;
  project: string;
  maximized: boolean;
  onMaximize: () => void;
  onHide: () => void;
  /** The gallery: no terminal attaches, and a fresh terminal shows its hint as if live. */
  preview?: boolean;
}

/** The drawer for a loaded view (also what the markup tests render). */
export function DrawerBody({
  view,
  project,
  maximized,
  onMaximize,
  onHide,
  preview = false,
}: DrawerBodyProps) {
  const [picked, setPicked] = useState<string | null>(null);
  const [typed, setTyped] = useState<ReadonlySet<string>>(() => new Set());
  const [live, setLive] = useState(preview);
  const terminal = useRef<TerminalHandle>(null);
  const asked = useStore(tabRequests);
  // A shortcut or + asked for a tab: it shows once the service lists it, until the user picks another.
  const wanted = asked && view.tabs.some((t) => t.id === asked.tab) ? asked.tab : picked;
  const tab = shownTab(view.tabs, wanted, view.selected);
  const focusedSeq = useRef(0);

  // ...and its terminal gets the keyboard, once per request.
  useEffect(() => {
    if (!asked || asked.seq === focusedSeq.current || tab?.id !== asked.tab) return;
    focusedSeq.current = asked.seq;
    terminal.current?.focus();
  }, [asked, tab?.id]);

  const pick = (id: string) => {
    setPicked(id);
    tabRequests.set(null);
    request("drawer.select", { project, tab: id }).catch(ignore);
  };
  const show = (pending: Promise<unknown>) => {
    pending.then((tab) => {
      if (typeof tab === "string") requestTab(tab);
    }, ignore);
  };
  const close = (target: DrawerTab) => {
    request("drawer.close", { project, tab: target.id }).catch(ignore);
  };
  const onState = useCallback((state: TerminalState) => {
    setLive(state.phase === "live");
  }, []);
  return (
    <section
      className="drawer"
      aria-label={view.title}
      data-maximized={maximized || undefined}
      style={maximized ? undefined : { height: view.height }}
    >
      <div className="drawer-header">
        <div className="drawer-tabs" role="tablist" aria-label={view.title}>
          {view.tabs.map((t) => (
            <TabButton
              key={t.id}
              tab={t}
              selected={t.id === tab?.id}
              closeLabel={view.close_tab}
              onSelect={() => {
                pick(t.id);
              }}
              onClose={() => {
                close(t);
              }}
            />
          ))}
          <Tooltip text={view.new_label}>
            <button
              type="button"
              className="drawer-new"
              aria-label={view.new_label}
              onClick={() => {
                show(request("drawer.new", { project }));
              }}
            >
              <Icon name="plus" size={14} />
            </button>
          </Tooltip>
        </div>
        <span className="drawer-spacer" />
        <span className="drawer-controls">
          <Tooltip text={view.hide}>
            <IconButton icon="minus" label={view.hide} onClick={onHide} />
          </Tooltip>
          <Tooltip text={maximized ? view.restore : view.maximize}>
            <IconButton
              icon={maximized ? "minimize-2" : "maximize-2"}
              label={maximized ? view.restore : view.maximize}
              onClick={onMaximize}
            />
          </Tooltip>
          <Tooltip text={view.hide}>
            <IconButton icon="x" label={view.hide} onClick={onHide} />
          </Tooltip>
        </span>
      </div>
      <div
        className="drawer-main"
        onKeyDownCapture={() => {
          if (tab && !typed.has(tab.id)) setTyped(new Set([...typed, tab.id]));
        }}
      >
        {tab ? (
          <div className="drawer-terminal">
            {showsHint(tab, typed, live) && (
              <div className="drawer-hint">
                {tab.hint.map((line) => (
                  <p key={line}>{line}</p>
                ))}
              </div>
            )}
            <div className="drawer-terminal-box">
              {!preview && (
                <Terminal
                  key={tab.session}
                  session={tab.session}
                  label={`${tab.label} · ${view.title}`}
                  readOnly={tab.ended}
                  onState={onState}
                  files={{ kind: "project", project }}
                  ref={terminal}
                />
              )}
            </div>
            {tab.ended_line !== null && <p className="drawer-ended">{tab.ended_line}</p>}
          </div>
        ) : (
          <div className="drawer-empty">
            {view.empty.map((line) => (
              <p key={line}>{line}</p>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

/** The overlay: the shown project's drawer, which pushes the board up while it is open. */
export function TerminalsDrawer({ close }: OverlayProps<"terminals">) {
  const { project } = useSelection();
  const [maximized, setMaximized] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const key = project ?? "";
  const load = useCallback(() => request("drawer.view", { project: key }) as Promise<DrawerView>, [key]);
  const { view } = useServiceView(load, DRAWER_EVENTS);
  const height = view?.height;
  // The drawer is a row of the window, not a cover over the board (*Empty · shell prompt*: the board ends above it).
  useLayoutEffect(() => {
    const frame = root.current?.closest<HTMLElement>(".frame");
    if (!frame || height === undefined) return;
    // The frame keeps the window's height: the padding comes out of the board's room.
    frame.style.boxSizing = "border-box";
    frame.style.paddingBottom = maximized ? "" : `${String(height)}px`;
    return () => {
      frame.style.boxSizing = "";
      frame.style.paddingBottom = "";
    };
  }, [height, maximized]);
  if (project === null) return null;
  return (
    <div ref={root} className="drawer-host" data-maximized={maximized || undefined}>
      {view && (
        <DrawerBody
          view={view}
          project={project}
          maximized={maximized}
          onMaximize={() => {
            setMaximized(!maximized);
          }}
          onHide={close}
        />
      )}
    </div>
  );
}
