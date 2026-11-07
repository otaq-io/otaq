// Settings → Limits & usage, its Usage part: what is left of each tool's plan, "15% left · resets 3:45 pm", amber at
// 20% left or less, as the Running popover shows it (`shell.activity`), asked again with each new header view; then
// what a tool's limit does.
import { useEffect, useState } from "react";

import { useStore } from "../../app/store";
import type { ActivityToolUsage } from "../../generated/ActivityToolUsage";
import type { ActivityPopover } from "../../generated/ActivityPopover";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { headerStore } from "../shell/Header";
import { UsageTools } from "../shell/ActivityPopover";
import { sectionOf, useSettings } from "./context";

/** The usage as drawn: each tool's windows, or nothing to show yet. */
export function UsageBody({
  title,
  tools,
  note,
}: {
  title: string | null;
  tools: readonly ActivityToolUsage[] | null;
  note: string | null;
}) {
  return (
    <section
      className="settings-section settings-usage-section"
      data-section="usage"
      aria-label={title ?? "usage"}
    >
      {title !== null && <h3 className="settings-section-title">{title}</h3>}
      <div className="settings-usage">{tools && <UsageTools tools={tools} />}</div>
      {note !== null && (
        <p className="settings-right-now">
          <Icon name="info" size={13} />
          <span>{note}</span>
        </p>
      )}
    </section>
  );
}

/** The registered section. */
export function UsageSection() {
  const { page } = useSettings();
  const section = sectionOf(page, "usage");
  const header = useStore(headerStore);
  const [tools, setTools] = useState<readonly ActivityToolUsage[] | null>(null);
  useEffect(() => {
    let live = true;
    request("shell.activity").then(
      (v) => {
        if (live) setTools((v as ActivityPopover).usage.tools);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [header]);
  if (!section) return null;
  return <UsageBody title={section.title} tools={tools} note={section.note} />;
}
