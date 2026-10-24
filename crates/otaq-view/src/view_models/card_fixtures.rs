//! Sample cards from the `shop-web` project (design-briefs/00-README.md), one per variant of *04 · Ticket card
//! variants → Ticket Card v2 · all variants* (the M21 ones). The gallery draws them; tests build on them.

use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Actor, Flags, Kind, Priority, Status};
use otaq_core::domain::{AskedLead, ByLead, LeadNoteSent, SameFile, StuckStop, UnreadNotes};

use crate::view_models::card::{AgentFacts, CardExtras, Sleeve};

/// "Now" for fixtures, in Unix milliseconds.
pub const NOW: i64 = 1_791_600_000_000;

pub fn ticket(key: &str, title: &str, status: Status) -> Ticket {
    Ticket {
        id: key.into(),
        key: key.parse().expect("fixture key"),
        kind: Kind::Ticket,
        title: title.into(),
        body: String::new(),
        priority: Default::default(),
        priority_inherited: false,
        epic: None,
        place: otaq_core::domain::Place::of(status, None, None),
        status,
        base: Base::Branch("main".into()),
        base_locked: false,
        external_id: None,
        depends_on: Vec::new(),
        aliases: Vec::new(),
        next_child_number: None,
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
        epic_queue_mode: Default::default(),
        joined_queue_by: None,
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

/// The sleeve of a SHOP-40 Wishlist child (*04 → Row · epic children & Done (0080)*).
pub fn wishlist(pinned: bool) -> Option<Sleeve> {
    Some(Sleeve {
        epic_key: "SHOP-40".into(),
        name: "Wishlist".into(),
        pinned,
    })
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
    let child = |key: &str, title: &str, status| {
        let mut t = ticket(key, title, status);
        t.epic = Some("SHOP-40".parse().expect("key"));
        t
    };
    let sleeved = || CardExtras {
        sleeve: wishlist(false),
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
        "Created by a Project agent (0068)",
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

    let mut t = child(
        "SHOP-40.4",
        "Wishlist share link",
        Status::MergeConflictFixing,
    );
    t.flags.conflicts_with = Some(otaq_core::naming::branch("SHOP-40"));
    t.flags.conflict_files = vec!["lib/wishlist.ts".into()];
    live(&mut t, 10, 77_000);
    out.push(v("Merge conflict · fixing (0064)", t, sleeved()));

    let mut t = child("SHOP-40.4", "Wishlist share link", Status::DeliveryStopped);
    t.auto_merge_stop = Some(AutoMergeStop::KeepsConflicting);
    t.auto_merge_conflicts = 3;
    t.flags.conflicts_with = Some(otaq_core::naming::branch("SHOP-40"));
    t.flags.tokens = Some(64_000);
    out.push(v("Keeps conflicting (0064)", t, sleeved()));

    // d0120: Local merge runs the project's Merge check before main moves, then a failed one back with its agent.
    let mut t = ticket("SHOP-34", "Tax display", Status::WaitingForYou);
    t.flags.merge_check = Some(otaq_core::domain::MergeCheckState::Running);
    out.push(v("Merging · running the merge check (0120)", t, base()));

    let mut t = ticket("SHOP-34", "Tax display", Status::MergeConflictFixing);
    t.flags.merge_check = Some(otaq_core::domain::MergeCheckState::Failed);
    live(&mut t, 10, 91_000);
    out.push(v("Merge check failed · fixing (0120)", t, base()));

    let mut t = child("SHOP-40.3", "Wishlist page", Status::Queued);
    t.depends_on = keys(&["SHOP-40.2"]);
    out.push(v(
        "Picked by the lead (0083)",
        t,
        CardExtras {
            waits_for: Some("waits for SHOP-40.2 to merge".into()),
            ..sleeved()
        },
    ));

    let mut t = child("SHOP-40.2", "Wishlist button", Status::Working);
    live(&mut t, 12, 58_000);
    t.flags.unread_notes = Some(UnreadNotes {
        count: 1,
        from: WISHLIST_LEAD.into(),
        latest_text: "40.1 returns items newest first. Use the same order on the shared list."
            .into(),
        latest_at: NOW - 2 * 60_000,
    });
    t.flags.same_file = vec![SameFile {
        other: "SHOP-40.4".parse().expect("key"),
        files: vec!["lib/wishlist.ts".into()],
    }];
    out.push(v("Unread note · same file (0083 · T)", t, sleeved()));

    let mut t = child("SHOP-40.5", "Wishlist empty state", Status::AsksYou);
    live(&mut t, 12, 64_000);
    t.flags.via_lead = Some(WISHLIST_LEAD.into());
    out.push(v(
        "Asks you · via Wishlist lead (0083)",
        t,
        CardExtras {
            question: Some("Show the empty state on mobile too?".into()),
            ..sleeved()
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

    let mut t = child("SHOP-40.1", "Wishlist API", Status::Merged);
    t.validation_round = 1;
    t.flags.review_entry = Some(ReviewEntryFlag {
        entry: ReviewEntry::Validated,
        round: 1,
    });
    t.flags.merged_automatically = true;
    out.push(v("Merged automatically (0064)", t, sleeved()));

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
        ("Selected (panel open)", Look::Selected),
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

/// *04 → Row · epic children & Done (0080)*: sleeved children of SHOP-40 Wishlist (EPC-09, EPC-13, EPC-17).
pub fn sleeve_variants() -> Vec<Variant> {
    let child = |key: &str, title: &str, status| {
        let mut t = ticket(key, title, status);
        t.epic = Some("SHOP-40".parse().expect("key"));
        t
    };
    let mut out = Vec::new();
    let mut t = child("SHOP-40.2", "Wishlist button", Status::Working);
    t.depends_on = keys(&["SHOP-40.1"]);
    t.flags.last_signal_at = Some(NOW - 60_000);
    t.flags.tokens = Some(96_000);
    out.push(v(
        "Dev · epic child · working",
        t,
        CardExtras {
            sleeve: wishlist(false),
            ..base()
        },
    ));
    let mut t = child("SHOP-40.2", "Wishlist button", Status::AsksYou);
    t.flags.last_signal_at = Some(NOW - 2 * 60_000);
    t.flags.tokens = Some(98_000);
    out.push(v(
        "Dev · epic child · asks you",
        t,
        CardExtras {
            sleeve: wishlist(false),
            question: Some("Show the heart on product cards too?".into()),
            ..base()
        },
    ));
    // The epic is pinned: the pin is in the sleeve, not on the child (EPC-17).
    let mut t = child("SHOP-40.3", "Wishlist page", Status::Queued);
    t.depends_on = keys(&["SHOP-40.2"]);
    out.push(v(
        "Pinned epic · child (no own pin)",
        t,
        CardExtras {
            sleeve: wishlist(true),
            waits_for: Some("waits for SHOP-40.2 to merge".into()),
            waits_for_merge: true,
            ..base()
        },
    ));
    // *03 → epic flows 3 → 10*: SHOP-40.2's agent added it while the whole epic was queued (EPC-13).
    let mut t = child("SHOP-40.4", "Wishlist share link", Status::Queued);
    t.depends_on = keys(&["SHOP-40.2"]);
    t.created_by = Actor::Agent("SHOP-40.2".parse().expect("key"));
    t.joined_queue_by = Some(Actor::Agent("SHOP-40.2".parse().expect("key")));
    out.push(v(
        "Ready for Dev · epic child · new",
        t,
        CardExtras {
            sleeve: wishlist(false),
            ..base()
        },
    ));
    out
}

/// The lead's name in the fixtures.
pub const WISHLIST_LEAD: &str = "Wishlist lead";

/// The lead's note in the fixtures (*03 → 24, 25*).
pub fn lead_note() -> LeadNoteSent {
    LeadNoteSent {
        lead: WISHLIST_LEAD.into(),
        text: "The API returns items[] (SHOP-40.1). Fix the test, don't change the API.".into(),
        view: Some("The agent keeps changing the API instead of the test.".into()),
        stop: StuckStop::ValidationFailed,
        after_rounds: Some(3),
        at: NOW - 30 * 60_000,
    }
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
        "Your check \u{b7} skipped by the Project agent",
        check(
            "SHOP-27",
            "Fix README typo",
            Arrival::Skipped {
                stage: "Grill".into(),
                by: Some("Planner".into()),
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
    let stopped = |key: &str, title: &str, epic: bool| {
        let mut t = ticket(key, title, Status::DeliveryStopped);
        t.auto_merge_stop = Some(AutoMergeStop::KeepsConflicting);
        t.auto_merge_conflicts = 3;
        if epic {
            t.epic = Some("SHOP-40".parse().expect("key"));
        }
        t
    };
    out.push(v(
        "Conflicts \u{d7}3 \u{b7} the epic\u{2019}s branch",
        stopped("SHOP-40.4", "Wishlist share link", true),
        CardExtras {
            sleeve: wishlist(false),
            ..base()
        },
    ));
    out.push(v(
        "Conflicts \u{d7}3 \u{b7} main",
        stopped("SHOP-38", "Cart totals rounding", false),
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

/// *04 → Row · Epic children & the lead (0083)* and *Row · Unread notes & same file (0083 · T)*, in the frames'
/// order, then *03 → 25*'s second stop (LEAD-06…LEAD-10). SHOP-40 Wishlist has five children here.
pub fn lead_variants() -> Vec<Variant> {
    let child = |key: &str, title: &str, status| {
        let mut t = ticket(key, title, status);
        t.epic = Some("SHOP-40".parse().expect("key"));
        t
    };
    let sleeve = || CardExtras {
        sleeve: Some(Sleeve {
            epic_key: "SHOP-40".into(),
            name: "Wishlist".into(),
            pinned: false,
        }),
        ..base()
    };
    let live = |t: &mut Ticket, ago_s: i64, tokens: u64| {
        t.flags.last_signal_at = Some(NOW - ago_s * 1000);
        t.flags.tokens = Some(tokens);
    };
    let same = |other: &str| {
        vec![SameFile {
            other: other.parse().expect("key"),
            files: vec!["lib/wishlist.ts".into()],
        }]
    };
    let unread = || {
        UnreadNotes {
        count: 1,
        from: WISHLIST_LEAD.into(),
        latest_text: "40.1 returns items newest first (recorded decision on SHOP-40.1). Use the same order on the \
                      shared list."
            .into(),
        latest_at: NOW - 2 * 60_000,
    }
    };
    let mut out = Vec::new();

    let mut t = child("SHOP-40.2", "Wishlist button", Status::Working);
    t.depends_on = keys(&["SHOP-40.1"]);
    live(&mut t, 60, 104_000);
    t.flags.asked_lead = Some(AskedLead {
        lead: WISHLIST_LEAD.into(),
        question: "Should the button show a filled heart once the item is saved?".into(),
    });
    out.push(v("Dev · asked lead · waiting", t, sleeve()));

    let mut t = child("SHOP-40.2", "Wishlist button", Status::AsksYou);
    live(&mut t, 3 * 60, 106_000);
    t.flags.via_lead = Some(WISHLIST_LEAD.into());
    out.push(v(
        "Dev · Asks you · via Wishlist lead",
        t,
        CardExtras {
            question: Some("Can guests save items without logging in?".into()),
            ..sleeve()
        },
    ));

    let mut t = child("SHOP-40.3", "Wishlist page", Status::Queued);
    t.depends_on = keys(&["SHOP-40.4"]);
    t.flags.by_lead = Some(ByLead {
        lead: WISHLIST_LEAD.into(),
        added: false,
        updated: true,
        fields: vec!["description".into()],
        waits_for: keys(&["SHOP-40.4"]),
    });
    out.push(v("Ready for Dev · Updated by Wishlist lead", t, sleeve()));

    let mut t = child("SHOP-40.5", "Wishlist empty state", Status::Queued);
    t.created_by = Actor::ProjectAgent("lead-1".into());
    t.flags.by_lead = Some(ByLead {
        lead: WISHLIST_LEAD.into(),
        added: true,
        ..ByLead::default()
    });
    out.push(v(
        "Ready for Dev · new · added by Wishlist lead",
        t,
        sleeve(),
    ));

    let mut t = child("SHOP-40.3", "Wishlist page", Status::Working);
    live(&mut t, 20, 141_000);
    t.flags.lead_note_sent = Some(lead_note());
    out.push(v("Dev · Lead's note sent (round 0 of 3)", t, sleeve()));

    let mut t = child("SHOP-40.4", "Wishlist share link", Status::Working);
    live(&mut t, 40, 71_000);
    t.flags.unread_notes = Some(unread());
    out.push(v("Dev · 1 unread note from Wishlist lead", t, sleeve()));

    let mut t = child("SHOP-40.2", "Wishlist button", Status::Working);
    live(&mut t, 20, 118_000);
    t.flags.same_file = same("SHOP-40.4");
    out.push(v("Dev · same file as SHOP-40.4", t, sleeve()));

    let mut t = child("SHOP-40.4", "Wishlist share link", Status::Working);
    live(&mut t, 60, 71_000);
    t.flags.same_file = same("SHOP-40.2");
    out.push(v("Dev · same file as SHOP-40.2", t.clone(), sleeve()));

    t.flags.unread_notes = Some(unread());
    out.push(v("Dev · both on one card", t, sleeve()));

    let mut t = child("SHOP-40.3", "Wishlist page", Status::CheckFailed);
    t.validation_round = 3;
    t.flags.verdict = Some("orders.test.ts expects list".into());
    t.flags.tokens = Some(163_000);
    t.flags.lead_note_sent = Some(lead_note());
    out.push(v("Dev · Failed 3 times after the lead's note", t, sleeve()));
    out
}

/// An epic and its project's tickets, for the Epic Card rows.
pub struct EpicVariant {
    pub name: &'static str,
    pub epic: Ticket,
    pub tickets: Vec<Ticket>,
    pub branch: Option<&'static str>,
}

/// *Row · epics* and *Row · epics as delivery units (0059)*.
pub fn epic_variants() -> Vec<EpicVariant> {
    let epic = |key: &str, title: &str, status: Status, external: Option<&str>| {
        let mut t = ticket(key, title, status);
        t.kind = Kind::Epic;
        t.external_id = external.map(str::to_owned);
        t
    };
    let child = |epic: &str, key: &str, title: &str, status: Status| {
        let mut t = ticket(key, title, status);
        t.epic = Some(epic.parse().expect("key"));
        t
    };
    let wishlist = |status: Status, children: [Status; 3]| {
        let e = epic("SHOP-40", "Wishlist", status, Some("PAY-123"));
        let mut tickets = vec![e.clone()];
        for (i, (title, s)) in ["Wishlist API", "Wishlist button", "Wishlist page"]
            .into_iter()
            .zip(children)
            .enumerate()
        {
            tickets.push(child("SHOP-40", &format!("SHOP-40.{}", i + 1), title, s));
        }
        (e, tickets)
    };
    let mut out = Vec::new();
    let (e, t) = wishlist(
        Status::Backlog,
        [Status::Merged, Status::Working, Status::Backlog],
    );
    out.push(EpicVariant {
        name: "Epic · Backlog",
        epic: e,
        tickets: t,
        branch: None,
    });
    let (e, t) = wishlist(Status::WaitingForYou, [Status::Merged; 3]);
    out.push(EpicVariant {
        name: "Epic · Ready for Review",
        epic: e,
        tickets: t,
        branch: None,
    });
    let (mut e, mut t) = wishlist(Status::PrOpen, [Status::Merged; 3]);
    e.flags.pr_number = Some(490);
    t[0] = e.clone();
    out.push(EpicVariant {
        name: "Epic · Approved · PR open",
        epic: e,
        tickets: t,
        branch: Some("feature/PAY-123-wishlist"),
    });
    let (e, t) = wishlist(Status::Merged, [Status::Merged; 3]);
    out.push(EpicVariant {
        name: "Epic · Done",
        epic: e,
        tickets: t,
        branch: Some("feature/PAY-123-wishlist"),
    });
    let gift = epic("SHOP-50", "Gift cards", Status::ClosedNotMerged, None);
    out.push(EpicVariant {
        name: "Epic · closed, not merged",
        tickets: vec![
            gift.clone(),
            child(
                "SHOP-50",
                "SHOP-50.1",
                "Gift card model",
                Status::ClosedNotMerged,
            ),
            child(
                "SHOP-50",
                "SHOP-50.2",
                "Redeem gift card",
                Status::ClosedNotMerged,
            ),
        ],
        epic: gift,
        branch: None,
    });
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
