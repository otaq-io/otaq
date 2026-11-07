import { Fragment } from "react";

import type { DemoProps } from "./Demo";
import { Ic } from "./Ic";
import { type BoardView, type CardView, type ColumnView, editFlowView } from "./view";

type BoardProps = DemoProps & { b: BoardView };

/** The project's toolbar: Board, Files and Settings, the Board's two layouts (columns or List), and New ticket. */
export function Toolbar({ s, store, b }: BoardProps) {
  return (
    <div className="tb">
      <div className="seg" role="tablist" aria-label="View">
        <button className="on" type="button" role="tab" aria-selected={true}>
          Board
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={false}
          onClick={() => {
            store.tip("files");
          }}
        >
          Files
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={false}
          onClick={() => {
            store.tip("projectSettings");
          }}
        >
          Settings
        </button>
      </div>
      <div className="seg" role="group" aria-label="Layout">
        <button
          className={s.view === "board" ? "on" : ""}
          type="button"
          aria-pressed={s.view === "board"}
          aria-label="Columns"
          onClick={() => {
            store.setView("board");
          }}
        >
          <Ic name="columns" />
        </button>
        <button
          className={s.view === "list" ? "on" : ""}
          type="button"
          aria-pressed={s.view === "list"}
          aria-label="List"
          onClick={() => {
            store.setView("list");
          }}
        >
          <Ic name="list" />
        </button>
      </div>
      <span className="sp"></span>
      <span className="cnt">{b.hintText}</span>
      <button
        className="hbtn out"
        type="button"
        onClick={() => {
          document.querySelector<HTMLInputElement>(".oq .ask input")?.focus();
        }}
      >
        <Ic name="plus" />
        New ticket
      </button>
    </div>
  );
}

/** Edit flow's bar over the board: what's being edited, Watch a ticket, Start from…, Cancel and Save flow. */
function EditBar({ s, store }: DemoProps) {
  const v = editFlowView(s);
  return (
    <div className="editbar">
      <Ic name="git-merge" />
      <div className="et">
        <b>{v.title}</b>
        <span>{v.hint}</span>
      </div>
      <span className="sp"></span>
      <button
        className="hbtn"
        type="button"
        onClick={() => {
          store.tip("watch");
        }}
      >
        <Ic name="play" cls="s12" />
        Watch a ticket
      </button>
      <button
        className="hbtn"
        type="button"
        data-menu
        onClick={() => {
          store.toggleMenu("startFrom");
        }}
        aria-haspopup="menu"
        aria-expanded={s.menu === "startFrom"}
      >
        Start from…
      </button>
      <button
        className="btn btn-s btn-sm"
        type="button"
        onClick={() => {
          store.closeFlow();
        }}
      >
        Cancel
      </button>
      <button
        className="btn btn-p btn-sm"
        type="button"
        disabled={!v.changed}
        onClick={() => {
          store.saveFlow();
        }}
      >
        Save flow
      </button>
    </div>
  );
}

/** The Needs you strip (questions and decisions, then Your review), then the columns or the list. */
export function Board({ s, store, b }: BoardProps) {
  const next = (
    <>
      <Ic name="arrow-right" cls="s12" />
      Next<span className="kbd">⌘J</span>
    </>
  );
  const v = s.flowOpen ? editFlowView(s) : null;
  return (
    <>
      {s.flowOpen ? (
        <EditBar s={s} store={store} />
      ) : b.needs.length + b.reviews.length > 0 ? (
        <div className="strip" role="region" aria-label="Needs you">
          {b.needs.length > 0 && <span className="t">Needs you · {b.needs.length}</span>}
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
          {b.reviews.length > 0 && <span className="t rv">Your review · {b.reviews.length}</span>}
          {b.reviews.map((n) => (
            <button
              key={n.k}
              className="ch rv"
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
      {s.view === "board" || s.flowOpen ? (
        <div className={"cols" + (s.flowOpen ? " editing" : "")}>
          {b.columns.map((col, i) => (
            <Fragment key={col.id}>
              {s.flowOpen && i > 0 && (
                <button
                  className="gap-add"
                  type="button"
                  aria-label={"Add a stage between " + (b.columns[i - 1]?.title ?? "") + " and " + col.title}
                  onClick={() => {
                    store.tip("addStage");
                  }}
                >
                  <Ic name="plus" cls="s12" />
                </button>
              )}
              <BoardColumn col={col} s={s} store={store} />
            </Fragment>
          ))}
          {!s.flowOpen && (
            <div className="endcol">
              <button
                className="hbtn dashed"
                type="button"
                onClick={() => {
                  store.openFlow();
                }}
              >
                <Ic name="git-merge" cls="s12" />
                Edit flow
              </button>
              {!s.doneOpen && (
                <button
                  className="donelink"
                  type="button"
                  onClick={() => {
                    store.openDone();
                  }}
                  title={"Done · " + b.delivery}
                >
                  Done {b.doneCount}
                  <Ic name="arrow-right" cls="s12" />
                </button>
              )}
            </div>
          )}
        </div>
      ) : (
        <List s={s} store={store} b={b} />
      )}
      {v?.details && (
        <div className="stagedet" role="region" aria-label={v.details.name}>
          <b className="nm">{v.details.name}</b>
          {v.details.rows.map(([label, value]) => (
            <Fragment key={label}>
              <span className="l">{label}</span>
              <span className={label === "Skill" ? "mono" : ""}>{value}</span>
            </Fragment>
          ))}
        </div>
      )}
    </>
  );
}

function BoardColumn({ col, s, store }: DemoProps & { col: ColumnView }) {
  const h = col.head;
  return (
    // A drop target for the mouse; from the keyboard, Start on the card or in the ticket's sheet does the same.
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- drag and drop only; Start is the keyboard path
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
      {h ? (
        <button
          className={"stagehead" + (h.sel ? " sel" : "") + (h.fixed ? " fixed" : "")}
          type="button"
          aria-pressed={h.sel}
          disabled={col.id === "backlog"}
          onClick={() => {
            if (col.id !== "backlog") store.pickStage(col.id);
          }}
        >
          <span className="nm">
            <span className="k" style={{ background: col.color }}></span>
            {col.title}
          </span>
          <span className="wh">{h.what}</span>
          {h.who && <span className="who">{h.who}</span>}
          {h.skill && h.skill !== "—" && <span className="mono sk">{h.skill.replace(/^\//, "")}</span>}
          {h.line && <span className="wh">{h.line}</span>}
        </button>
      ) : (
        <>
          <div className="col-h">
            <span className="k" style={{ background: col.color }}></span>
            {col.title}
            <span className="n">{col.cards.length}</span>
          </div>
          <div className="col-s">{col.sub}</div>
        </>
      )}
      <div className="col-b">
        {col.cards.map((c) => (
          <Card key={c.k} c={c} store={store} />
        ))}
        {col.cards.length === 0 && !h && <div className="ph">{col.emptyText}</div>}
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
        <button
          className={"btn btn-sm act " + (c.act === "Start" ? "btn-s" : "btn-p")}
          type="button"
          onClick={(e) => {
            if (c.act === "Start") {
              e.stopPropagation();
              store.start(c.k);
            } else open(e);
          }}
        >
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
