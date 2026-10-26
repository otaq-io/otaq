import { Fragment } from "react";

import type { DemoProps } from "./Demo";
import { Ic } from "./Ic";
import { flowEditorView } from "./view";

/** Change flow…: the presets, the draft's steps with their details, Noyan on the flow, and Apply. */
export function FlowEditor({ s, store }: DemoProps) {
  const v = flowEditorView(s);
  return (
    // The dimmed board closes the editor for the mouse; Cancel and Esc do it from the keyboard.
    // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- Cancel and Esc are the keyboard path
    <div
      className="scrim"
      onClick={(e) => {
        if (e.target === e.currentTarget) store.closeFlow();
      }}
    >
      <div className="sheet" role="dialog" aria-label="Change flow">
        <div className="sh-l">
          <span className="lbl">Start from</span>
          {v.presets.map((fp) => (
            <button
              key={fp.id}
              className={"it " + (fp.on ? "on" : "")}
              type="button"
              aria-pressed={fp.on}
              onClick={() => {
                store.pickPreset(fp.id);
              }}
            >
              <b>{fp.name}</b>
              <span>{fp.desc}</span>
            </button>
          ))}
          <span className="lbl" style={{ marginTop: 8 }}>
            Made for shop-web
          </span>
          <button
            className="it"
            type="button"
            onClick={() => {
              store.tip("askTool");
            }}
          >
            <b>
              <span className="av av-CC">CC</span> Ask Claude Code
            </b>
            <span>reads how you use it</span>
          </button>
          <button
            className="it"
            type="button"
            onClick={() => {
              store.tip("askTool");
            }}
          >
            <b>
              <span className="av av-OC">OC</span> Ask OpenCode
            </b>
            <span>reads how you use it</span>
          </button>
          <button
            className="it"
            type="button"
            onClick={() => {
              store.tip("import");
            }}
          >
            <b>Import a flow…</b>
          </button>
        </div>
        <div className="sh-m">
          <div className="hd">
            {v.draftName}
            <span className="n">
              {v.draftCount} steps{v.draftInUse}
            </span>
          </div>
          {/* Keyed by the draft, so picking another preset replays the drawn pop-in. */}
          <div className="prev" key={v.draftName}>
            {v.stages.map((ds) => (
              <div key={ds.id} className="st" style={{ animationDelay: ds.delay }}>
                <b>
                  <span className="k" style={{ background: ds.color }}></span>
                  {ds.name}
                </b>
                <span>{ds.kind}</span>
                <span>{ds.who}</span>
              </div>
            ))}
          </div>
          <span className="slab">Steps · click one to see its details</span>
          {v.stages.map((ds) => (
            <Fragment key={ds.id}>
              <button
                className={"step " + (ds.sel ? "sel" : "")}
                type="button"
                aria-expanded={ds.sel}
                onClick={() => {
                  store.pickStage(ds.id);
                }}
              >
                <Ic name="grip" cls="s12" style={{ color: "var(--tx3)" }} />
                <span className="k" style={{ background: ds.color }}></span>
                {ds.name}
                <span className="kd">
                  {ds.kind} · {ds.who}
                </span>
              </button>
              {ds.sel && (
                <div className="det">
                  <b>Kind</b>
                  <span>{ds.kindLong}</span>
                  <b>Agent</b>
                  <span>{ds.who}</span>
                  <b>Skill</b>
                  <span className="mono">{ds.skill}</span>
                  <b>Fails go to</b>
                  <span>{ds.fail}</span>
                </div>
              )}
            </Fragment>
          ))}
          <button
            className="step"
            type="button"
            onClick={() => {
              store.tip("addStage");
            }}
            style={{ borderStyle: "dashed", justifyContent: "center", color: "var(--tx3)" }}
          >
            <Ic name="plus" cls="s12" />
            Add a stage
          </button>
        </div>
        <div className="sh-r">
          <span className="lbl">Noyan · the flow</span>
          <div className="msg">
            Ask about the flow, or to change it. Nothing on the board changes until you apply, and nothing
            runs until you ask.
          </div>
          {v.msgs.map((fm, i) => (
            <div key={String(i)} className={("msg " + fm.cls).trim()}>
              {fm.t}
            </div>
          ))}
          <div className="in">
            <input
              type="text"
              placeholder="“Why is Security here?”"
              aria-label="Ask about the flow"
              value={s.flowDraft}
              onChange={(e) => {
                store.setFlowDraft(e.target.value);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  store.sendFlowMsg();
                }
              }}
              style={{
                flex: 1,
                border: 0,
                background: "none",
                color: "var(--tx)",
                font: "12px Geist, sans-serif",
              }}
            />
            <span className="kbd">⌘L</span>
          </div>
        </div>
        <div className="sh-f">
          <span>{v.applyNote}</span>
          <span className="sp"></span>
          <button
            className="btn btn-g"
            type="button"
            onClick={() => {
              store.closeFlow();
            }}
          >
            Cancel
          </button>
          <button
            className="btn btn-p"
            type="button"
            onClick={() => {
              store.applyFlow();
            }}
          >
            Apply
          </button>
        </div>
      </div>
    </div>
  );
}
