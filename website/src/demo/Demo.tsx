// The live demo on the landing page (WEB-07, decisions 0102, 0138 §3): the canvas's Demo3.dc.html drawn in React at
// 1280 × 800. The state and every change live in ./state.ts and what's drawn is worked out in ./view.ts; these
// components only draw and call the store. The colours come from the page's tokens, so the demo switches theme with
// the page (no theme prop). Keys are handled only inside the demo; there is no handler on the page.

import "./demo.css";

import { type MouseEvent, type KeyboardEvent, useEffect, useState, useSyncExternalStore } from "react";

import { demoFrame } from "../components/demoScale";
import { Board, Toolbar } from "./Board";
import { FlowEditor } from "./FlowEditor";
import { SPRITE } from "./icons";
import { Ic } from "./Ic";
import { Noyan } from "./Noyan";
import { Sheet } from "./Sheet";
import { type DemoState, DemoStore } from "./state";
import { type BoardView, boardView, ticketView, type TicketView } from "./view";

// Below 900 px the page shows the demo as a view-only preview (WEB-05, components/demoScale.ts) and the pointer can't
// reach it; the demo is then inert too, so the keyboard and screen readers skip its controls, scaled far too small.
const subscribeWidth = (onChange: () => void) => {
  window.addEventListener("resize", onChange);
  return () => {
    window.removeEventListener("resize", onChange);
  };
};
const isViewOnly = () => demoFrame(document.documentElement.clientWidth || window.innerWidth).viewOnly;

export interface DemoProps {
  s: DemoState;
  store: DemoStore;
}

export default function Demo() {
  const [store] = useState(() => new DemoStore());
  const s = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const viewOnly = useSyncExternalStore(subscribeWidth, isViewOnly, () => false);
  useEffect(
    () => () => {
      store.dispose();
    },
    [store],
  );

  const b = boardView(s);
  const t = ticketView(s);

  // A click outside the open menu closes it; the buttons that open menus (`data-menu`) toggle their own.
  const onClick = (e: MouseEvent) => {
    if (!s.menu && !s.stageMenu) return;
    if (e.target instanceof Element && e.target.closest(".menu, [data-menu]")) return;
    store.closeMenus();
  };
  // Esc closes the innermost thing open: a menu, the flow editor, then the ticket.
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    if (s.menu || s.stageMenu) store.closeMenus();
    else if (s.flowOpen) store.closeFlow();
    else if (s.openKey) store.closeTicket();
    else return;
    e.stopPropagation();
  };

  return (
    // Clicks and Esc are caught here only to close what's open; every control is a button of its own.
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- closing menus on a click outside them
    <div className="oq" inert={viewOnly} onClick={onClick} onKeyDown={onKeyDown}>
      <svg className="sprite" aria-hidden="true" dangerouslySetInnerHTML={{ __html: SPRITE }}></svg>
      <Header s={s} store={store} b={b} />
      <div className="body">
        <Rail s={s} store={store} b={b} />
        {s.noyanOpen && <Noyan s={s} store={store} />}
        <section className="main" aria-label="Board">
          <Toolbar s={s} store={store} b={b} />
          {s.paused && (
            <div className="banner">
              <Ic name="pause" />
              Paused · {b.pause.scope}. Agents are held; nothing is lost. Pause never lifts on its own.
              <button
                className="btn btn-s btn-sm"
                type="button"
                onClick={() => {
                  store.resume();
                }}
              >
                <Ic name="play" cls="s12" />
                Resume
              </button>
            </div>
          )}
          <Board s={s} store={store} b={b} />
          {!s.noyanOpen && (
            <label className="ask">
              <Ic name="bot" />
              <input
                type="text"
                placeholder="Ask Noyan, your planning agent… (“why is checkout slow?”)"
                aria-label="Ask Noyan"
                value={s.askDraft}
                onChange={(e) => {
                  store.setAskDraft(e.target.value);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    store.sendNoyan(s.askDraft);
                  }
                }}
              />
              <span className="kbd">⌘L</span>
            </label>
          )}
          {t && <Sheet s={s} store={store} t={t} b={b} />}
        </section>
      </div>
      <Menus s={s} store={store} b={b} t={t} />
      {s.flowOpen && <FlowEditor s={s} store={store} />}
      {s.toast && (
        <div className="toast" role="status">
          <Ic name="info" cls="s16" />
          <span>{s.toast}</span>
        </div>
      )}
    </div>
  );
}

function Header({ s, store, b }: DemoProps & { b: BoardView }) {
  return (
    <header className="hd">
      <span className="lights" aria-hidden="true">
        <i></i>
        <i></i>
        <i></i>
      </span>
      <button
        className="pbtn"
        type="button"
        data-menu
        onClick={() => {
          store.toggleMenu("proj");
        }}
        aria-haspopup="menu"
        aria-expanded={s.menu === "proj"}
      >
        {b.proj.name}
        <Ic name="chevron-down" cls="s12" style={{ color: "var(--tx3)" }} />
      </button>
      <span className="chip">
        <Ic name="git-branch" cls="s12" />
        {b.proj.branch}
      </span>
      <div className="search">
        <label>
          <Ic name="search" />
          <input
            type="text"
            placeholder="Search or ask…"
            aria-label="Search"
            value={s.search}
            onChange={(e) => {
              store.setSearch(e.target.value);
            }}
          />
          {/* The app's search key (decision 0137): the canvas says ⌘K. */}
          <span className="kbd">/</span>
        </label>
      </div>
      <button
        className={"hbtn " + (s.menu === "activity" ? "on" : "")}
        type="button"
        data-menu
        onClick={() => {
          store.toggleMenu("activity");
        }}
        aria-haspopup="dialog"
        aria-expanded={s.menu === "activity"}
      >
        <span className="dot" style={{ background: "var(--work)" }}></span>
        <span className="mono" style={{ fontSize: 12 }}>
          {b.runningLabel}
        </span>
        <span className="mono" style={{ fontSize: 12, color: "var(--pau)" }}>
          · 18% left
        </span>
      </button>
      <button
        className={"hbtn " + b.pause.cls}
        type="button"
        data-menu
        onClick={() => {
          store.toggleMenu("pause");
        }}
        aria-haspopup="menu"
        aria-expanded={s.menu === "pause"}
      >
        <Ic name={b.pause.icon} />
        {b.pause.label}
      </button>
      <button
        className="btn btn-p"
        type="button"
        onClick={() => {
          store.tip("new");
        }}
      >
        <Ic name="plus" />
        New ticket
      </button>
      <button
        className="ibtn"
        type="button"
        onClick={() => {
          store.tip("settings");
        }}
        aria-label="Settings"
      >
        <Ic name="settings" cls="s16" />
      </button>
    </header>
  );
}

function Rail({ s, store, b }: DemoProps & { b: BoardView }) {
  return (
    <nav className="rail" aria-label="Projects">
      {b.projects.map((p) => (
        <span key={p.key} style={{ display: "contents" }}>
          <button
            className={"proj " + (p.on ? "on" : "")}
            type="button"
            onClick={() => {
              store.pickProject(p.key);
            }}
            aria-label={p.name}
            aria-current={p.on ? "page" : undefined}
          >
            {p.key}
            {p.badge > 0 && <span className="badge pb">{p.badge}</span>}
          </button>
          <span className="proj-n">{p.name}</span>
        </span>
      ))}
      <button
        className="ibtn"
        type="button"
        onClick={() => {
          store.tip("addProject");
        }}
        aria-label="Add project"
        style={{ border: "1px dashed var(--bds)" }}
      >
        <Ic name="plus" />
      </button>
      <span className="sp"></span>
      <button
        className={"ibtn " + (s.noyanOpen ? "on" : "")}
        type="button"
        onClick={() => {
          store.toggleNoyan();
        }}
        aria-label="Noyan (⌘L)"
        aria-pressed={s.noyanOpen}
      >
        <Ic name="bot" cls="s16" />
      </button>
      <button
        className="ibtn"
        type="button"
        onClick={() => {
          store.tip("terminals");
        }}
        aria-label="Terminals"
      >
        <Ic name="terminal" cls="s16" />
      </button>
    </nav>
  );
}

/** The header's menus, Approve's and a column's stage menu, placed as drawn in the demo's own 1280 px coordinates. */
function Menus({ s, store, b, t }: DemoProps & { b: BoardView; t: TicketView | null }) {
  return (
    <>
      {s.menu === "proj" && (
        <div className="menu" style={{ top: 40, left: 54 }} role="menu">
          <div className="h">shop-web</div>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.openFlow();
            }}
          >
            <Ic name="git-merge" />
            Change flow…
          </button>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.tip("settings");
            }}
          >
            <Ic name="settings" />
            Project settings
          </button>
          <div className="sep"></div>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.tip("addProject");
            }}
          >
            <Ic name="plus" />
            Add project…
          </button>
        </div>
      )}
      {s.menu === "pause" && (
        <div className="menu" style={{ top: 40, right: 170 }} role="menu">
          <div className="h">{b.pause.menuTitle}</div>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.pause("this");
            }}
          >
            This project<span>⇧⌘.</span>
          </button>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.pause("all");
            }}
          >
            All projects
          </button>
          <div className="n">
            Holds agents at once and loses nothing. Never lifts on its own, restarts included.
          </div>
        </div>
      )}
      {s.menu === "activity" && (
        <div
          className="menu act"
          style={{ top: 40, right: 300, width: 420 }}
          role="dialog"
          aria-label="Activity"
        >
          <div className="h">Activity · {b.runningLabel}</div>
          {b.actTools.map((at) => (
            <div key={at.id} className="arow">
              <span className={"av lg av-" + at.id}>{at.id}</span>
              <div className="ainfo">
                <div className="an">
                  <b>{at.name}</b>
                  <span className="cnt">{at.running}</span>
                </div>
                <div className={"al " + at.cls}>{at.limit}</div>
                {at.pct !== null && (
                  <div className="abar">
                    <i style={{ width: String(at.pct) + "%", background: at.barColor }}></i>
                  </div>
                )}
                <div className="at">{at.tokens}</div>
              </div>
              {at.hasFallback && (
                <button
                  className="btn btn-s btn-sm"
                  type="button"
                  onClick={() => {
                    store.useFallback();
                  }}
                >
                  Use fallback now
                </button>
              )}
            </div>
          ))}
          <div className="n">
            Tokens are what each tool reports. Otaq shows no dollar estimates. ⇧⌘U opens this.
          </div>
        </div>
      )}
      {s.menu === "approve" && t && (
        <div className="menu" style={{ bottom: 60, right: 40, zIndex: 60 }} role="menu">
          <div className="h">Approve {t.k}</div>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.approve(t.k, "local");
            }}
          >
            <Ic name="git-merge" />
            Local merge<span>squash into main</span>
          </button>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.approve(t.k, "pr");
            }}
          >
            <Ic name="git-pr" />
            Open PR<span>gh · GitHub</span>
          </button>
          <div className="n">
            Local merge runs the merge check (pnpm test on the combined result) if main moved since the checks
            ran.
          </div>
        </div>
      )}
      {s.stageMenu && (
        <div className="menu" style={{ top: s.stageMenu.top, left: s.stageMenu.left }} role="menu">
          <div className="h">{s.stageMenu.name}</div>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.openFlow(true);
            }}
          >
            Edit this stage…
          </button>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.openFlow(true);
            }}
          >
            Add a stage after
          </button>
          <button
            className="i"
            type="button"
            role="menuitem"
            onClick={() => {
              store.tip("fold");
            }}
          >
            Fold when empty
          </button>
        </div>
      )}
    </>
  );
}
