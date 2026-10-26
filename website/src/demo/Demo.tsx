// The live demo on the landing page (WEB-07, decision 0102): a stand-in while the Demo3 port is built. The store, the
// data and the view functions (./state.ts, ./data.ts, ./view.ts) are Demo3's; piece 4 of OTQ-4 draws them here.

import "./demo.css";

import { useEffect, useState, useSyncExternalStore } from "react";

import { DemoStore } from "./state";
import { boardView } from "./view";

export default function Demo() {
  const [store] = useState(() => new DemoStore());
  const s = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  useEffect(
    () => () => {
      store.dispose();
    },
    [store],
  );
  const b = boardView(s);
  return (
    <div className="oq" aria-label="Live demo">
      {b.proj.name} · {b.flowLabel}
    </div>
  );
}
