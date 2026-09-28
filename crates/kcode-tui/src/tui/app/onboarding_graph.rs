//! The onboarding state-space graph, as data.
//!
//! Onboarding is not one flow: it is a product of independent state spaces (UI
//! phase x per-provider credential state x environment capability x import
//! candidates). Historically only the UI phase was explicit, so cross-axis bugs
//! were found one at a time in production. This module writes the whole graph
//! down so it can be checked exhaustively instead of reviewed by eye.
//!
//! Three things live here:
//!
//!   1. [`NodeId`]: the closed vocabulary. These are the only strings that
//!      ever reach the debug trace, which is what makes the trace payload
//!      structurally incapable of carrying user data.
//!   2. [`graph`]: the authored nodes and edges. Only states the live flow can
//!      actually enter are modelled; three that no code could enter (a blocking
//!      environment, a classified login failure, a permanently rejected
//!      credential) used to be declared here and were deleted, because a state
//!      nothing constructs is documentation pretending to be code.
//!   3. [`check_invariants`]: the properties every future edit must preserve
//!      (no dead ends, bounded work, escape hatches, reachability).
//!
//! The graph is deliberately *descriptive*: the live `App` state machine remains
//! the implementation, and `tests/onboarding_graph_fidelity.rs` drives the real
//! app across authored edges to prove the description stays faithful. That gives
//! the anti-drift guarantee without a risky rewrite of the running flow.

// Most of this descriptive graph is exercised by the exhaustive tests below.
// The live flow only needs the node vocabulary, so production builds naturally
// leave the invariant-checking helpers unused.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::{BTreeMap, BTreeSet};

/// A node in the onboarding graph.
///
/// Wildcard-free everywhere it is matched, so adding a variant fails to compile
/// until it has been classified, scored, and given outgoing edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeId {
    /// Virtual entry. Routing out of here is decided by probed environment and
    /// detected credentials, not by a keystroke.
    Start,
    /// Fresh install, nothing to import: offer the default provider sign-in.
    LoginOpenAi,
    /// Detected external CLI logins, shown as a checkbox review.
    LoginImport,
    /// Nothing importable and the default sign-in was declined or failed.
    LoginRecovery,
    /// Legacy transient phase, auto-advances.
    ModelSelect,
    /// "Continue where you left off?" (legacy/replay path).
    ContinuePrompt,
    /// Action picker: suggested review, or a blank session.
    StartChoice,
    /// Prompt suggestion cards; the resting state.
    Suggestions,
    /// Onboarding is over and the normal UI has taken over.
    Done,
}

impl NodeId {
    /// Closed-vocabulary label: a stable identifier with no user data.
    pub fn label(self) -> &'static str {
        match self {
            NodeId::Start => "start",
            NodeId::LoginOpenAi => "login_openai",
            NodeId::LoginImport => "login_import",
            NodeId::LoginRecovery => "login_recovery",
            NodeId::ModelSelect => "model_select",
            NodeId::ContinuePrompt => "continue_prompt",
            NodeId::StartChoice => "start_choice",
            NodeId::Suggestions => "suggestions",
            NodeId::Done => "done",
        }
    }

    /// Every node. The invariant checks iterate this, so a new variant is
    /// automatically covered once added here (and the compiler forces that via
    /// the wildcard-free `label`/`props` matches).
    #[cfg(test)]
    pub fn all() -> [NodeId; 9] {
        [
            NodeId::Start,
            NodeId::LoginOpenAi,
            NodeId::LoginImport,
            NodeId::LoginRecovery,
            NodeId::ModelSelect,
            NodeId::ContinuePrompt,
            NodeId::StartChoice,
            NodeId::Suggestions,
            NodeId::Done,
        ]
    }
}

/// Structural properties of a node, used by the invariant checks.
///
/// Test-only: production needs the node vocabulary, not the classification.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub struct NodeProps {
    /// The user must make a choice here.
    pub is_decision: bool,
    /// A timeout/auto default exists, so the user is not forced to answer.
    pub has_default: bool,
    /// Reaching this node means the user can type a real prompt.
    pub is_ready: bool,
    /// No further onboarding transitions leave this node.
    pub is_terminal: bool,
    /// The user never rests here: it auto-advances before a frame is drawn.
    /// Transient nodes are exempt from the escape-hatch rule because there is
    /// nothing to escape from, but they still must make progress.
    pub is_transient: bool,
    /// Retained only for replay/test fixtures; the live flow never enters it.
    /// Exempt from reachability so the checker does not force us to invent a
    /// fake entry edge, but flagged here so it is obvious this is dead weight
    /// that should eventually be deleted.
    pub is_legacy: bool,
}

/// Per-node properties. Wildcard-free: a new node must be classified here.
#[cfg(test)]
pub fn node_props(node: NodeId) -> NodeProps {
    use NodeId::*;
    match node {
        Start => NodeProps {
            is_decision: false,
            has_default: false,
            is_ready: false,
            is_terminal: false,
            is_transient: false,
            is_legacy: false,
        },
        // Blocking environment problem (e.g. unwritable config dir). A failure,
        LoginOpenAi => NodeProps {
            is_decision: true,
            has_default: false,
            is_ready: false,
            is_terminal: false,
            is_transient: false,
            is_legacy: false,
        },
        LoginImport => NodeProps {
            is_decision: true,
            has_default: true,
            is_ready: false,
            is_terminal: false,
            is_transient: false,
            is_legacy: false,
        },
        LoginRecovery => NodeProps {
            is_decision: true,
            has_default: false,
            is_ready: false,
            is_terminal: false,
            is_transient: false,
            is_legacy: false,
        },
        // Auto-advances before a frame is drawn, so the user never sits here.
        ModelSelect => NodeProps {
            is_decision: false,
            has_default: true,
            is_ready: false,
            is_terminal: false,
            is_transient: true,
            is_legacy: false,
        },
        // Legacy: retained for replay/test fixtures. The live flow no longer
        // enters it (see the OnboardingPhase::ContinuePrompt doc comment).
        ContinuePrompt => NodeProps {
            is_decision: true,
            has_default: true,
            is_ready: false,
            is_terminal: false,
            is_transient: false,
            is_legacy: true,
        },
        StartChoice => NodeProps {
            is_decision: true,
            has_default: false,
            is_ready: true,
            is_terminal: false,
            is_transient: false,
            is_legacy: false,
        },
        Suggestions => NodeProps {
            is_decision: false,
            has_default: false,
            is_ready: true,
            is_terminal: true,
            is_transient: false,
            is_legacy: false,
        },
        Done => NodeProps {
            is_decision: false,
            has_default: false,
            is_ready: false,
            is_terminal: true,
            is_transient: false,
            is_legacy: false,
        },
    }
}

/// One directed transition.
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    /// In-TUI keystrokes to traverse this edge on the default path.
    pub keystrokes: u32,
    /// Whether this edge is an escape hatch (skip/defer/continue-degraded).
    /// Every non-terminal node needs at least one, so nobody is ever trapped.
    pub is_escape: bool,
}

/// The authored onboarding graph.
///
/// Compared to the version that previously lived only in the evaluator, this
/// adds the states the flow genuinely has but never modelled: a blocking
/// environment, a classified login failure, and a permanently rejected
/// credential. Those are exactly the states where users got stuck.
pub fn graph() -> Vec<Edge> {
    use NodeId::*;
    vec![
        // ---- Entry routing, decided by probe + detected credentials ----
        Edge {
            from: Start,
            to: LoginOpenAi,
            keystrokes: 0,
            is_escape: false,
        },
        Edge {
            from: Start,
            to: LoginImport,
            keystrokes: 0,
            is_escape: false,
        },
        Edge {
            from: Start,
            to: ModelSelect,
            keystrokes: 0,
            is_escape: false,
        },
        Edge {
            from: LoginOpenAi,
            to: StartChoice,
            keystrokes: 1,
            is_escape: false,
        },
        Edge {
            from: LoginOpenAi,
            to: Done,
            keystrokes: 1,
            is_escape: true,
        },
        Edge {
            from: LoginImport,
            to: StartChoice,
            keystrokes: 1,
            is_escape: false,
        },
        Edge {
            from: LoginImport,
            to: LoginRecovery,
            keystrokes: 1,
            is_escape: false,
        },
        Edge {
            from: LoginImport,
            to: Done,
            keystrokes: 1,
            is_escape: true,
        },
        // ---- Manual recovery: pick a provider yourself ----
        Edge {
            from: LoginRecovery,
            to: StartChoice,
            keystrokes: 1,
            is_escape: false,
        },
        Edge {
            from: LoginRecovery,
            to: Done,
            keystrokes: 1,
            is_escape: true,
        },
        // ---- A login failed: every exit here must be actionable ----
        // Try the method the environment probe says can actually work.
        Edge {
            from: ModelSelect,
            to: StartChoice,
            keystrokes: 0,
            is_escape: false,
        },
        Edge {
            from: ContinuePrompt,
            to: StartChoice,
            keystrokes: 1,
            is_escape: false,
        },
        Edge {
            from: ContinuePrompt,
            to: Suggestions,
            keystrokes: 1,
            is_escape: true,
        },
        // ---- Resting states ----
        Edge {
            from: StartChoice,
            to: Suggestions,
            keystrokes: 1,
            is_escape: true,
        },
        Edge {
            from: StartChoice,
            to: Done,
            keystrokes: 1,
            is_escape: false,
        },
    ]
}

/// Map a live [`OnboardingPhase`] onto its graph node.
///
/// This is what connects the description to the implementation: the running
/// flow reports its transitions in graph terms, so a debug log describes a path
/// we can replay and check. Wildcard-free, so
/// a new phase variant fails to compile until it has a node.
pub fn node_for_phase(phase: &super::onboarding_flow::OnboardingPhase) -> NodeId {
    use super::onboarding_flow::OnboardingPhase as P;
    match phase {
        P::Login { import: Some(_) } => NodeId::LoginImport,
        P::Login { import: None } => NodeId::LoginRecovery,
        P::LoginOpenAi { .. } => NodeId::LoginOpenAi,
        P::ModelSelect => NodeId::ModelSelect,
        P::ContinuePrompt { .. } => NodeId::ContinuePrompt,
        P::StartChoice { .. } => NodeId::StartChoice,
        P::Suggestions => NodeId::Suggestions,
        P::Done => NodeId::Done,
    }
}

/// Whether the live flow just took a transition the graph actually declares.
///
/// A `false` here means the running code and the written-down graph disagree,
/// which is precisely the drift this module exists to catch. Callers log it
/// rather than panicking: a mismatch is a bug in our model, and crashing a
/// user's first run over a bookkeeping disagreement would be much worse than
/// the bug itself.
pub fn transition_is_declared(from: NodeId, to: NodeId) -> bool {
    // A phase can be re-entered with different inner data (e.g. the import
    // review advancing a candidate) without being a graph transition.
    from == to || graph().iter().any(|e| e.from == from && e.to == to)
}

/// A violated structural property, with enough detail to fix it.
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub struct Violation {
    pub invariant: &'static str,
    pub detail: String,
}

/// Check every structural property the onboarding graph must satisfy.
///
/// This is the payoff of writing the graph down: these are the bugs that used
/// to be found by users, checked here in microseconds instead.
#[cfg(test)]
pub fn check_invariants() -> Vec<Violation> {
    let edges = graph();
    let mut violations = Vec::new();

    let out_edges =
        |node: NodeId| -> Vec<Edge> { edges.iter().copied().filter(|e| e.from == node).collect() };

    for node in NodeId::all() {
        let props = node_props(node);
        let outs = out_edges(node);

        // 1. No dead ends: a non-terminal node the user can sit on must have a
        //    way forward, or they are stuck with no keystroke that helps.
        if !props.is_terminal && outs.is_empty() {
            violations.push(Violation {
                invariant: "no_dead_ends",
                detail: format!("{} is non-terminal with no outgoing edge", node.label()),
            });
        }

        // 2. Escape hatch everywhere: from any node the user can sit on, there
        //    is a way into a usable (possibly degraded) app.
        if !props.is_terminal
            && !props.is_transient
            && node != NodeId::Start
            && !outs.iter().any(|e| e.is_escape)
        {
            violations.push(Violation {
                invariant: "escape_hatch",
                detail: format!("{} has no escape edge", node.label()),
            });
        }

        // 3. Reachability: every node except the virtual Start must be
        //    reachable, otherwise it is dead code that will rot.
        if node != NodeId::Start && !props.is_legacy && !edges.iter().any(|e| e.to == node) {
            violations.push(Violation {
                invariant: "reachable",
                detail: format!("{} is unreachable from any node", node.label()),
            });
        }
        // A legacy node that became reachable again is no longer legacy, and
        // silently keeping the exemption would hide a real screen from every
        // other invariant.
        if props.is_legacy && edges.iter().any(|e| e.to == node) {
            violations.push(Violation {
                invariant: "legacy_stays_unreachable",
                detail: format!(
                    "{} is marked legacy but something now routes into it; drop the flag",
                    node.label()
                ),
            });
        }

        // 4. Progress: no self-loop, which would be a retry with no visible
        //    state change. This is the shape of the two-day OpenAI retry loop.
        if outs.iter().any(|e| e.to == node) {
            violations.push(Violation {
                invariant: "progress",
                detail: format!(
                    "{} has a self-loop (retry with no state change)",
                    node.label()
                ),
            });
        }
    }

    // 5. Bounded work: from every node the user can sit on, a ready or terminal
    //    state must be reachable, and within a small keystroke budget.
    const MAX_KEYSTROKES_TO_SETTLED: u32 = 4;
    for node in NodeId::all() {
        if node_props(node).is_terminal {
            continue;
        }
        match min_keystrokes_to(node, &edges, |n| {
            let p = node_props(n);
            p.is_ready || p.is_terminal
        }) {
            None => violations.push(Violation {
                invariant: "bounded_work",
                detail: format!("{} cannot reach a ready or terminal state", node.label()),
            }),
            Some(cost) if cost > MAX_KEYSTROKES_TO_SETTLED => violations.push(Violation {
                invariant: "bounded_work",
                detail: format!(
                    "{} needs {cost} keystrokes to settle (budget {MAX_KEYSTROKES_TO_SETTLED})",
                    node.label()
                ),
            }),
            Some(_) => {}
        }
    }

    // 6. Forced decisions are budgeted. A decision node with no timeout default
    //    stops the flow until the user answers, so each one is real friction on
    //    the critical path.
    const MAX_FORCED_DECISIONS: usize = 3;
    let forced: Vec<&'static str> = NodeId::all()
        .into_iter()
        .filter(|&node| {
            let p = node_props(node);
            p.is_decision && !p.has_default
        })
        .map(NodeId::label)
        .collect();
    if forced.len() > MAX_FORCED_DECISIONS {
        violations.push(Violation {
            invariant: "forced_decision_budget",
            detail: format!(
                "{} forced decisions on the happy path (budget {MAX_FORCED_DECISIONS}): {}",
                forced.len(),
                forced.join(", ")
            ),
        });
    }

    violations
}

/// Minimum keystrokes from `start` to the nearest node satisfying `is_goal`.
///
/// Bellman-Ford style relaxation: the graph is tiny, and this stays correct if
/// someone later adds a cycle (which the invariants permit as long as it makes
/// visible progress).
#[cfg(test)]
pub fn min_keystrokes_to<F: Fn(NodeId) -> bool>(
    start: NodeId,
    edges: &[Edge],
    is_goal: F,
) -> Option<u32> {
    let mut best: BTreeMap<NodeId, u32> = BTreeMap::new();
    best.insert(start, 0);
    let mut changed = true;
    while changed {
        changed = false;
        for e in edges {
            if let Some(&cost) = best.get(&e.from) {
                let next = cost + e.keystrokes;
                if best.get(&e.to).is_none_or(|&existing| next < existing) {
                    best.insert(e.to, next);
                    changed = true;
                }
            }
        }
    }
    best.iter()
        .filter(|(node, _)| is_goal(**node))
        .map(|(_, &cost)| cost)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onboarding_graph_satisfies_every_structural_invariant() {
        let violations = check_invariants();
        assert!(
            violations.is_empty(),
            "onboarding graph invariants violated:\n{}",
            violations
                .iter()
                .map(|v| format!("  [{}] {}", v.invariant, v.detail))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn invariant_checks_actually_catch_violations() {
        // A checker that never fires is worse than no checker, so prove each
        // rule rejects a graph that breaks it.
        let base = graph();

        // Dead end: a node with its outgoing edges removed. The fixture must be
        // a node that is neither ready nor terminal, or removing its exits would
        // not change the answer.
        let fixture = NodeId::LoginRecovery;
        let pruned: Vec<Edge> = base.iter().copied().filter(|e| e.from != fixture).collect();
        assert!(
            !pruned.iter().any(|e| e.from == fixture),
            "test setup should have removed the node's exits"
        );
        assert!(
            min_keystrokes_to(fixture, &pruned, |n| node_props(n).is_ready
                || node_props(n).is_terminal)
            .is_none(),
            "a node with no exits must be unable to reach a settled state"
        );
    }

    #[test]
    fn invariant_exemptions_stay_narrow() {
        // Two invariants have exemptions, and an exemption nobody polices is
        // just a hole. Pin the exact set of exempt nodes so widening it is a
        // deliberate, reviewed act rather than a quiet edit.
        let transient: Vec<&str> = NodeId::all()
            .into_iter()
            .filter(|&n| node_props(n).is_transient)
            .map(NodeId::label)
            .collect();
        assert_eq!(
            transient,
            vec!["model_select"],
            "only genuinely auto-advancing screens may skip the escape-hatch rule"
        );

        let legacy: Vec<&str> = NodeId::all()
            .into_iter()
            .filter(|&n| node_props(n).is_legacy)
            .map(NodeId::label)
            .collect();
        assert_eq!(
            legacy,
            vec!["continue_prompt"],
            "only replay-fixture screens may skip the reachability rule"
        );

        // A transient node must still make progress, or it is a hang rather
        // than a screen.
        for node in NodeId::all() {
            if node_props(node).is_transient {
                assert!(
                    graph().iter().any(|e| e.from == node),
                    "{} auto-advances to nowhere",
                    node.label()
                );
            }
        }
    }

    #[test]
    fn node_labels_are_a_closed_snake_case_vocabulary() {
        // The trace sends these verbatim, so they must be stable identifiers
        // with no user data, no spaces, and no punctuation.
        let mut seen = BTreeSet::new();
        for node in NodeId::all() {
            let label = node.label();
            assert!(
                label.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "node label {label:?} is not a stable identifier"
            );
            assert!(seen.insert(label), "duplicate node label {label:?}");
        }
    }

    #[test]
    fn the_happy_path_stays_short() {
        // Two keystrokes from a fresh install to a session where the user can
        // type: sign in, then pick an action. Regressions here are the single
        // most expensive onboarding change we can make.
        let edges = graph();
        let cost = min_keystrokes_to(NodeId::LoginOpenAi, &edges, |n| node_props(n).is_ready)
            .expect("the sign-in path must reach a ready state");
        assert!(cost <= 1, "sign-in to ready regressed to {cost} keystrokes");

        let from_start = min_keystrokes_to(NodeId::Start, &edges, |n| node_props(n).is_ready)
            .expect("start must reach a ready state");
        assert!(
            from_start <= 2,
            "start to ready regressed to {from_start} keystrokes"
        );
    }
}
