import type { MouseEvent } from "react";

import type { DemoProps } from "./Demo";
import { Ic } from "./Ic";
import { type BoardView, type CardView, type ColumnView, stageMenuAt } from "./view";

type BoardProps = DemoProps & { b: BoardView };

/** The frame's scale (LiveDemo.astro's `data-demo-scale`): the demo is drawn scaled, so a click's offset is divided by
 * it to land in the demo's own 1280 px coordinates (decision 0138 §3). */
function frameScale(el: Element): number {
  const v = Number(el.closest<HTMLElement>("[data-demo-scale]")?.dataset.demoScale);
  return v > 0 ? v : 1;
}

/** The board's toolbar: Board, List and Files, the flow button and the hint. */
export function Toolbar({ s, store, b }: BoardProps) {
  return (
    <div className="tb">
      <div className="seg" role="tablist">
        <button
          className={s.view === "board" ? "on" : ""}
          type="button"
          role="tab"
          aria-selected={s.view === "board"}
          onClick={() => {
            store.setView("board");
          }}
        >
          <Ic name="columns" />
          Board
        </button>
        <button
          className={s.view === "list" ? "on" : ""}
          type="button"
          role="tab"
          aria-selected={s.view === "list"}
          onClick={() => {
            store.setView("list");
          }}
        >
          <Ic name="list" />
          List
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={false}
          onClick={() => {
            store.tip("files");
          }}
        >
          <Ic name="folder" />
          Files
        </button>
      </div>
      <button
        className="hbtn"
        type="button"
        onClick={() => {
          store.openFlow();
        }}
      >
        <Ic name="git-merge" />
        {b.flowLabel}
        <Ic name="chevron-down" cls="s12" style={{ color: "var(--tx3)" }} />
      </button>
      <span className="sp"></span>
      <span className="cnt">{b.hintText}</span>
    </div>
  );
}

/** The Needs you strip, then the columns or the list. */
export function Board({ s, store, b }: BoardProps) {
  const next = (
    <>
      Next<span className="kbd">⌘J</span>
    </>
  );
  return (
    <>
      {b.needs.length > 0 ? (
        <div className="strip" role="region" aria-label="Needs you">
          <span className="t">Needs you · {b.needs.length}</span>
          {b.needs.map((n) => (
            <button
              key={n.k}
              className="ch"
              type="button"
              onClick={() => {
                store.openTicket(n.k);
              }}
            >
              <span className="k">{n.k}</span>
              {n.why}
            </button>
          ))}
          <button
            className="btn btn-s btn-sm nx"
            type="button"
            onClick={() => {
              store.nextItem();
            }}
          >
            {next}
          </button>
        </div>
      ) : (
        <div className="strip clear">
          <Ic name="check" style={{ color: "var(--ok)" }} />
          Nothing needs you here. Other projects show a badge on the rail; ⌘J moves on to them.
          <button
            className="btn btn-s btn-sm"
            type="button"
            onClick={() => {
              store.nextItem();
            }}
            style={{ marginLeft: "auto" }}
          >
            {next}
          </button>
        </div>
      )}
      {s.view === "board" ? (
        <div className="cols">
          {b.columns.map((col) => (
            <BoardColumn key={col.id} col={col} s={s} store={store} />
          ))}
          {!s.doneOpen && (
            <button
              className="donerail"
              type="button"
              onClick={() => {
                store.openDone();
              }}
              aria-label="Show Done"
            >
              <Ic name="git-merge" style={{ color: "var(--done)" }} />
              <span>Done</span>
              <span className="cnt" style={{ writingMode: "horizontal-tb" }}>
                {b.doneCount}
              </span>
            </button>
          )}
        </div>
      ) : (
        <List s={s} store={store} b={b} />
      )}
    </>
  );
}

function BoardColumn({ col, s, store }: DemoProps & { col: ColumnView }) {
  const openMenu = (e: MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    const btn = e.currentTarget;
    const host = btn.closest(".oq");
    if (!host) return;
    const r = btn.getBoundingClientRect();
    const h = host.getBoundingClientRect();
    const scale = frameScale(host);
    const at = stageMenuAt({ bottom: (r.bottom - h.top) / scale, left: (r.left - h.left) / scale });
    store.openStageMenu(col.id, col.title, at.top, at.left);
  };
  return (
    // A drop target for the mouse; from the keyboard, Start in the ticket's sheet does the same.
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- drag and drop only; the sheet is the keyboard path
    <div
      className={("colm " + col.cls).trim()}
      onDragOver={(e) => {
        if (!s.dragKey) return;
        e.preventDefault();
        store.dragOver(col.id);
      }}
      onDrop={(e) => {
        e.preventDefault();
        store.drop(col.id);
      }}
    >
      <div className="col-h">
        <span className="k" style={{ background: col.color }}></span>
        {col.title}
        <span className="n">{col.cards.length}</span>
        <span className="kd">{col.kind}</span>
        <button className="ibtn" type="button" data-menu onClick={openMenu} aria-label="Stage menu">
          <Ic name="ellipsis" cls="s12" />
        </button>
      </div>
      <div className="col-s">{col.sub}</div>
      <div className="col-b">
        {col.cards.map((c) => (
          <Card key={c.k} c={c} store={store} />
        ))}
        {col.cards.length === 0 && <div className="ph">{col.emptyText}</div>}
      </div>
    </div>
  );
}

function Card({ c, store }: Pick<DemoProps, "store"> & { c: CardView }) {
  const open = (e: { stopPropagation(): void }) => {
    e.stopPropagation();
    store.openTicket(c.k);
  };
  return (
    // The whole card opens the ticket for the mouse, as drawn; its title is the button that does it from the keyboard.
    // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- the title button inside is the keyboard path
    <div
      className={("cd " + c.cls).trim()}
      draggable={c.drag}
      onDragStart={(e) => {
        try {
          e.dataTransfer.setData("text/plain", c.k);
          e.dataTransfer.effectAllowed = "move";
        } catch {
          // Some browsers refuse data on a drag; the store keeps the dragged key itself.
        }
        store.dragStart(c.k);
      }}
      onDragEnd={() => {
        store.dragEnd();
      }}
      onClick={open}
    >
      <div className="cd-top">
        <span className="key">{c.k}</span>
        {c.hasPrio && <span className={"prio p" + String(c.prio)}>P{c.prio}</span>}
        <span style={{ flex: 1 }}></span>
        <span className={"av av-" + c.tool} title={c.agent}>
          {c.tool}
        </span>
      </div>
      <button className="cd-ti" type="button" onClick={open}>
        {c.title}
      </button>
      <div className={("st " + c.status.cls).trim()}>
        <Ic name={c.status.icon} cls="s12" />
        <span>{c.status.label}</span>
      </div>
      {c.pieces.length > 0 && (
        <div className="pieces">
          <span className="ph2">
            Pieces · {c.pieces.length} · {c.pieceSum}
          </span>
          {c.piecesShort.map((pc) => (
            <div key={pc.n} className="pr">
              <span className="n">{pc.n}</span>
              <span>
                {pc.t} · <span className={("s " + pc.cls).trim()}>{pc.s}</span>
              </span>
            </div>
          ))}
        </div>
      )}
      <div className="who">
        <span>{c.agent}</span>
        <span className="ago">{c.ago}</span>
      </div>
      {c.act && (
        <button className="btn btn-p btn-sm act" type="button" onClick={open}>
          {c.act}
        </button>
      )}
    </div>
  );
}

function List({ store, b }: BoardProps) {
  return (
    <div className="list">
      <div className="lh">
        <span>Ticket</span>
        <span>Where in the flow</span>
        <span>What’s happening</span>
        <span>Who</span>
      </div>
      {b.columns.map((col) => (
        <div key={col.id}>
          <div className="lg">
            <span className="k" style={{ background: col.color }}></span>
            {col.title}
            <span className="cnt">{col.cards.length}</span>
          </div>
          {col.cards.map((c) => (
            <button
              key={c.k}
              className={("lr " + c.cls).trim()}
              type="button"
              onClick={() => {
                store.openTicket(c.k);
              }}
            >
              <span>
                <span className="k">{c.k}</span>
                {c.title}
              </span>
              <span className="w">{col.title}</span>
              <span
                className={("st " + c.status.cls).trim()}
                style={{ display: "flex", alignItems: "center", gap: 5 }}
              >
                <Ic name={c.status.icon} cls="s12" />
                {c.status.label}
              </span>
              <span className="who">
                <span className={"av av-" + c.tool}>{c.tool}</span>
                {c.agent}
                <span className="mono" style={{ marginLeft: "auto" }}>
                  {c.ago}
                </span>
              </span>
            </button>
          ))}
        </div>
      ))}
    </div>
  );
}
