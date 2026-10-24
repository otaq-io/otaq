// Settings (⌘,; *15 · Settings*): one modal window, 1040 × 720, a nav on the left (the global topics, then every
// project) and one page on the right. Each global topic is its own page, titled with its nav label (or the heading the
// topic puts there: one agent's page), showing only that topic's sections; a project has its own page. The nav, the
// page and every row come from `settings.view`; an edit answers with the fresh view. Esc or × closes it.
import { type ReactNode, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { type OverlayProps, useRegistries } from "../../app/registry";
import { IconButton } from "../../components";
import type { RemoveProjectDialog } from "../../generated/RemoveProjectDialog";
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
import { CreditsDialog, RemoveDialog } from "./Dialogs";

type Open =
  { kind: "remove"; dialog: RemoveProjectDialog } | { kind: "credits"; dialog: ThemeCreditsDialog } | null;

function NavItem({ row, on, onClick }: { row: SettingsNavRow; on: boolean; onClick: () => void }) {
  return (
    <button type="button" className="settings-nav-item" aria-current={on || undefined} onClick={onClick}>
      <Icon name={row.icon} />
      <span className="settings-nav-label">{row.label}</span>
      {row.alert && (
        <span className="settings-nav-alert">
          <Icon name="circle-alert" size={13} />
        </span>
      )}
    </button>
  );
}

export function Settings({ params, close }: OverlayProps<"settings">) {
  const app = useRegistries();
  const [project, setProject] = useState<string | null>(params?.project ?? null);
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
  const load = useCallback(() => request("settings.view", { project }) as Promise<SettingsView>, [project]);
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

  const edit = useCallback(
    (field: string, value: SettingsValue) => {
      request("settings.edit", { project, field, value }).then(
        (fresh) => {
          setView(fresh as SettingsView);
          setError(null);
        },
        (e: unknown) => {
          setError(asRequestError(e));
        },
      );
    },
    [project],
  );

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
        case "remove_project":
          if (project === null) return;
          void request("settings.remove_preview", { project }).then(
            (d) => {
              setDialog({ kind: "remove", dialog: d as RemoveProjectDialog });
            },
            (e: unknown) => {
              setError(asRequestError(e));
            },
          );
          return;
      }
    },
    [close, project],
  );

  // A project's page at one of its sections (What this agent reads' Edit in <project>'s settings →).
  // Each opening counts, so the same section scrolls again; `scrolledTo` keeps the last one done.
  const [projectSection, setProjectSection] = useState<{ section: string; n: number } | null>(null);
  const scrolledTo = useRef(0);
  const openProject = useCallback((key: string, section: string) => {
    setProjectSection((p) => ({ section, n: (p?.n ?? 0) + 1 }));
    setHeading(null);
    setVisit((v) => v + 1);
    setProject(key);
  }, []);

  const page = view?.page;
  const context = useMemo<SettingsContextValue | null>(
    () =>
      page
        ? { page, project, edit, act, headerSlot, setHeading, visit, agent: params?.agent, openProject }
        : null,
    [page, project, edit, act, headerSlot, visit, params?.agent, openProject],
  );

  const globalRows = view?.nav.filter((r) => r.target.page === "global") ?? [];
  const projectRows = view?.nav.filter((r) => r.target.page === "project") ?? [];
  const openRow = topicRow(globalRows, topic);
  const topicSections: readonly string[] = openRow?.target.page === "global" ? openRow.target.sections : [];

  // The sections this page has that a screen registered, in the page's order; on the global page only the open topic's.
  const sections = !page
    ? []
    : page.page === "global"
      ? page.sections.flatMap((s) => {
          const entry = app.settings.global.get(s.id);
          return entry && topicSections.includes(s.id) ? [{ id: s.id, Component: entry.Component }] : [];
        })
      : page.sections.flatMap((s) => {
          const entry = app.settings.project.get(s.id);
          return entry ? [{ id: s.id, Component: entry.Component }] : [];
        });

  // Opened on a section further down its topic's page (or a project's): scroll to it once it's drawn.
  useLayoutEffect(() => {
    const fresh = projectSection !== null && projectSection.n !== scrolledTo.current ? projectSection : null;
    const wanted = scrollPending.current ?? fresh?.section ?? null;
    if (wanted === null || !page) return;
    const i = sections.findIndex((s) => s.id === wanted);
    const element = body.current?.querySelectorAll<HTMLElement>(":scope > .settings-section-slot")[i];
    if (!element || !body.current) return;
    body.current.scrollTop = i === 0 ? 0 : element.offsetTop;
    scrollPending.current = null;
    if (fresh) scrolledTo.current = fresh.n;
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
    if (row.target.page === "project") setProject(row.target.key);
    else {
      setTopic(row.target.section);
      setProject(null);
    }
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
              on={project === null && row === openRow}
              onClick={() => {
                go(row);
              }}
            />
          ))}
          <h3 className="settings-nav-heading">{view?.projects_heading}</h3>
          {projectRows.map((row) => (
            <NavItem
              key={row.target.page === "project" ? row.target.key : row.label}
              row={row}
              on={row.target.page === "project" && row.target.key === project}
              onClick={() => {
                go(row);
              }}
            />
          ))}
        </nav>
        <div className="settings-main">
          <header className="settings-header">
            {heading ?? <h2>{page?.page === "global" ? openRow?.label : page?.title}</h2>}
            <div ref={setHeaderSlot} className="settings-header-actions" />
            <IconButton icon="x" label={view?.close_label ?? ""} onClick={close} />
          </header>
          {error && (
            <p className="settings-error" role="alert">
              {error.message}
            </p>
          )}
          <div ref={body} className="settings-body">
            {page?.page === "project" && <p className="settings-intro settings-page-intro">{page.intro}</p>}
            {context && (
              <SettingsContext value={context}>
                {sections.map(({ id, Component }) => (
                  <div key={`${project ?? ""}-${id}`} className="settings-section-slot">
                    <Component project={project ?? ""} />
                  </div>
                ))}
              </SettingsContext>
            )}
          </div>
        </div>
        {dialog?.kind === "remove" && project !== null && (
          <RemoveDialog
            dialog={dialog.dialog}
            onCancel={() => {
              setDialog(null);
            }}
            onConfirm={() => {
              setDialog(null);
              request("settings.remove", { project }).then(
                (fresh) => {
                  setProject(null);
                  setView(fresh as SettingsView);
                },
                (e: unknown) => {
                  setError(asRequestError(e));
                },
              );
            }}
          />
        )}
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
