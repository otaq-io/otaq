// Settings → Agents (T-0410; AGM-02, AGM-03, AGM-04, AGT-05's marks; d0108, d0078, d0086, d0092; *15 → Agents
// (update AG §8)* `s15-58` / `s15-59`, the model picker `s15-30`, the mark `s15-32`): the default model for new agents,
// "Noyan uses <agent>" (the one setting for Noyan's agent) and the list (NAME · MODEL · USED IN, each row's ⋯
// Duplicate / Delete…); and one agent's page in the same place (← name, its Model with the tool's effort, its
// Instructions in the Markdown editor, Used in, then Delete agent, Cancel and Save). An agent has no role: each stage
// names its agent. An agent has no skill list (T7). An agent's edits are kept only on Save. Everything comes from the
// views (`settings.agents`, `settings.agent` and the answers of the calls below); this keeps which page is open, the
// open menu and dialog, and the text being typed until it is saved.
import { type ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { AnchoredMenu, Button, ConfirmDialog, IconButton, Select, TextInput } from "../../components";
import type { AgentAddOutcome } from "../../generated/AgentAddOutcome";
import type { AgentChange } from "../../generated/AgentChange";
import type { AgentDeleteDialog } from "../../generated/AgentDeleteDialog";
import type { AgentDeleteOutcome } from "../../generated/AgentDeleteOutcome";
import type { AgentInstructionsField } from "../../generated/AgentInstructionsField";
import type { AgentListRow } from "../../generated/AgentListRow";
import type { AgentMarkView } from "../../generated/AgentMarkView";
import type { AgentOption } from "../../generated/AgentOption";
import type { AgentPage } from "../../generated/AgentPage";
import type { AgentPageParams } from "../../generated/AgentPageParams";
import type { AgentReadsTarget } from "../../generated/AgentReadsTarget";
import type { AgentReadsView } from "../../generated/AgentReadsView";
import type { AgentsSection as AgentsSectionView } from "../../generated/AgentsSection";
import type { MenuItem } from "../../generated/MenuItem";
import type { RequestError } from "../../generated/RequestError";
import {
  MarkdownSurface,
  MarkdownToolbar,
  openingMode,
  rememberMode,
  SourceArea,
  SourceToggle,
  useFocusOnLeave,
  useMarkdownEditor,
  useSourceMode,
} from "../../markdown";
import { asRequestError, watchView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { useSelection } from "../../app/selection";
import { AgentReadsBody, AgentReadsHeading } from "./AgentReads";
import { SettingsContext } from "./context";
import { proposeFrom } from "./flowActions";
import { ModelDropdown, type ModelPick, ModelPicker } from "./ModelPicker";

/** The amber line of a mark, the model in mono. */
export function MarkLine({ mark }: { mark: AgentMarkView }) {
  return (
    <span className="settings-agent-mark-line">
      <Icon name="triangle-alert" size={12} />
      <span>
        {mark.before}
        {mark.model !== null && <code>{mark.model}</code>}
        {mark.after}
      </span>
    </span>
  );
}

/** A small select's choices (the tool, the effort) as menu rows, the current one checked. */
export function optionItems(options: readonly AgentOption[]): MenuItem[] {
  return options.map((o) => ({
    id: o.id,
    label: o.label,
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked: o.selected,
    header: false,
  }));
}

/** What a picked option sends: its id, or none for the empty id (the default model, the tool's effort). */
export function optionValue(id: string): string | null {
  return id === "" ? null : id;
}

/** A row's ⋯ button and its menu (Duplicate, Delete…). */
function MoreMenu({
  label,
  items,
  onChoose,
}: {
  label: string;
  items: readonly MenuItem[];
  onChoose: (id: string) => void;
}) {
  const more = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  return (
    <>
      <IconButton
        ref={more}
        icon="ellipsis"
        label={label}
        onClick={() => {
          setOpen((o) => !o);
        }}
      />
      {open && (
        <AnchoredMenu
          anchor={more}
          align="end"
          label={label}
          items={items}
          onChoose={(id) => {
            setOpen(false);
            onChoose(id);
          }}
          onClose={(how) => {
            setOpen(false);
            if (how === "key") more.current?.focus();
          }}
        />
      )}
    </>
  );
}

/** What the page asks the section to do. */
export interface AgentsActions {
  onOpen: (params: AgentPageParams) => void;
  onMenu: (id: string, action: string) => void;
  onDefaultModel: (pick: ModelPick) => void;
  /** "Noyan uses <agent>": the agent's id. */
  onNoyanAgent: (agent: string) => void;
}

function Row({ row, actions }: { row: AgentListRow; actions: AgentsActions }) {
  const open = () => {
    actions.onOpen({ id: row.id, duplicate: null, draft: null, change: null });
  };
  return (
    <div className="settings-agents-row" role="row">
      <span className="settings-agents-name" role="cell">
        <button type="button" className="settings-agents-open" onClick={open}>
          <span className="settings-agents-title">
            <span>{row.name}</span>
          </span>
          <span className="settings-agents-came">{row.came_from}</span>
        </button>
      </span>
      <span className="settings-agents-model" role="cell" data-marked={row.mark ? true : undefined}>
        {row.model}
      </span>
      <span className="settings-agents-use" role="cell">
        {row.used_in}
      </span>
      <span className="settings-agents-more" role="cell">
        <MoreMenu
          label={row.menu_label}
          items={row.menu}
          onChoose={(action) => {
            actions.onMenu(row.id, action);
          }}
        />
      </span>
      {row.mark && (
        <span className="settings-agents-mark">
          <MarkLine mark={row.mark} />
          {row.mark_link !== null && (
            <>
              <span className="settings-agents-dot">·</span>
              <button type="button" className="settings-link" onClick={open}>
                {row.mark_link}
              </button>
            </>
          )}
        </span>
      )}
    </div>
  );
}

/** The Agents page as the view says (`s15-58`): default model, the agent Noyan uses, the list. */
/** Rows the skeleton draws for the agents list. */
const SKELETON_ROWS = 4;

/** Settings → Agents while `settings.agents` hasn't answered yet: the page's shape in grey bars, no words. */
export function AgentsSkeleton() {
  return (
    <section className="settings-section settings-agents settings-agents-skeleton" aria-busy="true">
      <span className="settings-skeleton-bar settings-skeleton-intro" />
      <div className="settings-agents-settings">
        {[0, 1].map((i) => (
          <div key={i} className="settings-agents-setting">
            <span className="settings-row-text">
              <span className="settings-skeleton-bar settings-skeleton-label" />
              <span className="settings-skeleton-bar settings-skeleton-description" />
            </span>
            <span className="settings-skeleton-bar settings-skeleton-control" />
          </div>
        ))}
      </div>
      <div className="settings-agents-table">
        {Array.from({ length: SKELETON_ROWS + 1 }, (_, i) => (
          <div key={i} className="settings-agents-row">
            {[0, 1, 2].map((c) => (
              <span key={c} className="settings-skeleton-bar" />
            ))}
          </div>
        ))}
      </div>
    </section>
  );
}

export function AgentsSectionBody({
  section,
  actions,
}: {
  section: AgentsSectionView;
  actions: AgentsActions;
}) {
  return (
    <section className="settings-section settings-agents" data-section="agents" aria-label={section.title}>
      <p className="settings-intro">{section.intro}</p>
      <div className="settings-agents-settings">
        <div className="settings-agents-setting">
          <span className="settings-row-text">
            <span className="settings-row-label">{section.default_model.label}</span>
            <span className="settings-row-description">{section.default_model.description}</span>
          </span>
          <span className="settings-agents-setting-control">
            <ModelDropdown
              label={section.default_model.label}
              dropdown={section.default_model.dropdown}
              onPick={actions.onDefaultModel}
            />
          </span>
        </div>
        <div className="settings-agents-setting">
          <span className="settings-row-text">
            <span className="settings-row-label">{section.noyan.label}</span>
            <span className="settings-row-description">{section.noyan.description}</span>
          </span>
          <span className="settings-agents-setting-control">
            <Select
              label={section.noyan.label}
              value={section.noyan.value}
              items={optionItems(section.noyan.options)}
              onChange={(id) => {
                if (!section.noyan.options.some((o) => o.id === id && o.selected)) actions.onNoyanAgent(id);
              }}
            />
          </span>
        </div>
      </div>
      <div className="settings-agents-table" role="table" aria-label={section.title}>
        <div className="settings-agents-row settings-agents-head" role="row">
          {section.columns.map((column) => (
            <span key={column} role="columnheader">
              {column}
            </span>
          ))}
        </div>
        {section.rows.map((row) => (
          <Row key={row.id} row={row} actions={actions} />
        ))}
      </div>
    </section>
  );
}

/** What one agent's page asks for. `onChange` and `onAdd` carry the name and instructions as typed. */
export interface PageActions {
  onChange: (change: AgentChange, name: string, instructions: string) => void;
  /** Add agent on a draft, Save on a saved agent. */
  onAdd: (name: string, instructions: string) => void;
  onCancel: () => void;
  onDelete: () => void;
  onRetry: (tool: string) => void;
}

/** One agent's page (`s15-58` "One agent"). Everything is held in the page's draft: a saved agent's edits are kept
 * only on Save, and Cancel drops them; New agent and Duplicate are kept on Add agent. */
export function AgentPageBody({ page, actions }: { page: AgentPage; actions: PageActions }) {
  const [name, setName] = useState(page.name.value);
  const [instructions, setInstructions] = useState(page.instructions.value);
  // A fresh page with another name (saved, or a refused one sent back) replaces what is typed.
  const [shownName, setShownName] = useState(page.name.value);
  if (shownName !== page.name.value) {
    setShownName(page.name.value);
    setName(page.name.value);
  }
  const buttons = page.add;
  const unchanged =
    buttons !== null &&
    buttons.unchanged &&
    name === page.name.value &&
    instructions === page.instructions.value;
  const model = page.model;
  const change = (c: AgentChange) => {
    actions.onChange(c, name, instructions);
  };
  return (
    <section className="settings-section settings-agent-page" data-section="agent" aria-label={page.title}>
      {page.mark && (
        <div className="settings-agent-mark">
          <MarkLine mark={page.mark} />
          {page.mark.message !== null && (
            <span className="settings-agent-mark-message">“{page.mark.message}”</span>
          )}
        </div>
      )}
      <label className="settings-agent-label" htmlFor="agent-name">
        {page.name.label}
      </label>
      <TextInput id="agent-name" value={name} invalid={page.name.problem !== null} onChange={setName} />
      {page.name.problem !== null && <span className="settings-agent-problem">{page.name.problem}</span>}

      <span className="settings-agent-label">{model.label}</span>
      <span className="settings-agent-model">
        <Select
          label={model.tool_label}
          value={model.tool_value}
          items={optionItems(model.tools)}
          onChange={(id) => {
            change({ field: "tool", value: optionValue(id) });
          }}
        />
        {model.picker && (
          <ModelPicker
            label={model.label}
            picker={model.picker}
            onPick={(picked) => {
              change({ field: "model", value: picked });
            }}
            onRetry={actions.onRetry}
          />
        )}
        {model.default_now !== null && (
          <span className="settings-agent-default-now">{model.default_now}</span>
        )}
        {model.effort && (
          <span className="settings-agent-effort">
            <span>{model.effort.label}</span>
            <Select
              label={model.effort.label}
              value={model.effort.value}
              items={optionItems(model.effort.options)}
              onChange={(id) => {
                change({ field: "effort", value: optionValue(id) });
              }}
            />
          </span>
        )}
      </span>
      {model.problem !== null ? (
        <span className="settings-agent-problem">{model.problem}</span>
      ) : (
        model.picker?.status === null &&
        model.picker.value.no_longer_listed !== null && (
          <span className="settings-agent-hint">{model.picker.hint}</span>
        )
      )}

      <span className="settings-agent-label">{page.instructions.label}</span>
      <Instructions field={page.instructions} value={instructions} onChange={setInstructions} />

      {page.used_in && (
        <>
          <span className="settings-agent-label">{page.used_in.label}</span>
          <span className="settings-agent-used">
            {page.used_in.items.map((item) => (
              <code key={item} className="settings-agent-place">
                {item}
              </code>
            ))}
            {page.used_in.empty !== null && <span>{page.used_in.empty}</span>}
          </span>
        </>
      )}
      {page.note !== null && (
        <p className="settings-agent-note">
          <Icon name="info" size={14} />
          <span>{page.note}</span>
        </p>
      )}
      {page.problem !== null && <span className="settings-agent-problem">{page.problem}</span>}
      {buttons && (
        <span className="settings-agent-add">
          {buttons.delete_label !== null && (
            <button type="button" className="settings-agent-delete" onClick={actions.onDelete}>
              {buttons.delete_label}
            </button>
          )}
          <Button variant="ghost" label={buttons.cancel_label} onClick={actions.onCancel} />
          <Button
            variant="primary"
            label={buttons.add_label}
            disabled={unchanged}
            onClick={() => {
              actions.onAdd(name, instructions);
            }}
          />
        </span>
      )}
    </section>
  );
}

/** Instructions in the Markdown editor (d0078), with its toolbar and its Markdown source switch (D9), which opens in the
 * mode last chosen and is remembered for every editor. */
function Instructions({
  field,
  value,
  onChange,
}: {
  field: AgentInstructionsField;
  value: string;
  onChange: (text: string) => void;
}) {
  const mode = useSourceMode(value, openingMode(false));
  const editor = useMarkdownEditor({
    value: mode.editorValue,
    onChange,
    placeholder: field.placeholder,
    label: field.label,
  });
  useFocusOnLeave(editor, mode);
  const toggle = (on: boolean) => {
    mode.toggle();
    rememberMode(on);
    request("editor.source_mode", { on }).catch(() => undefined);
  };
  return (
    <MarkdownSurface
      editor={editor}
      className="settings-agent-instructions"
      toolbar={
        <MarkdownToolbar
          disabled={mode.on}
          expand={<></>}
          sourceToggle={<SourceToggle on={mode.on} label={field.markdown_label} onChange={toggle} />}
        />
      }
      {...(mode.on
        ? {
            source: (
              <SourceArea value={value} label={field.label} takeFocus={mode.switched} onChange={onChange} />
            ),
          }
        : {})}
    />
  );
}

/** One agent's heading in the Settings header: ← name, where it came from. */
function AgentHeading({ page, onBack }: { page: AgentPage; onBack: () => void }) {
  return (
    <span className="settings-agent-heading">
      <span className="settings-agent-heading-line">
        <IconButton icon="arrow-left" label={page.back_label} onClick={onBack} />
        <h2>{page.title}</h2>
      </span>
      {page.came_from !== null && <span className="settings-agents-came">{page.came_from}</span>}
    </span>
  );
}

type Asking = { kind: "delete"; dialog: AgentDeleteDialog } | null;

/** The global page's Agents topic: its page, or one agent's. It shows Otaq's copy of the model lists at once, then
 * asks the tools for their lists again (once per opening, d0086) and redraws with them. */
export function AgentsSection() {
  // The project on screen: the shipped agents file's flow editor is its (FLW-10).
  const screenProject = useSelection().project;
  const settings = useContext(SettingsContext);
  // `undefined` outside Settings (the gallery, tests): the header's buttons are drawn above the page instead.
  const headerSlot = settings?.headerSlot;
  const setHeading = settings?.setHeading;
  const [section, setSection] = useState<AgentsSectionView | null>(null);
  const [page, setPage] = useState<AgentPage | null>(null);
  // What this agent reads, open over its page (AGR-01).
  const [reads, setReads] = useState<AgentReadsView | null>(null);
  const [error, setError] = useState<RequestError | null>(null);
  const [asking, setAsking] = useState<Asking>(null);
  const failed = (e: unknown) => {
    setError(asRequestError(e));
  };

  useEffect(
    () =>
      watchView(
        () => request("settings.agents", { refresh: false }) as Promise<AgentsSectionView>,
        // A tool connected or hidden in Agent tools changes the default model and the models' words.
        ["agents_changed", "agent_tools_changed"],
        (state) => {
          if (state.view) setSection(state.view);
          setError(state.error ?? null);
        },
      ),
    [],
  );
  useEffect(() => {
    let live = true;
    request("settings.agents", { refresh: true }).then(
      (s) => {
        if (live) setSection(s as AgentsSectionView);
      },
      // The copy already shown stays; the next opening asks again.
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, []);

  // Opened on one agent (Open agent… on a card): its page, once.
  const openedOn = useRef(false);
  const startAgent = settings?.agent;
  useEffect(() => {
    if (startAgent === undefined || openedOn.current) return;
    openedOn.current = true;
    request("settings.agent", { id: startAgent, duplicate: null, draft: null, change: null }).then(
      (fresh) => {
        setPage(fresh as AgentPage);
      },
      (e: unknown) => {
        setError(asRequestError(e));
      },
    );
  }, [startAgent]);

  const back = useCallback(() => {
    setPage(null);
    setReads(null);
    setError(null);
  }, []);
  const backToPage = useCallback(() => {
    setReads(null);
    setError(null);
  }, []);
  // A nav click goes back to the list.
  const visit = settings?.visit;
  const [seenVisit, setSeenVisit] = useState(visit);
  if (seenVisit !== visit) {
    setSeenVisit(visit);
    setPage(null);
    setReads(null);
  }
  // The header shows the open agent, and the topic's label again without one.
  useEffect(() => {
    setHeading?.(
      reads ? (
        <AgentReadsHeading view={reads} onBack={backToPage} />
      ) : page ? (
        <AgentHeading page={page} onBack={back} />
      ) : null,
    );
  }, [page, reads, setHeading, back, backToPage]);
  useEffect(() => () => setHeading?.(null), [setHeading]);
  // One agent's page (or what it reads) is drawn alone, without the tools above the list.
  const setSolo = settings?.setSolo;
  const own = page !== null || reads !== null;
  useEffect(() => {
    setSolo?.(own ? "agents" : null);
  }, [own, setSolo]);
  useEffect(() => () => setSolo?.(null), [setSolo]);

  const show = (p: Promise<unknown>) => {
    p.then((fresh) => {
      setPage(fresh as AgentPage);
      setError(null);
    }, failed);
  };
  const open = (params: AgentPageParams) => {
    show(request("settings.agent", params));
  };
  // What this agent reads, on `project` (the picked one; first the project on screen, else the first).
  const openReads = (id: string, project: string | null) => {
    request("settings.agent_reads", { id, project }).then((fresh) => {
      setReads(fresh as AgentReadsView);
      setError(null);
    }, failed);
  };
  const readsLink = (view: AgentReadsView, target: AgentReadsTarget) => {
    if (target.kind === "agent") backToPage();
    else if (target.kind === "project_notes") settings?.openProject?.(target.project, "project_notes");
    else if (view.project !== null)
      request("settings.agent_reads_reveal", {
        id: view.agent_id,
        project: view.project,
        skill: target.skill,
      }).catch(failed);
  };
  const listed = (p: Promise<unknown>) => {
    p.then((fresh) => {
      setSection(fresh as AgentsSectionView);
      setError(null);
    }, failed);
  };
  const askDelete = (id: string) => {
    request("settings.agent_delete_dialog", { id }).then((d) => {
      setAsking({ kind: "delete", dialog: d as AgentDeleteDialog });
    }, failed);
  };
  const menu = (id: string, action: string) => {
    if (action === "duplicate") open({ id: null, duplicate: id, draft: null, change: null });
    else if (action === "delete") askDelete(id);
  };

  const actions: AgentsActions = {
    onOpen: open,
    onMenu: menu,
    onDefaultModel: (pick) => {
      if (pick) listed(request("settings.agents_default_model", pick));
    },
    onNoyanAgent: (agent) => {
      listed(request("settings.noyan_agent", { id: agent }));
    },
  };

  const pageActions = (p: AgentPage): PageActions => ({
    onChange: (change, name, instructions) => {
      show(
        request("settings.agent", {
          id: p.id,
          duplicate: null,
          draft: { ...p.draft, name, instructions },
          change,
        }),
      );
    },
    // Add agent on a draft; Save on a saved agent, the only moment its edits are kept.
    onAdd: (name, instructions) => {
      const draft = { ...p.draft, name, instructions };
      const saving =
        p.id === null
          ? request("settings.agent_add", { draft })
          : request("settings.agent_save", { id: p.id, draft });
      saving.then((o) => {
        const outcome = o as AgentAddOutcome;
        // A saved agent's Save goes back to the list, as Cancel does; a new agent opens its page.
        if (outcome.outcome === "added" && p.id !== null) back();
        else setPage(outcome.page);
        setError(null);
      }, failed);
    },
    onCancel: back,
    onDelete: () => {
      if (p.id !== null) askDelete(p.id);
    },
    onRetry: (tool) => {
      request("settings.agents_models_retry", { tool }).then(() => {
        open({ id: p.id, duplicate: null, draft: p.draft, change: null });
      }, failed);
    },
  });

  let header: ReactNode = null;
  if (page && !reads) {
    const id = page.id;
    header = id !== null && (
      <>
        {page.reads_label !== null && (
          <Button
            variant="secondary"
            compact
            icon="eye"
            label={page.reads_label}
            onClick={() => {
              openReads(id, screenProject);
            }}
          />
        )}
        {page.menu.length > 0 && (
          <MoreMenu
            label={page.menu_label}
            items={page.menu}
            onChoose={(a) => {
              menu(id, a);
            }}
          />
        )}
      </>
    );
  } else if (!page && section) {
    header = (
      <>
        <Button
          variant="secondary"
          icon="plus"
          label={section.new_agent_label}
          onClick={() => {
            open({ id: null, duplicate: null, draft: null, change: null });
          }}
        />
        {/* Apply the shipped agents file again (FLW-10): the flow editor, for the project on screen. */}
        <MoreMenu
          label={section.more_label}
          items={section.more.map((m) =>
            m.id === "agents_file" && screenProject === null ? { ...m, disabled: true } : m,
          )}
          onChoose={(id) => {
            if (id === "agents_file" && screenProject !== null)
              proposeFrom(screenProject, { from: "agents_file" }).catch(failed);
          }}
        />
      </>
    );
  }

  return (
    <>
      {headerSlot && header ? createPortal(header, headerSlot) : null}
      {headerSlot === undefined && header && <div className="settings-agents-header">{header}</div>}
      {error && (
        <p className="settings-problem" role="alert">
          {error.message}
        </p>
      )}
      {reads ? (
        <AgentReadsBody
          view={reads}
          actions={{
            onProject: (project) => {
              openReads(reads.agent_id, project);
            },
            onLink: (target) => {
              readsLink(reads, target);
            },
          }}
        />
      ) : page ? (
        <AgentPageBody key={page.id ?? "draft"} page={page} actions={pageActions(page)} />
      ) : section ? (
        <AgentsSectionBody section={section} actions={actions} />
      ) : (
        !error && <AgentsSkeleton />
      )}
      {asking?.kind === "delete" &&
        (asking.dialog.kind === "refused" ? (
          <ConfirmDialog
            title={asking.dialog.title}
            body={asking.dialog.text}
            confirmLabel={null}
            cancelLabel={asking.dialog.ok_label}
            hint="close"
            onConfirm={() => {
              setAsking(null);
            }}
            onCancel={() => {
              setAsking(null);
            }}
          />
        ) : (
          <ConfirmDialog
            title={asking.dialog.title}
            body={asking.dialog.text}
            confirmLabel={asking.dialog.delete_label}
            cancelLabel={asking.dialog.cancel_label}
            destructive
            onConfirm={() => {
              if (asking.dialog.kind !== "confirm") return;
              const id = asking.dialog.id;
              setAsking(null);
              request("settings.agent_delete", { id }).then((o) => {
                const outcome = o as AgentDeleteOutcome;
                if (outcome.outcome === "deleted") {
                  setSection(outcome.section);
                  setPage(null);
                } else setAsking({ kind: "delete", dialog: outcome.dialog });
              }, failed);
            }}
            onCancel={() => {
              setAsking(null);
            }}
          />
        ))}
    </>
  );
}
