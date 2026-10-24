// The typing hint (d0126): after a fast burst of letters with nothing focused ran nothing, a quiet line at the bottom
// centre, where the `g` hint shows, says "Typing? Click where you want to type" (Rust's words, `Keymap.typing_hint`).
// It comes and goes with the `toast` motion; when it shows and goes is the dispatcher's (`typingHint`).
import { useRef } from "react";

import "./sequencehint.css";
import { useEnterExit } from "../components";
import { keymap, typingHint } from "./keyboard";
import { useStore } from "./store";

function Shown({ text }: { text: string }) {
  const layer = useRef<HTMLDivElement>(null);
  useEnterExit(layer, "toast");
  return (
    <div ref={layer} className="g-hint-layer" role="status">
      <div className="g-hint">{text}</div>
    </div>
  );
}

/** Shows the hint while the dispatcher says so. */
export function TypingHint() {
  const shown = useStore(typingHint);
  const map = useStore(keymap);
  if (!shown || !map) return null;
  return <Shown text={map.typing_hint} />;
}
