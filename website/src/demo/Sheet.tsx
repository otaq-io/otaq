import type { Line } from "./data";
import type { DemoProps } from "./Demo";
import { Ic } from "./Ic";
import { type BoardView, PANE_TABS, type TicketView } from "./view";

type SheetProps = DemoProps & { t: TicketView; b: BoardView };

/** The open ticket's sheet over the board: header, details, the path through the flow, the tabs and the review bar. */
export function Sheet({ s, store, t, b }: SheetProps) {
  return (
    <>
      {/* The dimmed board closes the sheet for the mouse; Close and Esc do it from the keyboard. */}
      {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- Close and Esc are the keyboard path */}
      <div
        className="ts-dim"
        onClick={() => {
          store.closeTicket();
        }}
      ></div>
      <section className="tsheet" aria-label={t.k + " " + t.title}>
        <div className="ts-h">
          <div className="ts-heading">
            <nav className="ts-crumbs" aria-label="Breadcrumb">
              <span className="ts-crumb">{b.proj.name}</span>
              <span className="ts-sep">/</span>
              <span className="ts-crumb mono">{t.k}</span>
            </nav>
            <div className="ts-title-row">
              <h2 className="ts-title">{t.title}</h2>
              <span className={"pill " + t.pill.cls}>
                <Ic name={t.pill.icon} cls={t.pill.spin ? "spin" : ""} />
                {t.pill.label}
              </span>
            </div>
          </div>
          <div className="ts-acts">
            <button
              className="hbtn out"
              type="button"
              onClick={() => {
                store.tip("move");
              }}
            >
              <Ic name="arrow-right" cls="s12" />
              Move…
            </button>
            <button
              className="ibtn"
              type="button"
              onClick={() => {
                store.tip("more");
              }}
              aria-label="More"
            >
              <Ic name="ellipsis" />
            </button>
            <button
              className="ibtn"
              type="button"
              onClick={() => {
                store.tip("full");
              }}
              aria-label="Full page (e)"
            >
              <Ic name="maximize" />
            </button>
            <button
              className="ibtn"
              type="button"
              onClick={() => {
                store.closeTicket();
              }}
              aria-label="Close (Esc)"
            >
              <Ic name="x" cls="s16" />
            </button>
          </div>
        </div>
        <div className="ts-details">
          <span className="ts-det">
            <span className="ts-dl">Assignee</span>
            <span className="ts-dv">
              <span className={"av av-" + t.tool}>{t.tool}</span>
              {t.agent}
            </span>
          </span>
          <span className="ts-det">
            <span className="ts-dl">Model</span>
            <span className="ts-dv">{t.model}</span>
          </span>
          <span className="ts-det">
            <span className="ts-dl">{t.stateLabel}</span>
            <span className="ts-dv">
              {t.isRunning && <span className="ts-dot"></span>}
              {t.stateValue}
            </span>
          </span>
          <span className="ts-det">
            <span className="ts-dl">Priority</span>
            <button
              className="ts-dv ts-dbtn"
              type="button"
              onClick={() => {
                store.tip("priority");
              }}
            >
              P{t.prio}
              <Ic name="chevron-down" style={{ width: 11, height: 11 }} />
            </button>
          </span>
          <span className="ts-det">
            <span className="ts-dl">Branch</span>
            <span className="ts-dv">
              <span className="mono">otaq/{t.k}</span>
              <span className="ts-dn">from {b.proj.branch}</span>
            </span>
          </span>
        </div>
        <ol className="ts-path">
          {t.steps.map((st, i) => (
            <Step key={st.name} st={st} first={i === 0} />
          ))}
        </ol>
        <div className="ptabs" role="tablist">
          {PANE_TABS.map(([id, label, icon, key]) => (
            <button
              key={id}
              className={"ptab " + (s.ptab === id ? "on" : "")}
              type="button"
              role="tab"
              aria-selected={s.ptab === id}
              onClick={() => {
                store.setTab(id);
              }}
            >
              <Ic name={icon} />
              {label}
              {id === "session" && t.liveSession && (
                <span className="dot" style={{ background: "var(--ok)" }}></span>
              )}
              {id === "files" && t.adds > 0 && <span className="cnt">{t.fileCount}</span>}
              <span className="kbd" style={{ marginLeft: 4 }}>
                {key}
              </span>
            </button>
          ))}
        </div>
        <div className="ts-body">
          {s.ptab === "overview" && <Overview s={s} store={store} t={t} />}
          {s.ptab === "session" && <Session s={s} store={store} t={t} b={b} />}
          {s.ptab === "files" && <Files t={t} />}
          {s.ptab === "activity" &&
            t.hist.map((h) => (
              <div key={h.at + h.t} className="hist">
                <span className="t">{h.at}</span>
                <span>{h.t}</span>
              </div>
            ))}
        </div>
        {t.canApprove && (
          <div className="rbar">
            <span className="pill pl-r">
              <Ic name="eye" />
              Your check
            </span>
            <span className="cnt">{t.rbarNote}</span>
            <span style={{ flex: 1 }}></span>
            <button
              className="btn btn-s btn-sm"
              type="button"
              onClick={() => {
                store.sendBack(t.k);
              }}
            >
              Send back…
            </button>
            <button
              className="btn btn-p btn-sm"
              type="button"
              data-menu
              onClick={() => {
                store.toggleMenu("approve");
              }}
              aria-haspopup="menu"
            >
              Approve
            </button>
          </div>
        )}
      </section>
    </>
  );
}

function Step({ st, first }: { st: TicketView["steps"][number]; first: boolean }) {
  return (
    <>
      {!first && <li className={"ts-line " + (st.state === "to" ? "" : "done")} role="presentation"></li>}
      <li
        className={"ts-step " + st.state + (st.attn ? " attn" : "")}
        aria-current={st.state === "current" ? "step" : undefined}
      >
        <span className="ts-node">
          {st.state === "done" && <Ic name="check" style={{ width: 11, height: 11, strokeWidth: 2.5 }} />}
          {st.state === "current" && <span className="ts-sdot"></span>}
        </span>
        <span className="ts-sname">{st.name}</span>
      </li>
    </>
  );
}

function Overview({ s, store, t }: Pick<SheetProps, "s" | "store" | "t">) {
  return (
    <div className="ov">
      <div className="ov-l">
        {t.ask && (
          <div className="opts">
            <span className="opts-t">
              <Ic name={t.ask.icon} cls="s12" />
              {t.ask.title}
            </span>
            {t.ask.opts.map((o) => (
              <button
                key={o.n}
                className="opt"
                type="button"
                onClick={() => {
                  store.answer(o.act);
                }}
              >
                <span className="n">{o.n}</span>
                {o.label}
                <span className="rec">{o.rec}</span>
              </button>
            ))}
            <button
              className="opt"
              type="button"
              onClick={() => {
                store.tip("other");
              }}
            >
              <span className="n">{t.otherN}</span>
              Other…<span className="rec">type an answer</span>
            </button>
          </div>
        )}
        {t.canApprove && (
          <>
            {t.before && (
              <section className="before">
                <span className="sname">Before you · {t.before.head}</span>
                <span className="cnt">{t.before.sum}</span>
                {t.before.checks.map((ck) => (
                  <div key={ck.name} className="bagent">
                    <span className={"av av-" + ck.tool}>{ck.tool}</span>
                    <b>{ck.name}</b>
                    <span className="verdict" style={{ color: ck.color }}>
                      <Ic name="shield-check" cls="s12" />
                      {ck.verdict}
                    </span>
                    <span className="cnt">{ck.model}</span>
                    <span style={{ flex: 1 }}></span>
                    <button
                      className="btn btn-s btn-sm"
                      type="button"
                      onClick={() => {
                        store.showSession();
                      }}
                    >
                      Open terminal
                    </button>
                    <p>{ck.s}</p>
                  </div>
                ))}
              </section>
            )}
            {t.manual.length > 0 && (
              <section className="manual">
                <span className="sname">Manual checks for you</span>
                {t.manual.map((mc, i) => (
                  <button
                    key={mc.t}
                    className="chk"
                    type="button"
                    role="checkbox"
                    aria-checked={mc.on}
                    onClick={() => {
                      store.toggleManual(t.k, i);
                    }}
                  >
                    <i className={mc.on ? "on" : ""}>
                      {mc.on && <Ic name="check" style={{ width: 10, height: 10, strokeWidth: 2.5 }} />}
                    </i>
                    {mc.t}
                    <span className="cnt" style={{ marginLeft: "auto" }}>
                      from {mc.src}
                    </span>
                  </button>
                ))}
              </section>
            )}
            <div className="actbar">
              <button
                className="btn btn-p"
                type="button"
                data-menu
                onClick={() => {
                  store.toggleMenu("approve");
                }}
                aria-haspopup="menu"
              >
                <Ic name="check" />
                Approve
                <span className="kbd" style={{ marginLeft: 4 }}>
                  ⌘↵
                </span>
                <Ic name="chevron-down" cls="s12" />
              </button>
              <button
                className="btn btn-s"
                type="button"
                onClick={() => {
                  store.sendBack(t.k);
                }}
              >
                <Ic name="undo" />
                Send back · to Dev…
              </button>
              <button
                className="btn btn-s"
                type="button"
                onClick={() => {
                  store.tip("run");
                }}
              >
                <Ic name="play" />
                Run
              </button>
            </div>
          </>
        )}
        {t.canDecide && (
          <>
            <div className="callout attn">
              <Ic name="circle-alert" />
              <div>
                <b>Security failed 3 times</b>
                <span>
                  Unescaped SQL in lib/address.ts, the same finding each round. Decide what happens next.
                </span>
              </div>
            </div>
            <div className="actbar">
              <button
                className="btn btn-p"
                type="button"
                onClick={() => {
                  store.answer("r32");
                }}
              >
                Give it another round
              </button>
              <button
                className="btn btn-s"
                type="button"
                onClick={() => {
                  store.answer("m32");
                }}
              >
                Move on anyway
              </button>
              <button
                className="btn btn-s"
                type="button"
                onClick={() => {
                  store.showSession();
                }}
              >
                Open terminal
              </button>
            </div>
          </>
        )}
        {t.canStart && (
          <div className="actbar">
            <button
              className="btn btn-p"
              type="button"
              onClick={() => {
                store.start(t.k);
              }}
            >
              <Ic name="play" />
              Start
            </button>
            <span className="cnt">{t.startNote}</span>
          </div>
        )}
        {t.isRunning && (
          <div className="actbar">
            <button
              className="btn btn-s"
              type="button"
              onClick={() => {
                store.showSession();
              }}
            >
              <Ic name="square-terminal" />
              Open agent session
            </button>
            <button
              className="btn btn-s"
              type="button"
              onClick={() => {
                store.tip("stop");
              }}
            >
              Stop
            </button>
            <span className="cnt mono">otaq attach {t.k}</span>
          </div>
        )}
        {t.isPR && (
          <>
            <div className="callout">
              <Ic name="git-pr" style={{ color: "var(--ok)" }} />
              <div>
                <b>PR open on GitHub</b>
                <span>
                  Your team reviews it there. Otaq doesn’t watch GitHub: mark it merged here when it lands.
                </span>
              </div>
            </div>
            <div className="actbar">
              <button
                className="btn btn-p"
                type="button"
                onClick={() => {
                  store.markMerged(t.k);
                }}
              >
                <Ic name="git-merge" />
                Mark merged
              </button>
              <button
                className="btn btn-s"
                type="button"
                onClick={() => {
                  store.tip("openPR");
                }}
              >
                Open on GitHub
              </button>
            </div>
          </>
        )}
        <section>
          <span className="sname">What and why</span>
          <p className="prose">{t.desc}</p>
        </section>
        {t.pieces.length > 0 && (
          <section>
            <span className="sname">Plan · {t.pieces.length} pieces · written by Milestone plan</span>
            <div className="pieces big">
              {t.pieces.map((pc) => (
                <div key={pc.n} className="pr">
                  <span className="n">{pc.n}</span>
                  <span>{pc.t}</span>
                  <span className={("s " + pc.cls).trim()}>{pc.s}</span>
                </div>
              ))}
            </div>
            <span className="cnt">Status · from Otaq</span>
          </section>
        )}
        {t.decision && (
          <section>
            <span className="sname">Decisions</span>
            <div className="rep">
              <Ic name="lightbulb" cls="s12" style={{ color: "var(--inv)" }} />
              <div>
                <b>{t.decision.t}</b>
                <span>{t.decision.s}</span>
              </div>
            </div>
          </section>
        )}
        <section>
          <span className="sname">Dependencies</span>
          <div className="deps">
            <div className="dep">
              <span className="cnt">Blocked by</span>
              <span>{t.blockedBy}</span>
            </div>
            <div className="dep">
              <span className="cnt">Blocks</span>
              <span>{t.blocks}</span>
            </div>
          </div>
        </section>
        <Comments s={s} store={store} t={t} />
      </div>
      <div className="ov-r">
        <span className="sname">Attachments · {t.atts.length}</span>
        {t.atts.map((at) => (
          <button
            key={at.name}
            className="att"
            type="button"
            onClick={() => {
              store.tip("report");
            }}
          >
            <Ic name="file-text" style={{ color: "var(--tx2)" }} />
            <div>
              <div className="att-n">{at.name}</div>
              <div className="cnt">
                {at.size} ·{" "}
                <span
                  className={"av av-" + at.tool}
                  style={{ width: 12, height: 12, fontSize: 6, verticalAlign: -2 }}
                >
                  {at.tool}
                </span>{" "}
                {at.who} · {at.stage}
              </div>
              <div className="att-l">{at.line}</div>
            </div>
          </button>
        ))}
        {t.atts.length === 0 && <span className="cnt">No reports yet. Agents attach long output here.</span>}
        <button
          className="btn btn-s btn-sm"
          type="button"
          onClick={() => {
            store.tip("attach");
          }}
          style={{ alignSelf: "flex-start" }}
        >
          <Ic name="plus" cls="s12" />
          Add
        </button>
      </div>
    </div>
  );
}

function Comments({ s, store, t }: Pick<SheetProps, "s" | "store" | "t">) {
  return (
    <section>
      <span className="sname">Comments · {t.comments.length}</span>
      {t.comments.map((cm, i) => (
        <div key={String(i) + cm.who} className="cmt">
          <span className={"av av-" + cm.tool}>{cm.tool}</span>
          <div>
            <div className="cmt-h">
              <b>{cm.who}</b>
              <span className="cnt">
                {cm.at} · {cm.stage}
              </span>
            </div>
            <p>{cm.t}</p>
          </div>
        </div>
      ))}
      <div className="cbox">
        <input
          type="text"
          placeholder="Comment, or hand the next stage a note…"
          aria-label="Comment"
          value={s.cdraft}
          onChange={(e) => {
            store.setCDraft(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              store.addComment();
            }
          }}
        />
        <label className="cnt" style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <input
            type="checkbox"
            checked={s.tellNow}
            onChange={() => {
              store.toggleTell();
            }}
          />
          Also tell the agent now
        </label>
      </div>
    </section>
  );
}

function Term({ lines }: { lines: readonly Line[] }) {
  return (
    <>
      {lines.map((l, i) => (
        <div key={String(i)} className={"tl " + l.c}>
          {l.t}
        </div>
      ))}
    </>
  );
}

function Session({ s, store, t, b }: SheetProps) {
  return (
    <div className="sess-wrap">
      <div className="sw">
        {t.parts.map((pp) => (
          <button
            key={pp.id}
            className={"swb " + (pp.on ? "on" : "")}
            type="button"
            aria-pressed={pp.on}
            onClick={() => {
              store.setPart(pp.id);
            }}
          >
            {pp.dot && <span className="dot" style={{ background: pp.dot }}></span>}
            {pp.label}
          </button>
        ))}
        <span style={{ flex: 1 }}></span>
        <span className="cnt mono">otaq attach {t.k}</span>
      </div>
      {t.pieces.length > 0 && (
        <>
          <span className="sname">Sessions · 1 in charge · {t.helperCount} helpers</span>
          <div className="sess on">
            <span className="av av-CC">CC</span>Rust developer · in charge<span className="m">working</span>
          </div>
          {t.pieces.map((pc) => (
            <button
              key={pc.n}
              className="sess"
              type="button"
              onClick={() => {
                store.tip("helper");
              }}
            >
              <span className="av av-CC">CC</span>
              {pc.n} · Helper · {pc.t}
              <span className="m">{pc.s}</span>
            </button>
          ))}
        </>
      )}
      {s.part === "agent" && (
        <>
          <div className="tterm">
            {t.log.length === 0 && (
              <div className="tl d">
                No session yet. An agent starts when this ticket reaches its first agent stage.
              </div>
            )}
            <Term lines={t.log} />
            <div className="tprompt">
              <span>❯</span>
              <input
                type="text"
                placeholder="Type to the agent…"
                aria-label="Type to the agent"
                value={s.tdraft}
                onChange={(e) => {
                  store.setTDraft(e.target.value);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    store.sendToAgent();
                  }
                }}
              />
            </div>
          </div>
          <span className="cnt">The tool’s own session. Typing goes straight to it.</span>
        </>
      )}
      {s.part === "check" && (
        <>
          <div className="tterm">
            <Term lines={t.checkLog} />
          </div>
          <span className="cnt">The check agent’s own session, read-only.</span>
        </>
      )}
      {s.part === "run" && (
        <>
          {/* The canvas also shows the project's run command here, but its sample data has none: left out. */}
          <div className="runbar">
            <button
              className="btn btn-p btn-sm"
              type="button"
              onClick={() => {
                store.tip("run");
              }}
            >
              <Ic name="play" cls="s12" />
              Run
            </button>
            <span className="cnt">$PORT = {t.port}</span>
          </div>
          <div className="tterm">
            <div className="tl d">
              Not running. Run starts the project’s run command for this ticket on its own port, with an Open
              URL button when it listens.
            </div>
          </div>
        </>
      )}
      {s.part === "shell" && (
        <div className="tterm">
          <div className="tl d">
            ~/.otaq/wt/{b.proj.name}/{t.k}
          </div>
          <div className="tl">$ git status --short</div>
          <div className="tl o">{"M  src/cli.js"}</div>
          <div className="tprompt">
            <span>$</span>
            <input type="text" placeholder="A plain shell in the ticket’s worktree" aria-label="Shell" />
          </div>
        </div>
      )}
    </div>
  );
}

function Files({ t }: Pick<SheetProps, "t">) {
  if (t.files.length === 0)
    return (
      <div className="ph" style={{ padding: 28 }}>
        No changes yet. Files shows the diff once the agent commits.
      </div>
    );
  return (
    <>
      <div className="files-h">
        <span>Against where {t.k} started</span>
        <span className="seg" style={{ marginLeft: 8 }}>
          <span className="on" style={{ padding: "3px 8px", borderRadius: 5, fontSize: 11.5 }}>
            By file
          </span>
          <span style={{ padding: "3px 8px", fontSize: 11.5, color: "var(--tx2)" }}>By piece</span>
        </span>
        <span style={{ flex: 1 }}></span>
        <span className="add">+{t.adds}</span>
        <span className="del">−{t.dels}</span>
        <span className="cnt">{t.fileCount} files · 0 viewed</span>
      </div>
      {t.files.map((f) => (
        <div key={f.name} className="dfile">
          <div className="dfile-h">
            <Ic name="file-diff" cls="s12" />
            {f.name}
            <span style={{ flex: 1 }}></span>
            <span className="add">+{f.a}</span>
            <span className="del">−{f.d}</span>
            <label className="cnt" style={{ display: "flex", gap: 5, alignItems: "center", marginLeft: 10 }}>
              <input type="checkbox" />
              Viewed
            </label>
          </div>
          {f.lines.map(([c, n, text], i) => (
            <div key={String(i)} className={"dl " + c}>
              <span className="ln">{n}</span>
              <span className="tx">{text}</span>
            </div>
          ))}
        </div>
      ))}
    </>
  );
}
