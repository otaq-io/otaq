// Settings (⌘,; *15 · Settings*): one modal window, 1040 × 720, a nav on the left (the app's topics) and one page on
// the right. Each topic is its own page, titled with its nav label (or the heading the topic puts there: one agent's
// page), showing only that topic's sections. The nav, the page and every row come from `settings.view`; an edit
// answers with the fresh view. Esc or × closes it. A project's settings are its own page, in its main area
// (`project_settings/`).
import { type ReactNode, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { type OverlayProps, useRegistries } from "../../app/registry";
import { IconButton } from "../../components";
import type { SettingsButtonAction } from "../../generated/SettingsButtonAction";
import type { SettingsNavRow } from "../../generated/SettingsNavRow";
import type { SettingsValue } from "../../generated/SettingsValue";
import type { SettingsView } from "../../generated/SettingsView";
import type { ThemeCreditsDialog } from "../../generated/ThemeCreditsDialog";
import type { RequestError } from "../../generated/RequestError";
import { asRequestError, watchView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { SettingsContext, type SettingsContextValue, topicRow } from "./context";
import { openProjectSettings } from "../project_settings/logic";
import { CreditsDialog } from "./Dialogs";

type Open = { kind: "credits"; dialog: ThemeCreditsDialog } | null;

function NavItem({ row, on, onClick }: { row: SettingsNavRow; on: boolean; onClick: () => void }) {
  return (
    <button type="button" className="settings-nav-item" aria-current={on || undefined} onClick={onClick}>
      <Icon name={row.icon} />
      <span className="settings-nav-label">{row.label}</span>
    </button>
  );
}

export function Settings({ params, close }: OverlayProps<"settings">) {
  const app = useRegistries();
  const [view, setView] = useState<SettingsView | null>(null);
  const [error, setError] = useState<RequestError | null>(null);
  const [dialog, setDialog] = useState<Open>(null);
  // Where a topic puts its own header buttons (Keyboard shortcuts' Reset all).
  const [headerSlot, setHeaderSlot] = useState<HTMLElement | null>(null);
  // A topic's own heading in place of its nav label (one agent's page on Agents).
  const [heading, setHeading] = useState<ReactNode | null>(null);
  const [visit, setVisit] = useState(0);
  // The open global topic, as any of its sections: Settings opened on a section opens the topic that holds it.
  const [topic, setTopic] = useState<string | null>(params?.section ?? null);
  // The section Settings was opened on, scrolled to once its topic's page is drawn.
  const scrollPending = useRef<string | null>(params?.section ?? null);
  const body = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);

  // The view, again on every settings or project change (a theme picked elsewhere, a project added), every warm
  // build change (the switch's "Clearing… (waiting for …)" line) and every change of what macOS allows (ATT-14).
  const load = useCallback(() => request("settings.view") as Promise<SettingsView>, []);
  useEffect(
    () =>
      watchView(
        load,
        ["settings_changed", "projects_changed", "warm_build_changed", "notifications_changed"],
        (state) => {
          if (state.view) setView(state.view);
          setError(state.error ?? null);
        },
      ),
    [load],
  );

  const edit = useCallback((field: string, value: SettingsValue) => {
    request("settings.edit", { field, value }).then(
      (fresh) => {
        setView(fresh as SettingsView);
        setError(null);
      },
      (e: unknown) => {
        setError(asRequestError(e));
      },
    );
  }, []);

  const act = useCallback(
    (action: SettingsButtonAction) => {
      switch (action) {
        case "cleanup":
          close();
          openOverlay("cleanup");
          return;
        case "credits":
          void request("settings.credits").then((d) => {
            setDialog({ kind: "credits", dialog: d as ThemeCreditsDialog });
          });
          return;
        // What macOS allows (ATT-14): its question, in the app's process, or Otaq's entry in System Settings.
        case "allow_notifications":
          request("notices.notifications_allow").catch((e: unknown) => {
            setError(asRequestError(e));
          });
          return;
        case "open_system_settings":
          request("notices.open_notification_settings").catch((e: unknown) => {
            setError(asRequestError(e));
          });
          return;
        // A project's rows: never on the app's page.
        case "undo_run_command":
        case "edit_field":
        case "remove_project":
          return;
      }
    },
    [close],
  );

  // A project's page at one of its sections (What this agent reads' Edit in <project>'s settings →): its own page.
  const openProject = useCallback((key: string, section: string) => {
    openProjectSettings(key, section === "project_notes" ? "project_notes" : null);
  }, []);

  const page = view?.page;
  const context = useMemo<SettingsContextValue | null>(
    () =>
      page
        ? { page, project: null, edit, act, headerSlot, setHeading, visit, agent: params?.agent, openProject }
        : null,
    [page, edit, act, headerSlot, visit, params?.agent, openProject],
  );

  const globalRows = view?.nav ?? [];
  const openRow = topicRow(globalRows, topic);
  const topicSections: readonly string[] = openRow?.target.sections ?? [];

  // The sections this page has that a screen registered, in the page's order; only the open topic's.
  const sections =
    page?.page !== "global"
      ? []
      : page.sections.flatMap((s) => {
          const entry = app.settings.global.get(s.id);
          return entry && topicSections.includes(s.id) ? [{ id: s.id, Component: entry.Component }] : [];
        });

  // Opened on a section further down its topic's page: scroll to it once it's drawn.
  useLayoutEffect(() => {
    const wanted = scrollPending.current;
    if (wanted === null || !page) return;
    const i = sections.findIndex((s) => s.id === wanted);
    const element = body.current?.querySelectorAll<HTMLElement>(":scope > .settings-section-slot")[i];
    if (!element || !body.current) return;
    body.current.scrollTop = i === 0 ? 0 : element.offsetTop;
    scrollPending.current = null;
  });

  // Esc closes Settings, unless a menu or a dialog inside it takes it (or a field, which stops it first). The box
  // takes the focus once, when Settings opens: the frame hands a new `close` on each of its renders, and focusing
  // again then took the focus from the field being typed in (T-0506).
  const latestClose = useRef(close);
  useEffect(() => {
    latestClose.current = close;
  });
  useEffect(() => {
    const box = root.current;
    if (!box) return;
    box.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || (e.target as Element | null)?.closest(".menu, .popover, .dialog")) return;
      e.stopPropagation();
      latestClose.current();
    };
    box.addEventListener("keydown", onKey);
    return () => {
      box.removeEventListener("keydown", onKey);
    };
  }, []);

  const go = (row: SettingsNavRow) => {
    scrollPending.current = null;
    setHeading(null);
    setVisit((v) => v + 1);
    setTopic(row.target.section);
    body.current?.scrollTo({ top: 0 });
  };

  return (
    <div
      className="settings-backdrop"
      role="presentation"
      // A press on the dim around it closes Settings, as Esc does and as every dialog does.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div
        ref={root}
        className="settings"
        role="dialog"
        aria-modal="true"
        aria-label={view?.title}
        tabIndex={-1}
      >
        <nav className="settings-nav" aria-label={view?.title}>
          <h2 className="settings-title">{view?.title}</h2>
          <h3 className="settings-nav-heading">{view?.global_heading}</h3>
          {globalRows.map((row) => (
            <NavItem
              key={row.label}
              row={row}
              on={row === openRow}
              onClick={() => {
                go(row);
              }}
            />
          ))}
        </nav>
        <div className="settings-main">
          <header className="settings-header">
            {heading ?? <h2>{openRow?.label}</h2>}
            <div ref={setHeaderSlot} className="settings-header-actions" />
            <IconButton icon="x" label={view?.close_label ?? ""} onClick={close} />
          </header>
          {error && (
            <p className="settings-error" role="alert">
              {error.message}
            </p>
          )}
          <div ref={body} className="settings-body">
            {context && (
              <SettingsContext value={context}>
                {sections.map(({ id, Component }) => (
                  <div key={id} className="settings-section-slot">
                    <Component />
                  </div>
                ))}
              </SettingsContext>
            )}
          </div>
        </div>
        {dialog?.kind === "credits" && (
          <CreditsDialog
            dialog={dialog.dialog}
            onClose={() => {
              setDialog(null);
            }}
          />
        )}
      </div>
    </div>
  );
}
