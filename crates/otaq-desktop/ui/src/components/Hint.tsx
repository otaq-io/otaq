// One-time hint: a concept explained at the moment it first matters, never behind a setting. A dark popover (dark in
// every theme, like a tooltip) next to the element it explains, with one sentence and one button. Only that button
// dismisses it, for good; Esc, a press elsewhere or a scroll leave it up (it follows its anchor), so a hint is never
// lost to a stray click before it was read. The service keeps the dismissed hints per data folder.
import { useEffect, useRef, useState } from "react";

import type { HintId } from "../generated/HintId";
import type { HintView } from "../generated/HintView";
import { request } from "../tauri";
import { Button } from "./Button";
import { type Anchor, useFloatingStyle } from "./Floating";

export interface HintPopoverProps {
  /** The element the hint explains. */
  anchor: Anchor;
  /** Its title, when it has one ("An agent needs you"). */
  title?: string | null | undefined;
  /** The note. */
  text: string;
  /** The one button's label ("Got it"). */
  button: string;
  align?: "start" | "end" | undefined;
  onDismiss: () => void;
}

/** The popover itself, drawn from what a hint's view says. */
export function HintPopover({ anchor, title, text, button, align, onDismiss }: HintPopoverProps) {
  const floating = useRef<HTMLDivElement>(null);
  // No `onDismiss` here: nothing but the button closes it, and it follows its anchor.
  const style = useFloatingStyle(anchor, floating, align);
  return (
    <div ref={floating} className="hint" role="status" style={style}>
      {title !== null && title !== undefined && <p className="hint-title">{title}</p>}
      <p className="hint-text">{text}</p>
      <Button variant="primary" compact label={button} onClick={onDismiss} />
    </div>
  );
}

/** Asks for a hint's view: `null` once the user dismissed it (or when the service can't say). */
export async function loadHint(id: HintId): Promise<HintView | null> {
  try {
    return (await request("hints.view", { hint: id })) as HintView | null;
  } catch {
    return null;
  }
}

export interface OneTimeHintProps {
  hint: HintId;
  anchor: Anchor;
  align?: "start" | "end" | undefined;
}

/**
 * Hint `hint` next to `anchor`, until the user dismisses it: then never again. Render it at the hint's moment (the
 * first agent question, the first ticket in Review…); it shows nothing once dismissed.
 */
export function OneTimeHint({ hint, anchor, align }: OneTimeHintProps) {
  const [view, setView] = useState<HintView | null>(null);
  useEffect(() => {
    let current = true;
    void loadHint(hint).then((v) => {
      if (current) setView(v);
    });
    return () => {
      current = false;
    };
  }, [hint]);
  if (view === null) return null;
  return (
    <HintPopover
      anchor={anchor}
      title={view.title}
      text={view.text}
      button={view.button}
      align={align}
      onDismiss={() => {
        setView(null);
        void request("hints.dismiss", { hint }).catch(() => undefined);
      }}
    />
  );
}
