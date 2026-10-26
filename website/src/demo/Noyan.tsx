import type { DemoProps } from "./Demo";
import { Ic } from "./Ic";

/** Noyan's panel beside the board: the conversation, its ticket table with Create them / Not yet, and the input. */
export function Noyan({ s, store }: DemoProps) {
  return (
    <aside className="noyan" aria-label="Noyan">
      <div className="ny-h">
        <Ic name="bot" cls="s16" style={{ color: "var(--inv)" }} />
        <b>Noyan</b>
        <span className="cnt">Checkout slowness</span>
        <button
          className="ibtn"
          type="button"
          onClick={() => {
            store.toggleNoyan();
          }}
          aria-label="Close (⌘L)"
        >
          <Ic name="x" />
        </button>
      </div>
      <div className="ny-sub">
        <span className="av av-CC">CC</span>Claude Code · reads code, never edits it · creates Backlog tickets
        only
        <span style={{ marginLeft: "auto" }} className="mono">
          96k tokens
        </span>
      </div>
      <div className="ny-b" role="log">
        {s.noyanMsgs.map((m, i) => (
          <div key={String(i)} className={("msg " + m.cls).trim()}>
            {m.t}
            {m.tbl && (
              <div className="tbl">
                <div>
                  <span>#</span>
                  <span>Title</span>
                  <span>Agent</span>
                </div>
                {m.tbl.map(([n, title, agent]) => (
                  <div key={n}>
                    <span>{n}</span>
                    <span>{title}</span>
                    <span>{agent}</span>
                  </div>
                ))}
              </div>
            )}
            {m.ask && (
              <div
                className="opts"
                style={{ padding: 0, border: 0, background: "none", flexDirection: "row" }}
              >
                <button
                  className="btn btn-p btn-sm"
                  type="button"
                  onClick={() => {
                    store.createFromNoyan();
                  }}
                >
                  Create them
                </button>
                <button
                  className="btn btn-s btn-sm"
                  type="button"
                  onClick={() => {
                    store.declineNoyan();
                  }}
                >
                  Not yet
                </button>
              </div>
            )}
            {m.links && (
              <div style={{ marginTop: 6, display: "flex", gap: 10, flexWrap: "wrap" }}>
                {m.links.map((k) => (
                  <button
                    key={k}
                    className="lk"
                    type="button"
                    onClick={() => {
                      store.openTicket(k);
                    }}
                  >
                    {k}
                  </button>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
      <div className="ny-in">
        <input
          type="text"
          placeholder="Ask about shop-web…"
          aria-label="Ask Noyan"
          value={s.nyDraft}
          onChange={(e) => {
            store.setNyDraft(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              store.sendNoyan(s.nyDraft);
            }
          }}
        />
        <button
          className="ibtn"
          type="button"
          onClick={() => {
            store.sendNoyan(s.nyDraft);
          }}
          aria-label="Send"
          style={{ width: 24, height: 24 }}
        >
          <Ic name="send" />
        </button>
      </div>
    </aside>
  );
}
