//! Sample cards from the `shop-web` project (design-briefs/00-README.md), one per variant of *04 · Ticket card
//! variants → Ticket Card v2 · all variants* (the M21 ones). The gallery draws them; tests build on them.

use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Actor, Flags, Priority, Status};

use crate::view_models::card::{AgentFacts, CardExtras};

/// "Now" for fixtures, in Unix milliseconds.
pub const NOW: i64 = 1_791_600_000_000;

pub fn ticket(key: &str, title: &str, status: Status) -> Ticket {
    Ticket {
        id: key.into(),
        key: key.parse().expect("fixture key"),
        title: title.into(),
        body: String::new(),
        plan: String::new(),
        plan_by: None,
        plan_at: None,
        priority: Default::default(),
        place: otaq_core::domain::Place::of(status, None, None),
        status,
        base: Base::Branch("main".into()),
        base_locked: false,
        external_id: None,
        depends_on: Vec::new(),
        flags: Flags::default(),
        created_by: Actor::User("me".into()),
        updated_by: None,
        started_at: None,
        created_at: 0,
        updated_at: 0,
        status_since: None,
        queued_at: None,
        ready_since: None,
        validation_round: 0,
        auto_merge_conflicts: 0,
        auto_merge_stop: None,
        work: None,
        start_problem: None,
        interrupted_from: None,
        signal: None,
        question: None,
        pushed_differs: None,
    }
}

/// How the gallery draws a variant. `CardLook` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "CardLook")]
pub enum Look {
    #[default]
    Plain,
    Selected,
    Hovered,
    Dragging,
    Chain,
    Dimmed,
}

pub struct Variant {
    pub name: &'static str,
    pub ticket: Ticket,
    pub extras: CardExtras,
    pub look: Look,
}

fn v(name: &'static str, ticket: Ticket, extras: CardExtras) -> Variant {
    Variant {
        name,
        ticket,
        extras,
        look: Look::Plain,
    }
}

fn base() -> CardExtras {
    CardExtras {
        project_base: Some("main".into()),
        ..CardExtras::default()
    }
}

fn keys(list: &[&str]) -> Vec<otaq_core::domain::TicketKey> {
    list.iter()
        .map(|k| k.parse().expect("fixture key"))
        .collect()
}

/// *04 → Update U + V · Ticket Card v2 · all variants (0086 · V)* in the frame's order, without the variants that
/// need other tools or named agents (M22, M23); then *Interaction states*. Names are the frame's; "(0066)" and
/// "(0083)" drop the "agent picked" footer M21 leaves out.
pub fn variants() -> Vec<Variant> {
    use otaq_core::board::human_check::Arrival;
    use otaq_core::board::tickets::AgentSignal;
    use otaq_core::domain::{AutoMergeStop, ReviewEntry, ReviewEntryFlag, ValidationRound};
    let signal = |text: &str| AgentSignal {
        text: Some(text.into()),
        since: Some(NOW - 60_000),
        wakeup_at: None,
    };
    let live = |t: &mut Ticket, ago_s: i64, tokens: u64| {
        t.flags.last_signal_at = Some(NOW - ago_s * 1000);
        t.flags.tokens = Some(tokens);
    };
    let asks = |question: &str| CardExtras {
        question: Some(question.into()),
        ..base()
    };
    let mut out = Vec::new();

    let mut t = ticket("SHOP-31", "Hotfix: checkout crash", Status::Backlog);
    t.base = Base::Branch("release/2.3".into());
    t.priority = Priority::Urgent;
    out.push(v("Backlog · calm", t, base()));

    let mut t = ticket("SHOP-24", "Update footer links", Status::Backlog);
    t.priority = Priority::Low;
    t.flags.ready_to_start = true;
    out.push(v("Ready to start (0066)", t, base()));

    let mut t = ticket("SHOP-36", "Cache product prices", Status::Backlog);
    t.created_by = Actor::ProjectAgent("pa-1".into());
    out.push(v(
        "Created by Noyan (0068)",
        t,
        CardExtras {
            agent_name: Some("Checkout slowness".into()),
            ..base()
        },
    ));

    let mut t = ticket("SHOP-20", "Checkout page", Status::Queued);
    t.depends_on = keys(&["SHOP-18", "SHOP-19"]);
    out.push(v(
        "Waits for merge (0049)",
        t,
        CardExtras {
            waits_for: Some("waits for SHOP-18, SHOP-19 to merge".into()),
            waits_for_merge: true,
            ..base()
        },
    ));

    out.push(v(
        "Start failed (0018)",
        ticket("SHOP-28", "Cart badge count", Status::StartInterrupted),
        CardExtras {
            start_failure: Some("setup command failed".into()),
            ..base()
        },
    ));

    let mut t = ticket("SHOP-35", "Slow checkout query", Status::WaitingForSlot);
    t.priority = Priority::High;
    out.push(v("Waiting for a free slot (0016)", t, base()));

    let mut working = ticket("SHOP-19", "Price table", Status::Working);
    live(&mut working, 40, 182_000);
    out.push(v("Working", working.clone(), base()));

    // Quiet past the core's 5 minutes (ATT-05).
    let mut t = ticket("BILL-4", "Invoice PDF export", Status::Working);
    live(&mut t, 6 * 60, 64_000);
    out.push(v("Working · stale signal (0027)", t, base()));

    let mut t = working.clone();
    t.status = Status::WorkingInBackground;
    t.flags.last_signal_at = None;
    out.push(v(
        "Working in background (0042)",
        t,
        CardExtras {
            background: Some("test run, 1 helper agent · 6 min".into()),
            ..base()
        },
    ));

    let mut t = working.clone();
    t.flags.run_port = Some(4020);
    t.flags.pinned_at = Some(1);
    out.push(v("Running (Run, 0035) · pinned (0047)", t, base()));

    let mut t = ticket("SHOP-29", "Coupon field", Status::Working);
    t.flags.last_signal_at = Some(NOW - 12_000);
    t.flags.tokens_unknown = true;
    out.push(v("Tokens not reported", t, base()));

    let mut t = ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou);
    live(&mut t, 12, 64_000);
    out.push(v(
        "Needs you · permission (0042)",
        t,
        asks("Allow rm -rf .next?"),
    ));

    let mut t = ticket("SHOP-12", "Add dark mode toggle", Status::AsksYou);
    live(&mut t, 12, 64_000);
    out.push(v(
        "Asks you (0037)",
        t,
        asks("Should dark mode follow the system setting?"),
    ));

    let mut t = ticket("SHOP-19", "Price table", Status::Idle);
    live(&mut t, 12, 64_000);
    out.push(v("Idle (0042)", t, asks("Should I also update the tests?")));

    let mut t = ticket("SHOP-25", "Upgrade to Next.js 15", Status::Interrupted);
    t.flags.tokens = Some(311_000);
    out.push(v("Interrupted (0018)", t, base()));

    let mut t = ticket("SHOP-33", "Shipping estimate", Status::CheckerNeedsYou);
    t.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
    live(&mut t, 60, 64_000);
    t.signal = Some(signal("Allow Bash(pnpm test)?"));
    out.push(v(
        "Needs you · validator (0063)",
        t.clone(),
        base().with_signal(&t, NOW),
    ));

    let mut t = ticket("SHOP-32", "Address autocomplete", Status::CheckFailed);
    t.validation_round = 3;
    t.flags.verdict = Some("Fail: Arrow keys do nothing in the list".into());
    t.flags.tokens = Some(64_000);
    out.push(v("Validation failed 3 times (0063)", t, base()));

    let mut t = ticket("SHOP-34", "Tax display", Status::CheckNoResult);
    t.flags.tokens = Some(64_000);
    out.push(v("Validation ended without a result", t, base()));

    let mut t = ticket("SHOP-20", "Checkout page", Status::ParentClosed);
    t.depends_on = keys(&["SHOP-18"]);
    out.push(v(
        "Parent closed (0033)",
        t,
        CardExtras {
            closed_parent: Some("SHOP-18".parse().expect("key")),
            ..base()
        },
    ));

    let mut t = ticket("SHOP-29", "Coupon field", Status::Checking);
    t.status_since = Some(NOW - 2 * 60_000);
    t.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
    live(&mut t, 3 * 60, 23_000);
    out.push(v("Validating… (0063)", t, base()));

    let mut t = ticket("SHOP-30", "Order history page", Status::Working);
    t.validation_round = 2;
    t.flags.validation_round = Some(ValidationRound { round: 2, of: 3 });
    t.flags.verdict = Some("Fail: the list isn't paginated".into());
    live(&mut t, 20, 141_000);
    out.push(v("Validation failed · round 2 of 3", t, base()));

    let mut t = ticket(
        "SHOP-44",
        "Wishlist share link",
        Status::MergeConflictFixing,
    );
    t.flags.conflict_files = vec!["lib/wishlist.ts".into()];
    live(&mut t, 10, 77_000);
    out.push(v("Merge conflict · fixing (0064)", t, base()));

    let mut t = ticket("SHOP-44", "Wishlist share link", Status::DeliveryStopped);
    t.auto_merge_stop = Some(AutoMergeStop::KeepsConflicting);
    t.auto_merge_conflicts = 3;
    t.flags.tokens = Some(64_000);
    out.push(v("Keeps conflicting (0064)", t, base()));

    // d0120: Local merge runs the project's Merge check before main moves, then a failed one back with its agent.
    let mut t = ticket("SHOP-34", "Tax display", Status::WaitingForYou);
    t.flags.merge_check = Some(otaq_core::domain::MergeCheckState::Running);
    out.push(v("Merging · running the merge check (0120)", t, base()));

    let mut t = ticket("SHOP-34", "Tax display", Status::MergeConflictFixing);
    t.flags.merge_check = Some(otaq_core::domain::MergeCheckState::Failed);
    live(&mut t, 10, 91_000);
    out.push(v("Merge check failed · fixing (0120)", t, base()));

    let mut t = ticket("SHOP-43", "Wishlist page", Status::Queued);
    t.depends_on = keys(&["SHOP-42"]);
    out.push(v(
        "Waits for a dependency",
        t,
        CardExtras {
            waits_for: Some("waits for SHOP-42 to merge".into()),
            ..base()
        },
    ));

    // How each got into Review, as the board reads it from the stage runs (FLW-14).
    let review = |name, entry, round, diff: bool| {
        let mut t = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        t.validation_round = round;
        t.flags.review_entry = Some(ReviewEntryFlag { entry, round });
        let stage = "Validate".to_owned();
        t.flags.arrival = Some(match entry {
            ReviewEntry::Validated => Arrival::Passed {
                stage,
                round: Some(round),
            },
            ReviewEntry::FailedMoved => Arrival::FailedMovedOn {
                stage,
                times: round,
            },
            _ => Arrival::Skipped { stage, by: None },
        });
        v(
            name,
            t,
            CardExtras {
                diff: diff.then_some((412, 38, 9)),
                ..base()
            },
        )
    };
    out.push(review(
        "Validated · round 2",
        ReviewEntry::Validated,
        2,
        true,
    ));
    out.push(review(
        "Validation skipped by you",
        ReviewEntry::Skipped,
        1,
        true,
    ));
    out.push(review(
        "Failed 3 times · moved by you",
        ReviewEntry::FailedMoved,
        3,
        false,
    ));

    let mut t = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
    t.flags.pr_number = Some(481);
    t.flags.tokens = Some(64_000);
    out.push(v(
        "Approved · PR open (0017)",
        t,
        CardExtras {
            pr_provider: Some("GitHub".into()),
            ..base()
        },
    ));

    out.push(v(
        "Merged",
        ticket("SHOP-17", "Add Stripe client", Status::Merged),
        base(),
    ));

    let mut t = ticket("SHOP-41", "Wishlist API", Status::Merged);
    t.validation_round = 1;
    t.flags.review_entry = Some(ReviewEntryFlag {
        entry: ReviewEntry::Validated,
        round: 1,
    });
    t.flags.merged_automatically = true;
    out.push(v("Merged automatically (0064)", t, base()));

    let mut t = ticket("SHOP-26", "Add sitemap", Status::ClosedNotMerged);
    t.flags.pr_number = Some(481);
    out.push(v("Closed · not merged (0033)", t, base()));

    out.push(v("Only one tool connected (0086)", working.clone(), base()));

    let mut t = ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou);
    t.depends_on = keys(&["SHOP-18", "SHOP-19"]);
    t.flags.run_port = Some(4022);
    live(&mut t, 12, 64_000);
    out.push(v(
        "Needs you + running + 2 dependencies",
        t,
        asks("Allow rm -rf .next?"),
    ));

    for (name, look) in [
        ("Hover", Look::Hovered),
        ("Selected (sheet open)", Look::Selected),
        ("Dragging", Look::Dragging),
        ("Chain-highlighted", Look::Chain),
        ("Dimmed", Look::Dimmed),
    ] {
        out.push(Variant {
            look,
            ..v(name, working.clone(), base())
        });
    }
    out
}

/// The refused model's message, as Claude Code words it, straight apostrophe and all (the models report; T-0335's
/// fixture).
pub const MODEL_ISSUE_MESSAGE: &str = "There's an issue with the selected model (claude-fable-5-1). It may not \
                                       exist or you may not have access to it.";

/// shop-web's agents in the M22 cards (update Y, "a list you made"): five of them. Before T-0411 a card names no
/// agent before its start (AGM-09), so `_developer` (the Dev stage's agent) shows nowhere yet.
pub fn named(_developer: &str) -> AgentFacts {
    AgentFacts {
        count: 5,
        ..AgentFacts::default()
    }
}

/// *04 → Variants · one tool (M22)* (`uUqnz`): a refused model, a card before its start naming no agent (AGM-09),
/// then the running footer with two or more agents and with one (Q-A). The gallery's "M22 · Named agents".
pub fn agent_variants() -> Vec<Variant> {
    use otaq_core::board::tickets::{AgentSignal, StartProblem};
    use otaq_core::domain::StartStep;
    let with = |agents: AgentFacts| CardExtras { agents, ..base() };
    let mut out = Vec::new();

    let mut t = ticket("SHOP-48", "Add gift wrap option", Status::StartInterrupted);
    t.start_problem = Some(StartProblem {
        step: StartStep::Model,
        message: MODEL_ISSUE_MESSAGE.into(),
    });
    out.push(v(
        "Start failed on the model",
        t.clone(),
        with(named("Fable test")),
    ));

    let mut t = ticket("SHOP-29", "Coupon field", Status::CheckNoResult);
    t.signal = Some(AgentSignal {
        text: Some(otaq_core::validation::couldnt_run_text(
            "claude-fable-5-1",
            MODEL_ISSUE_MESSAGE,
        )),
        since: Some(NOW - 60_000),
        wakeup_at: None,
    });
    t.started_at = Some(NOW - 3_600_000);
    out.push(v(
        "No result \u{b7} the validator\u{2019}s model",
        t.clone(),
        CardExtras {
            agents: named("Claude Code"),
            ..base()
        }
        .with_signal(&t, NOW),
    ));

    let mut t = ticket("SHOP-24", "Update footer links", Status::Backlog);
    t.flags.ready_to_start = true;
    out.push(v(
        "Project default \u{b7} no footer",
        t,
        with(named("Strong")),
    ));

    let mut working = ticket("SHOP-19", "Price table", Status::Working);
    working.started_at = Some(NOW - 3_600_000);
    working.flags.last_signal_at = Some(NOW - 40_000);
    working.flags.tokens = Some(182_000);
    out.push(v(
        "Working \u{b7} two or more named agents",
        working.clone(),
        with(AgentFacts {
            running_developer: Some("Strong".into()),
            ..named("Strong")
        }),
    ));
    out.push(v(
        "Working \u{b7} one named agent",
        working,
        with(AgentFacts {
            count: 1,
            running_developer: Some("Claude Code".into()),
            ..named("Claude Code")
        }),
    ));
    out
}

/// The cards of checks and deliveries in any flow (FLW-13…FLW-17; `s03-44`, `s03-46`, `s03-48`, `s03-54`): shop-web's
/// Security check (Security reviewer on opus), its Review, its Done · PR delivery by Shipper, and billing-api's
/// click Delivery (drawn on the default flow in `s03-46`; built for other flows only, d0108 keeps the default board).
/// The gallery's "M33 · Checks and deliveries".
/// SHOP-33's Validate with two check agents (d0130; `s29-04` "On the card", `s30-05` "When a check agent doesn't
/// finish"): both running, one held at its plan limit, both failed and back in Dev, one ended without a verdict, a
/// round again after a change; then SHOP-30's one check agent as today.
pub fn check_agent_variants() -> Vec<Variant> {
    use otaq_core::agent_tools::Tool;
    use otaq_core::agents::{HoldReason, SessionRole as Role};
    use otaq_core::domain::{
        CheckAgentFlag, CheckRoundFlag, ValidationOutcome as O, ValidationRound,
    };
    let round = |security: Option<O>, performance: Option<O>| CheckRoundFlag {
        round: 1,
        agents: vec![
            CheckAgentFlag {
                name: "Security reviewer".into(),
                tool: Some(Tool::Claude),
                model: Some("opus".into()),
                outcome: security,
            },
            CheckAgentFlag {
                name: "Performance reviewer".into(),
                tool: Some(Tool::Codex),
                model: Some("gpt-5.5".into()),
                outcome: performance,
            },
        ],
        again: false,
        back_in: None,
    };
    let validating = |status: Status, flag: CheckRoundFlag| {
        let mut t = ticket("SHOP-33", "Shipping estimate", status);
        t.flags.check = Some("Validate".into());
        t.flags.validating_round =
            matches!(status, Status::Checking).then_some(ValidationRound { round: 1, of: 3 });
        t.flags.check_round = Some(flag);
        t
    };
    let named = AgentFacts {
        count: 7,
        running_developer: Some("Developer".into()),
        ..AgentFacts::default()
    };
    let extras = |agents: AgentFacts| CardExtras {
        agents,
        several_tools: true,
        custom_flow: true,
        ..base()
    };
    let mut out = vec![v(
        "Validate · two check agents · running",
        validating(Status::Checking, round(Some(O::Pass), None)),
        extras(named.clone()),
    )];
    let mut held = hold(
        Role::Validator,
        HoldReason::ToolLimit,
        Tool::Codex,
        Some("gpt-5.5"),
        "Performance reviewer",
        Some(Tool::Claude),
        true,
        false,
    );
    held.check_agent = Some("Performance reviewer".into());
    out.push(v(
        "Validate · one held at its plan limit",
        validating(Status::Checking, round(Some(O::Pass), None)),
        extras(AgentFacts {
            holds: vec![held],
            ..named.clone()
        }),
    ));
    let mut back = ticket("SHOP-33", "Shipping estimate", Status::Working);
    back.flags.check = Some("Validate".into());
    back.flags.validation_round = Some(ValidationRound { round: 1, of: 3 });
    back.flags.check_round = Some(CheckRoundFlag {
        back_in: Some("Dev".into()),
        ..round(Some(O::Fail), Some(O::Fail))
    });
    out.push(v(
        "Validate · both gave a verdict · back in Dev",
        back,
        extras(named.clone()),
    ));
    out.push(v(
        "Validate · one ended without a verdict",
        validating(
            Status::CheckNoResult,
            round(Some(O::Pass), Some(O::NoResult)),
        ),
        extras(named.clone()),
    ));
    out.push(v(
        "Validate · round 1 again · the code changed",
        validating(
            Status::Checking,
            CheckRoundFlag {
                again: true,
                ..round(None, None)
            },
        ),
        extras(named.clone()),
    ));
    let mut one = ticket("SHOP-30", "Order history page", Status::Checking);
    one.flags.check = Some("Validate".into());
    one.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
    out.push(v(
        "Validate · one check agent · as today",
        one,
        extras(named),
    ));
    out
}

/// *30 → The card in Dev* (`s30-02`, `s30-03`, `s30-04`; d0133, T-0541): one card in its build stage with a row per
/// piece, the fold past four, all merged, back from Validate, after a restart, a check of the whole ticket, and a
/// ticket working alone as today.
pub fn piece_variants() -> Vec<Variant> {
    use crate::view_models::pieces::fixtures::{helper, mid_way, waiting};
    use crate::view_models::plan::fixtures::WISHLIST;
    use otaq_core::domain::ValidationRound;
    use otaq_core::helpers::{Helper, HelperStatus as H};
    let shop40 = |status: Status| {
        let mut t = ticket("SHOP-40", "Wishlist", status);
        t.plan = WISHLIST.to_owned();
        t.flags.last_signal_at = Some(NOW - 180_000);
        t
    };
    let extras = |helpers: Vec<Helper>| CardExtras {
        helpers,
        next_stage: Some("Validate".into()),
        agents: AgentFacts {
            count: 4,
            running_developer: Some("Developer".into()),
            ..AgentFacts::default()
        },
        ..base()
    };
    let merged = |n: u32| -> Vec<Helper> {
        [
            "Wishlist API",
            "Wishlist button",
            "Wishlist page",
            "Share link",
        ]
        .iter()
        .zip(1..=n)
        .map(|(title, piece)| helper("SHOP-40", piece, title, H::Merged))
        .collect()
    };
    let mut early = merged(0);
    early.push(helper("SHOP-40", 1, "Wishlist API", H::Working));
    let mut reviewed = mid_way();
    reviewed[1].status = H::BeingReviewed;
    reviewed[2].status = H::SentBack;
    reviewed[2].note = Some("No empty state.".into());
    let mut seventy = ticket("SHOP-70", "Checkout redesign", Status::Working);
    seventy.plan =
        "## Pieces\n1. Cart\n2. Sign-in step\n3. Shipping step\n4. Address step\n5. Payment step\n\
                    6. Order summary \u{2014} waits for 4, 5\n"
            .to_owned();
    let seventy_helpers = vec![
        helper("SHOP-70", 1, "Cart", H::Merged),
        helper("SHOP-70", 2, "Sign-in step", H::Merged),
        helper("SHOP-70", 3, "Shipping step", H::Merged),
        helper("SHOP-70", 4, "Address step", H::Working),
        helper("SHOP-70", 5, "Payment step", H::Working),
    ];
    let mut back = shop40(Status::Working);
    back.flags.check = Some("Validate".into());
    back.flags.validation_round = Some(ValidationRound { round: 1, of: 3 });
    let mut fix = merged(4);
    fix.push(helper("SHOP-40", 5, "Share link signed out", H::Working));
    let mut checking = shop40(Status::Checking);
    checking.flags.check = Some("Validate".into());
    checking.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
    let mut asked = mid_way();
    asked[1].asked = Some(otaq_core::helpers::questions::HelperQuestionState::WithInCharge);
    let waits = |asks: bool, status: Status| {
        let (wait, helpers) = waiting(asks);
        let mut t = shop40(status);
        t.flags.piece_wait = Some(wait);
        (t, helpers)
    };
    let (passed_on, passed_on_helpers) = waits(true, Status::AsksYou);
    let (prompt, prompt_helpers) = waits(false, Status::NeedsYou);
    let mut interrupted = shop40(Status::Interrupted);
    interrupted.flags.helpers_to_resume = vec!["40.2".into(), "40.3".into()];
    vec![
        v(
            "Dev · early: one helper working",
            shop40(Status::Working),
            extras(early),
        ),
        v(
            "Dev · mid-way: two side by side",
            shop40(Status::Working),
            extras(mid_way()),
        ),
        v(
            "Dev · one reviewed, one sent back",
            shop40(Status::Working),
            extras(reviewed),
        ),
        v(
            "Dev · more than four pieces",
            seventy,
            extras(seventy_helpers),
        ),
        v(
            "Dev · all merged, about to move",
            shop40(Status::Working),
            extras(merged(4)),
        ),
        v(
            "Dev · back from Validate: a fix is a new piece",
            back,
            extras(fix),
        ),
        v(
            "Dev · a helper asked the agent in charge",
            shop40(Status::Working),
            extras(asked),
        ),
        v(
            "Dev · passed on: a helper asks you",
            passed_on,
            extras(passed_on_helpers),
        ),
        v(
            "Dev · a helper's permission prompt",
            prompt,
            extras(prompt_helpers),
        ),
        v(
            "Dev · interrupted with helpers",
            interrupted,
            extras(mid_way()),
        ),
        v(
            "Validate · checking the whole ticket",
            checking,
            extras(merged(4)),
        ),
        v(
            "Dev · working alone: as today",
            ticket("SHOP-33", "Shipping estimate", Status::Working),
            extras(Vec::new()),
        ),
    ]
}

pub fn flow_variants() -> Vec<Variant> {
    use otaq_core::agent_tools::Tool;
    use otaq_core::agents::facts::RunsOnTool;
    use otaq_core::agents::{HoldReason, SessionRole as Role};
    use otaq_core::board::human_check::Arrival;
    use otaq_core::domain::{AutoMergeStop, ValidationRound};
    use otaq_core::flow::model::Delivers;
    let agents = |validator: Option<(&str, &str)>, developer: Option<(&str, &str)>| AgentFacts {
        count: 7,
        running_validator: validator.map(|(name, _)| name.to_owned()),
        validator_on: validator.map(|(_, model)| RunsOnTool {
            tool: Tool::Claude,
            model: None,
            passed: Some(model.to_owned()),
            fallback: None,
        }),
        running_developer: developer.map(|(name, _)| name.to_owned()),
        developer_on: developer.map(|(_, model)| RunsOnTool {
            tool: Tool::Claude,
            model: None,
            passed: Some(model.to_owned()),
            fallback: None,
        }),
        ..AgentFacts::default()
    };
    let shop = |delivers| CardExtras {
        delivers: Some(delivers),
        delivery_agent: Some("Shipper".into()),
        agents: AgentFacts {
            count: 7,
            ..AgentFacts::default()
        },
        ..base()
    };
    let security = |t: &mut Ticket| t.flags.check = Some("Security".into());
    let mut out = Vec::new();

    // `s03-44`: an agent check in any flow.
    let mut t = ticket("SHOP-19", "Price table", Status::Checking);
    security(&mut t);
    t.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
    t.flags.tokens = Some(18_000);
    out.push(v(
        "Checking \u{b7} Security",
        t,
        CardExtras {
            agents: agents(Some(("Security reviewer", "opus")), None),
            ..base()
        },
    ));

    let mut t = ticket("SHOP-29", "Coupon field", Status::Working);
    security(&mut t);
    t.validation_round = 2;
    t.flags.validation_round = Some(ValidationRound { round: 2, of: 3 });
    t.flags.last_signal_at = Some(NOW - 20_000);
    t.flags.tokens = Some(141_000);
    out.push(v(
        "Security failed \u{b7} back in Dev",
        t,
        CardExtras {
            agents: agents(None, Some(("Rust developer", "sonnet"))),
            ..base()
        },
    ));

    let mut t = ticket("SHOP-32", "Address autocomplete", Status::CheckFailed);
    security(&mut t);
    t.validation_round = 3;
    t.flags.verdict = Some("Fail: unescaped SQL in search".into());
    out.push(v("Security failed \u{d7}3", t, shop(Delivers::Pr)));

    // `s03-46`, `s03-54`: your check.
    let check = |key: &str, title: &str, arrival| {
        let mut t = ticket(key, title, Status::WaitingForYou);
        t.flags.arrival = Some(arrival);
        t
    };
    out.push(v(
        "Your check \u{b7} passed Security",
        check(
            "SHOP-18",
            "Checkout API",
            Arrival::Passed {
                stage: "Security".into(),
                round: Some(2),
            },
        ),
        shop(Delivers::Pr),
    ));
    out.push(v(
        "Your check \u{b7} skipped Security",
        check(
            "SHOP-23",
            "Split user full name",
            Arrival::Skipped {
                stage: "Security".into(),
                by: None,
            },
        ),
        shop(Delivers::Pr),
    ));
    out.push(v(
        "Your check \u{b7} skipped by Noyan",
        check(
            "SHOP-27",
            "Fix README typo",
            Arrival::Skipped {
                stage: "Product spec".into(),
                by: Some("Noyan".into()),
            },
        ),
        shop(Delivers::Pr),
    ));
    out.push(v(
        "Your check \u{b7} moved back",
        check("SHOP-19", "Price table", Arrival::MovedBack),
        shop(Delivers::Pr),
    ));

    // `s03-46`: deliveries waiting for a click, and an open PR.
    let mut t = ticket("BILL-26", "Invoice PDF export", Status::ToDeliver);
    t.flags.arrival = Some(Arrival::Passed {
        stage: "Review".into(),
        round: None,
    });
    out.push(v(
        "Ready to merge \u{b7} passed Review",
        t,
        CardExtras {
            delivers: Some(Delivers::Either),
            ..base()
        },
    ));
    let mut t = ticket("BILL-30", "Refund webhook", Status::ToDeliver);
    t.flags.moved_past_check = Some("Failed Validate 3 times \u{b7} moved by you".into());
    out.push(v(
        "Ready to merge \u{b7} moved past Validate",
        t,
        CardExtras {
            delivers: Some(Delivers::Either),
            ..base()
        },
    ));
    let mut t = ticket("SHOP-45", "Gift cards", Status::ToDeliver);
    t.flags.moved_past_check = Some("Failed Security 3 times \u{b7} moved by you".into());
    out.push(v(
        "Ready for PR \u{b7} moved past Security",
        t,
        shop(Delivers::Pr),
    ));
    let mut t = ticket("SHOP-23", "Split user full name", Status::ToDeliver);
    t.flags.moved_past_check = Some("Skipped Security \u{b7} by you".into());
    out.push(v(
        "Ready for PR \u{b7} skipped Security",
        t,
        shop(Delivers::Pr),
    ));
    let mut t = ticket("SHOP-41", "Saved addresses", Status::ToDeliver);
    t.flags.delivering_automatically = true;
    out.push(v(
        "Ready for PR \u{b7} delivering automatically",
        t,
        shop(Delivers::Pr),
    ));
    let mut t = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
    t.flags.pr_number = Some(481);
    t.status_since = Some(NOW - 2 * 3_600_000);
    out.push(v(
        "PR #481 open \u{b7} opened by Shipper",
        t,
        shop(Delivers::Pr),
    ));

    // `s03-46`: conflicts at the Delivery.
    let mut t = ticket("SHOP-34", "Tax display", Status::MergeConflictFixing);
    t.flags.last_signal_at = Some(NOW - 10_000);
    t.flags.tokens = Some(77_000);
    out.push(v(
        "Merge conflict \u{b7} fixing in Dev",
        t,
        CardExtras {
            agents: agents(None, Some(("Rust developer", "sonnet"))),
            ..base()
        },
    ));
    let stopped = |key: &str, title: &str| {
        let mut t = ticket(key, title, Status::DeliveryStopped);
        t.auto_merge_stop = Some(AutoMergeStop::KeepsConflicting);
        t.auto_merge_conflicts = 3;
        t
    };
    out.push(v(
        "Conflicts \u{d7}3 \u{b7} main",
        stopped("SHOP-38", "Cart totals rounding"),
        base(),
    ));

    // `s03-48`: Done.
    let mut t = ticket("DOCS-7", "Fix broken anchors", Status::Merged);
    t.flags.merged_automatically = true;
    t.flags.delivered_without_checks = true;
    out.push(v("Delivered without checks", t, base()));

    // `s03-54`: a check held for the plan limit, with and without a fallback that can run.
    let limit = |fallback: bool| {
        let mut h = hold(
            Role::Validator,
            HoldReason::ToolLimit,
            Tool::Claude,
            Some("opus"),
            "Security reviewer",
            fallback.then_some(Tool::Codex),
            fallback,
            false,
        );
        h.can_skip_check = !fallback;
        h
    };
    for (name, fallback) in [
        ("Plan limit \u{b7} a fallback is set", true),
        ("Plan limit \u{b7} no fallback can run", false),
    ] {
        let mut t = ticket("SHOP-19", "Price table", Status::Checking);
        security(&mut t);
        out.push(v(
            name,
            t,
            CardExtras {
                agents: AgentFacts {
                    count: 7,
                    holds: vec![limit(fallback)],
                    ..AgentFacts::default()
                },
                ..base()
            },
        ));
    }
    // Each in a flow other than the default (`keeps_default_look`), as the frames draw them.
    for v in &mut out {
        v.extras.custom_flow = true;
    }
    out
}

/// A role held before it starts (T-0357's `agent_holds`, as `board.agents` names it).
#[allow(clippy::too_many_arguments)]
pub fn hold(
    role: otaq_core::agents::SessionRole,
    reason: otaq_core::agents::HoldReason,
    tool: otaq_core::agent_tools::Tool,
    model: Option<&str>,
    agent: &str,
    fallback: Option<otaq_core::agent_tools::Tool>,
    can_use_fallback: bool,
    fallback_held: bool,
) -> otaq_core::agents::facts::CardHold {
    otaq_core::agents::facts::CardHold {
        role,
        reason,
        tool,
        model: model.map(str::to_owned),
        agent: Some(agent.to_owned()),
        check_agent: None,
        agent_id: None,
        fallback: fallback.map(|tool| otaq_core::agents::ModelRef { tool, model: None }),
        can_use_fallback,
        fallback_held,
        can_skip_check: false,
        resets_at: None,
    }
}

/// A session running on `tool`, its tool `from`'s fallback, for `reason` (AGM-03).
pub fn on_fallback(
    tool: otaq_core::agent_tools::Tool,
    from: otaq_core::agent_tools::Tool,
    reason: otaq_core::agents::HoldReason,
) -> otaq_core::agents::facts::RunsOnTool {
    otaq_core::agents::facts::RunsOnTool {
        tool,
        model: None,
        passed: None,
        fallback: Some(otaq_core::agents::FallbackFrom { tool: from, reason }),
    }
}

/// *04 → Variants · several tools (M23)* (`gef2j`) and the fallback and limit cards of *Ticket Card v2 · all variants*
/// (`SPy9r`): two or more tools connected, so the head shows whoever has the turn's avatar. shop-web's list here:
/// Strong, Light, Sonnet, Fable test on Claude Code; Standard, Strong GPT, Old GPT on Codex; Verifier on Copilot
/// (update Y's sample list). The gallery's "M23 · Several tools".
pub fn tool_variants() -> Vec<Variant> {
    use crate::view_models::tool_avatar::{AvatarTool, ToolAvatarView};
    use otaq_core::agent_tools::Tool;
    use otaq_core::agents::{HoldReason as H, SessionRole as Role};
    use otaq_core::board::tickets::StartProblem;
    use otaq_core::domain::{StartStep, ValidationRound};
    let av = |tool: AvatarTool, model: Option<&str>| Some(ToolAvatarView::of(tool, model));
    let tools = |developer: Option<ToolAvatarView>, agents: AgentFacts| CardExtras {
        several_tools: true,
        developer,
        validator: av(AvatarTool::ClaudeCode, None),
        agents: AgentFacts { count: 8, ..agents },
        ..base()
    };
    // A held Developer is Held with its step (T-0357); the hold's row says the rest.
    let held = |key: &str, title: &str, step| {
        let mut t = ticket(key, title, Status::Held);
        t.start_problem = Some(StartProblem {
            step,
            message: "held before launch".into(),
        });
        t
    };
    let mut out = Vec::new();

    let mut t = ticket("SHOP-48", "Add gift wrap option", Status::StartInterrupted);
    t.priority = Priority::High;
    t.start_problem = Some(StartProblem {
        step: StartStep::Model,
        message: MODEL_ISSUE_MESSAGE.into(),
    });
    out.push(v(
        "Start failed \u{b7} several tools",
        t,
        tools(
            av(AvatarTool::ClaudeCode, Some("claude-fable-5-1")),
            named("Fable test"),
        ),
    ));

    let mut t = ticket("SHOP-18", "Checkout API", Status::StartInterrupted);
    t.start_problem = Some(StartProblem {
        step: StartStep::Model,
        message: "model gpt-5.4 is not supported on this account".into(),
    });
    out.push(v(
        "Start failed \u{b7} Codex",
        t,
        tools(av(AvatarTool::Codex, Some("gpt-5.4")), named("Standard")),
    ));

    let mut t = held(
        "SHOP-49",
        "Export orders as CSV",
        StartStep::ModelUnavailable,
    );
    t.priority = Priority::High;
    out.push(v(
        "Model unavailable",
        t,
        tools(
            av(AvatarTool::Codex, Some("gpt-5.4")),
            AgentFacts {
                holds: vec![hold(
                    Role::Developer,
                    H::ModelUnavailable,
                    Tool::Codex,
                    Some("gpt-5.4"),
                    "Old GPT",
                    None,
                    false,
                    false,
                )],
                ..named("Old GPT")
            },
        ),
    ));

    let mut t = ticket("SHOP-30", "Order history page", Status::Checking);
    t.priority = Priority::High;
    t.started_at = Some(NOW - 3_600_000);
    out.push(v(
        "Model unavailable \u{b7} the validator",
        t,
        CardExtras {
            validator: av(AvatarTool::Codex, Some("gpt-5.4")),
            ..tools(
                av(AvatarTool::ClaudeCode, None),
                AgentFacts {
                    holds: vec![hold(
                        Role::Validator,
                        H::ModelUnavailable,
                        Tool::Codex,
                        Some("gpt-5.4"),
                        "Old GPT",
                        None,
                        false,
                        false,
                    )],
                    ..named("Strong")
                },
            )
        },
    ));

    let mut swapped = ticket("SHOP-29", "Coupon field", Status::Checking);
    swapped.priority = Priority::High;
    swapped.started_at = Some(NOW - 3_600_000);
    swapped.status_since = Some(NOW - 2 * 60_000);
    swapped.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
    swapped.flags.tokens = Some(23_000);
    out.push(v(
        "Swapped model \u{b7} not on the card",
        swapped,
        CardExtras {
            validator: av(AvatarTool::Copilot, Some("auto")),
            ..tools(
                av(AvatarTool::ClaudeCode, None),
                AgentFacts {
                    running_validator: Some("Verifier".into()),
                    ..named("Strong")
                },
            )
        },
    ));

    let mut t = held("SHOP-24", "Update footer links", StartStep::ToolLimit);
    t.priority = Priority::Low;
    out.push(v(
        "Tool at its limit, no fallback (0086)",
        t,
        tools(
            av(AvatarTool::ClaudeCode, None),
            AgentFacts {
                holds: vec![hold(
                    Role::Developer,
                    H::ToolLimit,
                    Tool::Claude,
                    None,
                    "Light",
                    None,
                    false,
                    false,
                )],
                ..named("Light")
            },
        ),
    ));

    let mut t = held("SHOP-31", "Hotfix: checkout crash", StartStep::ToolLimit);
    t.priority = Priority::Urgent;
    out.push(v(
        "Tool at its limit \u{b7} fallback off",
        t,
        tools(
            av(AvatarTool::ClaudeCode, Some("claude-opus-5-5")),
            AgentFacts {
                holds: vec![hold(
                    Role::Developer,
                    H::ToolLimit,
                    Tool::Claude,
                    Some("claude-opus-5-5"),
                    "Strong",
                    Some(Tool::Codex),
                    true,
                    false,
                )],
                ..named("Strong")
            },
        ),
    ));

    let t = held(
        "SHOP-36",
        "Cache product prices",
        StartStep::ToolUnavailable,
    );
    out.push(v(
        "Tool not available \u{b7} fallback off (0086)",
        t,
        tools(
            av(AvatarTool::Codex, None),
            AgentFacts {
                holds: vec![hold(
                    Role::Developer,
                    H::ToolUnavailable,
                    Tool::Codex,
                    None,
                    "Standard",
                    Some(Tool::Claude),
                    true,
                    false,
                )],
                ..named("Standard")
            },
        ),
    ));

    let mut t = ticket("SHOP-33", "Shipping estimate", Status::Checking);
    t.started_at = Some(NOW - 3_600_000);
    out.push(v(
        "Validator held \u{b7} tool not available",
        t,
        CardExtras {
            validator: av(AvatarTool::Copilot, None),
            ..tools(
                av(AvatarTool::ClaudeCode, None),
                AgentFacts {
                    holds: vec![hold(
                        Role::Validator,
                        H::ToolUnavailable,
                        Tool::Copilot,
                        None,
                        "Verifier",
                        Some(Tool::Claude),
                        true,
                        false,
                    )],
                    ..named("Standard")
                },
            )
        },
    ));

    let mut t = ticket("SHOP-31", "Hotfix: checkout crash", Status::Working);
    t.priority = Priority::Urgent;
    t.started_at = Some(NOW - 600_000);
    t.flags.last_signal_at = Some(NOW - 30_000);
    t.flags.tokens = Some(80_000);
    out.push(v(
        "On its tool\u{2019}s fallback (AGM-03)",
        t,
        tools(
            av(AvatarTool::Codex, Some("gpt-5.5")),
            AgentFacts {
                running_developer: Some("Strong GPT".into()),
                developer_on: Some(on_fallback(Tool::Codex, Tool::Claude, H::ToolLimit)),
                ..named("Strong GPT")
            },
        ),
    ));

    let mut t = held("SHOP-31", "Hotfix: checkout crash", StartStep::ToolLimit);
    t.priority = Priority::Urgent;
    out.push(v(
        "Fallback also at its limit",
        t,
        tools(
            av(AvatarTool::ClaudeCode, Some("claude-opus-5-5")),
            AgentFacts {
                holds: vec![hold(
                    Role::Developer,
                    H::ToolLimit,
                    Tool::Claude,
                    Some("claude-opus-5-5"),
                    "Strong",
                    Some(Tool::Codex),
                    false,
                    true,
                )],
                ..named("Strong")
            },
        ),
    ));

    let mut t = ticket("SHOP-29", "Coupon field", Status::Checking);
    t.started_at = Some(NOW - 3_600_000);
    t.flags.tokens = Some(6_000);
    out.push(v(
        "Validator on its tool\u{2019}s fallback",
        t,
        CardExtras {
            validator: av(AvatarTool::ClaudeCode, Some("claude-opus-5-5")),
            ..tools(
                av(AvatarTool::Copilot, None),
                AgentFacts {
                    running_validator: Some("Strong".into()),
                    validator_on: Some(on_fallback(
                        Tool::Claude,
                        Tool::Copilot,
                        H::ToolUnavailable,
                    )),
                    ..named("Standard")
                },
            )
        },
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card::CardViewModel;

    #[test]
    fn every_fixture_builds_a_card() {
        let all = variants();
        assert!(all.len() >= 30);
        for variant in &all {
            let vm = CardViewModel::from(&variant.ticket, &variant.extras, NOW);
            assert!(!vm.title.is_empty(), "{}", variant.name);
        }
    }
}
