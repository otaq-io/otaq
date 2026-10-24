// First run (UI-08, UI-11, ONB-04, ONB-10; *14 · First run & onboarding*): over the whole window, its own title bar
// with the step indicator (Welcome · Theme · Tools · Project · Flow) between the OS's window controls, and one
// card per step: Welcome, Theme ("Pick a look", T-0217's picker: a click applies and saves), Tools (the agent tools
// and the checklist, Re-check, Continue only once an agent tool is connected; a row's Sign in…, Show or Hide checks
// again once done), Project (Add your first project), Flow (the flow editor from Otaq default, `FlowEditorPage`;
// FED-09, `s27-03`: Use this flow · Skip, use the default). The Flow step ends it on the project's board (*6 · Done*)
// once the flow is settled; with the warning shown and not closed, the permission-mode banner follows (T-0231).
// Quitting during the Flow step leaves the project in Flow setup, and the next start reopens onboarding on its Flow
// step (ASK-04).
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps } from "../../app/registry";
import { useRegistries } from "../../app/registry";
import { useStore } from "../../app/store";
import { Button, pickTheme, ThemePicker, useThemePicker } from "../../components";
import type { OnboardingStepId } from "../../generated/OnboardingStepId";
import type { OnboardingTexts } from "../../generated/OnboardingTexts";
import type { OnboardingTools } from "../../generated/OnboardingTools";
import type { OnboardingTitleBar } from "../../generated/OnboardingTitleBar";
import { asRequestError } from "../../service";
import { DRAG_REGION, request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { AddButton, AddProjectFields, useAddProject } from "./AddProjectForm";
import { askUsageDataWhenNeeded } from "./UsageData";
import { agentStillOffered, useAgentToolActions } from "../settings/AgentTools";
import { useRowTerminals } from "./RowTerminal";
import { checklistStillOffered } from "./Tools";
import { droppedFolder } from "./drops";
import { FlowEditorPage } from "../flow_editor/FlowEditor";
import { afterFlowStep, afterTools, finishOnboarding, STEP_ORDER } from "./logic";
import { ToolsList } from "./Tools";

/** The title bar: the OS's controls (the title bar screen's two ends), the product name and the steps. */
function TitleBar({ bar }: { bar: OnboardingTitleBar | undefined }) {
  const app = useRegistries();
  const Start = app.slots.get("title_bar_start")?.Component;
  const End = app.slots.get("title_bar_end")?.Component;
  return (
    <div className="onboarding-titlebar" {...DRAG_REGION}>
      {Start && <Start />}
      <span className="onboarding-product">{bar?.product}</span>
      <span className="onboarding-spacer" />
      <ol className="onboarding-steps">
        {bar?.steps.map((dot) => (
          <li key={dot.label} className="onboarding-step" data-state={dot.state}>
            <span className="onboarding-step-badge">
              {dot.state === "done" ? <Icon name="check" size={11} /> : dot.number}
            </span>
            <span className="onboarding-step-label">{dot.label}</span>
          </li>
        ))}
      </ol>
      <span className="onboarding-spacer" />
      {End && <End />}
    </div>
  );
}

function Card({ children, wide = false }: { children: ReactNode; wide?: boolean }) {
  return (
    <section className="onboarding-card" data-wide={wide || undefined}>
      {children}
    </section>
  );
}

function ProjectStep({ onAdded, drop }: { onAdded: (key: string) => void; drop: string | null }) {
  const state = useAddProject(true, drop, onAdded);
  const { view } = state;
  return (
    <Card>
      <h1 className="onboarding-title">{view?.title}</h1>
      {view?.text !== null && <p className="onboarding-text">{view?.text}</p>}
      <AddProjectFields state={state} />
      <div className="onboarding-foot">
        <span className="onboarding-foot-text">{view?.footnote}</span>
        <AddButton state={state} />
      </div>
    </Card>
  );
}

export function OnboardingOverlay({ params, close }: OverlayProps<"onboarding">) {
  const first = params?.step ?? "welcome";
  const [step, setStep] = useState<OnboardingStepId>(first);
  const [texts, setTexts] = useState<OnboardingTexts>();
  const [tools, setTools] = useState<OnboardingTools>();
  const [checking, setChecking] = useState(first === "tools");
  const [dismissed, setDismissed] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  // The project just added, or the one still in Flow setup when onboarding reopened on its Flow step: the Flow step's.
  const [added, setAdded] = useState<string | null>(params?.project ?? null);
  const picker = useThemePicker().view;
  const drop = useStore(droppedFolder);
  const focusRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    request("onboarding.texts").then(
      (answer) => {
        setTexts(answer as OnboardingTexts);
      },
      (error: unknown) => {
        setFailure(asRequestError(error).message);
      },
    );
  }, []);

  const runCheck = useCallback(
    () =>
      request("onboarding.tools").then(
        (answer) => {
          setTools(answer as OnboardingTools);
          setChecking(false);
        },
        (error: unknown) => {
          setChecking(false);
          setFailure(asRequestError(error).message);
        },
      ),
    [],
  );

  // An agent tool's row acts through `agent_tools.*`; the step then checks again (the page it answers is Settings').
  const ignorePage = useCallback(() => undefined, []);
  const recheckAfter = useCallback(() => {
    setChecking(true);
    void runCheck();
  }, [runCheck]);
  // One Install / Sign in… terminal for the agent tools and git, gh, az; `tools.check` checks every tool again.
  const terminals = useRowTerminals(
    (key) =>
      agentStillOffered(tools?.agent_tools.rows ?? [], key) || checklistStillOffered(tools?.rows ?? [], key),
    () => {
      setChecking(true);
      return runCheck();
    },
  );
  const agentTools = useAgentToolActions(ignorePage, setFailure, recheckAfter, terminals);

  // Opened on the tools step (Check tools on the board): the check runs at once.
  useEffect(() => {
    if (first === "tools") void runCheck();
  }, [first, runCheck]);

  // Each step's first button takes the keyboard.
  useEffect(() => {
    focusRef.current?.querySelector<HTMLElement>(".onboarding-card .button[data-variant='primary']")?.focus();
  }, [step, texts, tools]);

  const check = () => {
    setStep("tools");
    setChecking(true);
    void runCheck();
  };

  const warningShown = tools?.warning !== null && tools?.warning !== undefined && !dismissed;
  const finish = () => {
    finishOnboarding(warningShown, request).catch(() => undefined);
    close();
    // The last question of first run: anonymous usage data, asked once (d0085).
    askUsageDataWhenNeeded(request, (dialog) => {
      openOverlay("usage_data", dialog);
    }).catch(() => undefined);
  };

  const bar = texts?.title_bars[STEP_ORDER.indexOf(step)];
  let body: ReactNode = null;
  if (texts) {
    switch (step) {
      case "welcome":
        body = (
          <Card>
            <h1 className="onboarding-title onboarding-title-large">{texts.welcome.title}</h1>
            <p className="onboarding-text onboarding-text-large">{texts.welcome.text}</p>
            <ul className="onboarding-points">
              {texts.welcome.points.map((point) => (
                <li key={point.title} className="onboarding-point">
                  <span className="onboarding-point-icon">
                    <Icon name={point.icon} size={16} />
                  </span>
                  <span className="onboarding-point-text">
                    <span className="onboarding-point-title">{point.title}</span>
                    <span>{point.text}</span>
                  </span>
                </li>
              ))}
            </ul>
            <div className="onboarding-foot">
              <span className="onboarding-foot-text">{texts.welcome.foot}</span>
              <Button
                label={texts.welcome.button}
                icon="arrow-right"
                onClick={() => {
                  setStep("theme");
                }}
              />
            </div>
          </Card>
        );
        break;
      case "theme":
        body = (
          <Card wide>
            <h1 className="onboarding-title">{texts.theme.title}</h1>
            <p className="onboarding-text">{texts.theme.text}</p>
            {picker && (
              <ThemePicker
                sections={picker}
                onPick={(key) => {
                  void pickTheme(key);
                }}
              />
            )}
            <div className="onboarding-foot">
              <span className="onboarding-foot-text">{texts.theme.foot}</span>
              <Button label={texts.theme.button} icon="arrow-right" onClick={check} />
            </div>
          </Card>
        );
        break;
      case "tools":
        body = (
          <Card>
            {tools ? (
              <ToolsList
                tools={tools}
                checking={checking}
                warning={warningShown}
                onDismiss={() => {
                  setDismissed(true);
                  request("onboarding.dismiss_warning").catch(() => undefined);
                }}
                onRecheck={() => {
                  setChecking(true);
                  void runCheck();
                }}
                onContinue={() => {
                  const next = afterTools(tools, added);
                  if (next === "done") finish();
                  else setStep(next);
                }}
                onAgentAction={agentTools.act}
                terminals={terminals}
              />
            ) : (
              <p className="onboarding-text">{texts.checking}</p>
            )}
          </Card>
        );
        break;
      case "project":
        body = (
          <ProjectStep
            key={drop?.seq ?? 0}
            drop={drop?.path ?? null}
            onAdded={(key) => {
              setAdded(key);
              setStep("flow");
            }}
          />
        );
        break;
      case "flow":
        if (added !== null) {
          body = (
            <FlowEditorPage
              key={added}
              project={added}
              onDone={(agent) => {
                afterFlowStep(added, agent);
                finish();
              }}
            />
          );
        }
        break;
    }
  }

  return (
    <div
      className="onboarding"
      role="dialog"
      aria-modal="true"
      aria-label={bar?.product ?? ""}
      ref={focusRef}
    >
      <TitleBar bar={bar} />
      <div className="onboarding-body">
        {body}
        {failure !== null && <p className="onboarding-failure">{failure}</p>}
      </div>
      {agentTools.dialogs}
    </div>
  );
}
