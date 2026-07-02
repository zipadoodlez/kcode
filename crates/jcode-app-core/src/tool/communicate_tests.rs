use super::{
    CommunicateInput, CommunicateTool, canonical_swarm_action, cleanup_candidate_session_ids,
    coordination_in_flight_count, default_await_target_statuses, default_cleanup_target_statuses,
    format_awaited_members, format_awaited_members_with_reports, format_members,
    format_plan_status, latest_assistant_report, resolve_optional_target_session,
    resolve_run_plan_concurrency, swarm_member_is_drivable_worker, swarm_member_is_in_flight,
};
use crate::message::{Message, StreamEvent, ToolDefinition};
use crate::protocol::{
    AgentInfo, AgentStatusSnapshot, AwaitedMemberStatus, HistoryMessage, NotificationType, Request,
    ServerEvent, SessionActivitySnapshot, ToolCallSummary,
};
use crate::provider::{EventStream, Provider};
use crate::server::Server;
use crate::tool::{Tool, ToolContext, ToolExecutionMode};
use crate::transport::{ReadHalf, Stream, WriteHalf};
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[test]
fn tool_is_named_swarm() {
    assert_eq!(CommunicateTool::new().name(), "swarm");
}

#[test]
fn task_id_from_output_path_extracts_background_task_id() {
    assert_eq!(
        super::task_id_from_output_path(Path::new("/tmp/tasks/123abc.output")),
        Some("123abc")
    );
    assert_eq!(
        super::task_id_from_output_path(Path::new("/tmp/tasks/123abc.status.json")),
        None
    );
}

#[tokio::test]
async fn run_plan_reporter_finalize_puts_summary_before_log() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let output_path = dir.path().join("tsk42.output");
    let reporter = super::RunPlanReporter::background(&output_path);
    assert_eq!(reporter.task_id.as_deref(), Some("tsk42"));

    reporter.log("assigned a -> session_fox").await;
    reporter.log("assigned b -> session_wolf").await;
    reporter
        .finalize("Swarm plan reached terminal state.")
        .await;

    let content = tokio::fs::read_to_string(&output_path)
        .await
        .expect("output file");
    let summary_idx = content
        .find("Swarm plan reached terminal state.")
        .expect("summary present");
    let log_idx = content.find("assigned a -> session_fox").expect("log kept");
    assert!(
        summary_idx < log_idx,
        "summary must lead the output file so completion previews are useful:\n{content}"
    );
}

#[tokio::test]
async fn run_plan_reporter_inline_is_a_no_op() {
    let reporter = super::RunPlanReporter::inline();
    assert!(reporter.task_id.is_none());
    // Must not panic or create files.
    reporter.log("ignored").await;
    reporter.progress(1, 2, "ignored".to_string()).await;
    reporter.finalize("ignored").await;
}

#[test]
fn run_plan_concurrency_is_mode_aware() {
    // Light mode (no explicit limit) keeps the small cheap fan-out default.
    assert_eq!(
        resolve_run_plan_concurrency(None, false, 32),
        super::LIGHT_MODE_DEFAULT_CONCURRENCY
    );

    // Deep mode (no explicit limit) fans out wide using the configured cap,
    // NOT the old hardcoded 3 and NOT the light default.
    assert_eq!(resolve_run_plan_concurrency(None, true, 32), 32);
    assert_eq!(resolve_run_plan_concurrency(None, true, 64), 64);

    // Deep mode with the cap set to 0 means "no extra cap": dispatch the whole
    // ready set, bounded only by the swarm member cap.
    assert_eq!(resolve_run_plan_concurrency(None, true, 0), usize::MAX);

    // An explicit request always wins over the mode default, in both modes,
    // and is clamped to at least 1.
    assert_eq!(resolve_run_plan_concurrency(Some(5), true, 32), 5);
    assert_eq!(resolve_run_plan_concurrency(Some(5), false, 32), 5);
    assert_eq!(resolve_run_plan_concurrency(Some(0), true, 32), 1);
}

#[test]
fn run_plan_utilization_tracks_peak_and_starvation() {
    let mut util = super::RunPlanUtilization::default();

    // Loop 1: 0 in flight, 8 open slots, dispatched 8 -> budget fully used.
    util.record_loop(0, Some(8), 8);
    // Loop 2: 8 in flight, 0 open slots, dispatched 0 -> saturated, not starved.
    util.record_loop(8, Some(0), 0);
    // Loop 3: 2 in flight, 6 open slots, dispatched 1 -> starved (5 idle slots).
    util.record_loop(2, Some(6), 1);

    assert_eq!(util.peak_in_flight, 8);
    assert_eq!(util.loops, 3);
    assert_eq!(util.starved_loops, 1);

    let report = util.report(8, true);
    assert!(report.contains("peak 8 of 8"));
    assert!(report.contains("1 of 3 loop(s)"));
    // 1/3 starved and peak 8: healthy run, no hint.
    assert!(!report.contains("Deep-mode hint"));
}

#[test]
fn run_plan_utilization_flags_serial_deep_runs() {
    // A deep run that trickles one task at a time despite a 32-slot budget.
    let mut util = super::RunPlanUtilization::default();
    for _ in 0..4 {
        util.record_loop(0, Some(32), 1);
    }
    assert_eq!(util.peak_in_flight, 1);
    assert_eq!(util.starved_loops, 4);

    let deep_report = util.report(32, true);
    assert!(deep_report.contains("peak 1 of 32"));
    assert!(deep_report.contains("Deep-mode hint"));
    assert!(deep_report.contains("expand"));

    // The same shape in light mode is by design; no nagging.
    let light_report = util.report(32, false);
    assert!(!light_report.contains("Deep-mode hint"));
}

#[test]
fn run_plan_utilization_handles_unbounded_budget() {
    let mut util = super::RunPlanUtilization::default();
    // Unbounded budget (deep_cap=0): open slots are not meaningful, so no
    // starvation accounting, but peak parallelism still records.
    util.record_loop(10, None, 5);
    assert_eq!(util.peak_in_flight, 15);
    assert_eq!(util.starved_loops, 0);
    let report = util.report(usize::MAX, true);
    assert!(report.contains("peak 15 of unbounded"));
}

#[test]
fn plan_status_budget_line_is_deep_only_and_nudges_serialized_graphs() {
    let base = crate::protocol::PlanGraphStatus {
        swarm_id: Some("swarm-a".to_string()),
        version: 1,
        item_count: 10,
        ready_ids: vec!["a".to_string()],
        blocked_ids: Vec::new(),
        active_ids: vec!["b".to_string()],
        completed_ids: vec!["c".to_string()],
        failed_ids: Vec::new(),
        cycle_ids: Vec::new(),
        unresolved_dependency_ids: Vec::new(),
        next_ready_ids: Vec::new(),
        newly_ready_ids: Vec::new(),
        low_confidence_ids: Vec::new(),
        mode: "deep".to_string(),
    };

    // Light plans get no budget line at all.
    let light = crate::protocol::PlanGraphStatus {
        mode: "light".to_string(),
        ..base.clone()
    };
    assert_eq!(super::plan_status_budget_line(&light, 32), None);

    // Deep + narrow frontier (2 of 32) with 7 more items serialized behind
    // edges -> budget line plus the widen nudge.
    let narrow = super::plan_status_budget_line(&base, 32).expect("deep plans get a budget line");
    assert!(narrow.contains("Parallel budget: 32"));
    assert!(narrow.contains("ready set is 1 wide (1 active)"));
    assert!(narrow.contains("expand_node"));

    // Deep + the frontier is all that remains -> line but no nudge.
    let almost_done = crate::protocol::PlanGraphStatus {
        item_count: 3,
        ..base.clone()
    };
    let line = super::plan_status_budget_line(&almost_done, 32).unwrap();
    assert!(line.contains("Parallel budget: 32"));
    assert!(!line.contains("expand_node"));

    // deep_cap=0 (unbounded) surfaces the member cap as the budget.
    let unbounded = super::plan_status_budget_line(&base, 0).unwrap();
    assert!(unbounded.contains("1000 (member cap)"));
}

#[test]
fn assign_error_classification_recovers_on_member_cap_instead_of_failing() {
    use super::AssignErrorAction;

    // Graceful exhaustion of work or workers ends the assignment burst.
    assert_eq!(
        super::classify_assign_error("No runnable unassigned tasks are available in the swarm plan"),
        AssignErrorAction::BreakGracefully
    );
    assert_eq!(
        super::classify_assign_error(
            "No ready or completed swarm agents are available for automatic task assignment."
        ),
        AssignErrorAction::BreakGracefully
    );

    // The member cap must trigger recovery (cleanup + reuse fallback), not a
    // run-aborting failure. The server wraps the cap message in a spawn-failure
    // prefix, so classification must match on the substring.
    assert_eq!(
        super::classify_assign_error(
            "Failed to spawn preferred worker: Swarm member limit reached (max 1000). \
             This swarm already has 1000 agents; it cannot spawn more."
        ),
        AssignErrorAction::RecoverCapacity
    );

    // Anything else is still a real failure.
    assert_eq!(
        super::classify_assign_error("Not in a swarm."),
        AssignErrorAction::Fail
    );
}

#[test]
fn cap_recovery_prefers_cleanup_then_reuse_then_gives_up() {
    use super::CapRecoveryStep;

    // First cap hit with freed capacity: retry keeping the fresh-spawn preference.
    assert_eq!(super::cap_recovery_step(1, 3), CapRecoveryStep::RetryFresh);
    // First cap hit but nothing could be freed: fall back to reusing ready workers.
    assert_eq!(super::cap_recovery_step(1, 0), CapRecoveryStep::RetryReuse);
    // Recovery already ran and the cap still refuses: continue with in-flight
    // work instead of aborting or spinning.
    assert_eq!(super::cap_recovery_step(2, 0), CapRecoveryStep::GiveUp);
    assert_eq!(super::cap_recovery_step(3, 5), CapRecoveryStep::GiveUp);
}

#[test]
fn run_plan_driver_failures_carry_worker_retention_hint() {
    // Every driver-failure path must tell the caller the spawned workers are
    // still running and how to stop them.
    let hinted = super::with_worker_retention_hint(
        "run_plan stalled after 3 loop(s): no ready tasks and no in-flight workers.".to_string(),
    );
    assert!(hinted.contains("Spawned workers were retained"));
    assert!(hinted.contains("swarm cleanup"));

    // Max-loops keeps its intentional retention-for-inspection wording but
    // still gains the actionable hint.
    let max_loops = super::with_worker_retention_hint(
        "run_plan exceeded 200 coordination loops; leaving workers untouched for inspection"
            .to_string(),
    );
    assert!(max_loops.contains("swarm cleanup"));

    // Idempotent: re-wrapping (e.g. the background wrapper re-reporting the
    // error) must not duplicate the hint.
    let twice = super::with_worker_retention_hint(hinted.clone());
    assert_eq!(twice.matches("Spawned workers were retained").count(), 1);
}

#[test]
fn run_plan_terminal_summary_reports_failed_nodes() {
    let base = crate::protocol::PlanGraphStatus {
        swarm_id: Some("swarm-a".to_string()),
        version: 1,
        item_count: 4,
        ready_ids: Vec::new(),
        blocked_ids: Vec::new(),
        active_ids: Vec::new(),
        completed_ids: vec!["a".to_string(), "b".to_string()],
        failed_ids: vec!["c".to_string(), "d".to_string()],
        cycle_ids: Vec::new(),
        unresolved_dependency_ids: Vec::new(),
        next_ready_ids: Vec::new(),
        newly_ready_ids: Vec::new(),
        low_confidence_ids: Vec::new(),
        mode: "deep".to_string(),
    };

    let with_failures = super::format_run_plan_terminal_summary(5, &base, 7);
    assert!(with_failures.contains("completed=2"));
    assert!(with_failures.contains("failed=2"));
    assert!(with_failures.contains("Failed nodes: c, d"));
    assert!(with_failures.contains("did NOT finish cleanly"));

    // A clean run reports failed=0 and no failure callout.
    let clean = crate::protocol::PlanGraphStatus {
        completed_ids: vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ],
        failed_ids: Vec::new(),
        ..base
    };
    let clean_summary = super::format_run_plan_terminal_summary(5, &clean, 7);
    assert!(clean_summary.contains("failed=0"));
    assert!(!clean_summary.contains("Failed nodes"));
}

#[test]
fn plan_terminal_node_count_includes_failed_without_double_counting() {
    let summary = crate::protocol::PlanGraphStatus {
        swarm_id: Some("swarm-a".to_string()),
        version: 1,
        item_count: 4,
        ready_ids: Vec::new(),
        blocked_ids: vec!["x".to_string()],
        active_ids: Vec::new(),
        completed_ids: vec!["a".to_string()],
        failed_ids: vec!["c".to_string()],
        // "x" is both blocked and cyclic; it must count once.
        cycle_ids: vec!["x".to_string()],
        unresolved_dependency_ids: Vec::new(),
        next_ready_ids: Vec::new(),
        newly_ready_ids: Vec::new(),
        low_confidence_ids: Vec::new(),
        mode: "light".to_string(),
    };
    // a (completed) + c (failed) + x (blocked/cycle, deduped) = 3. Without
    // failed_ids in the terminal count a run with failed nodes would never
    // satisfy terminal_count >= item_count and run_plan could spin or stall.
    assert_eq!(super::plan_terminal_node_count(&summary), 3);
}

#[test]
fn canonical_swarm_action_maps_common_synonyms() {
    assert_eq!(canonical_swarm_action("inbox"), "read");
    assert_eq!(canonical_swarm_action("read_messages"), "read");
    assert_eq!(canonical_swarm_action("send"), "message");
    assert_eq!(canonical_swarm_action("msg"), "message");
    assert_eq!(canonical_swarm_action("direct_message"), "dm");
    assert_eq!(canonical_swarm_action("announce"), "broadcast");
    assert_eq!(canonical_swarm_action("agents"), "list");
    assert_eq!(canonical_swarm_action("plan"), "plan_status");
    assert_eq!(canonical_swarm_action("assign"), "assign_task");
    assert_eq!(canonical_swarm_action("kill"), "stop");
}

#[test]
fn canonical_swarm_action_is_case_insensitive_and_trims() {
    assert_eq!(canonical_swarm_action("  Inbox  "), "read");
    assert_eq!(canonical_swarm_action("SEND"), "message");
}

#[test]
fn canonical_swarm_action_passes_through_known_and_unknown_actions() {
    // Real actions are unchanged.
    assert_eq!(canonical_swarm_action("spawn"), "spawn");
    assert_eq!(canonical_swarm_action("dm"), "dm");
    assert_eq!(canonical_swarm_action("assign_role"), "assign_role");
    // Genuinely unknown actions are returned unchanged for normal validation.
    assert_eq!(canonical_swarm_action("totally_made_up"), "totally_made_up");
}

#[test]
fn communicate_input_aliases_to_session_and_target_session() {
    // Either field name should be accepted; the execute() normalization mirrors them.
    let from_target: CommunicateInput = serde_json::from_value(
        json!({ "action": "dm", "message": "hi", "target_session": "worker-1" }),
    )
    .expect("parse target_session input");
    assert_eq!(from_target.target_session.as_deref(), Some("worker-1"));
    assert_eq!(from_target.to_session, None);

    let from_to: CommunicateInput =
        serde_json::from_value(json!({ "action": "summary", "to_session": "worker-2" }))
            .expect("parse to_session input");
    assert_eq!(from_to.to_session.as_deref(), Some("worker-2"));
    assert_eq!(from_to.target_session, None);
}

#[test]
fn format_plan_status_includes_next_ready() {
    let output = format_plan_status(&crate::protocol::PlanGraphStatus {
        swarm_id: Some("swarm-a".to_string()),
        version: 3,
        item_count: 4,
        ready_ids: vec!["task-2".to_string(), "task-3".to_string()],
        blocked_ids: vec!["task-4".to_string()],
        active_ids: vec!["task-1".to_string()],
        completed_ids: vec!["setup".to_string()],
        failed_ids: Vec::new(),
        cycle_ids: Vec::new(),
        unresolved_dependency_ids: Vec::new(),
        next_ready_ids: vec!["task-2".to_string()],
        newly_ready_ids: vec!["task-3".to_string()],
        low_confidence_ids: Vec::new(),
        mode: "deep".to_string(),
    });
    let text = output.output;
    assert!(text.contains("Plan status for swarm swarm-a"));
    assert!(text.contains("Next up: task-2"));
    assert!(text.contains("Newly ready: task-3"));
    assert!(text.contains("Blocked: task-4"));
}

#[test]
fn in_flight_slot_accounting_counts_queued_workers_not_coordinator() {
    let summary = crate::protocol::PlanGraphStatus {
        swarm_id: Some("swarm-a".to_string()),
        version: 3,
        item_count: 4,
        ready_ids: vec!["queued-assigned".to_string()],
        blocked_ids: Vec::new(),
        active_ids: vec!["running-plan-task".to_string()],
        completed_ids: Vec::new(),
        failed_ids: Vec::new(),
        cycle_ids: Vec::new(),
        unresolved_dependency_ids: Vec::new(),
        next_ready_ids: vec!["queued-assigned".to_string()],
        newly_ready_ids: Vec::new(),
        low_confidence_ids: Vec::new(),
        mode: "light".to_string(),
    };
    let members = vec![
        AgentInfo {
            session_id: "coord".to_string(),
            friendly_name: None,
            files_touched: Vec::new(),
            status: Some("running".to_string()),
            detail: None,
            role: Some("coordinator".to_string()),
            is_headless: Some(false),
            report_back_to_session_id: None,
            latest_completion_report: None,
            live_attachments: None,
            status_age_secs: None,
            ..Default::default()
        },
        AgentInfo {
            session_id: "worker-queued".to_string(),
            friendly_name: None,
            files_touched: Vec::new(),
            status: Some("queued".to_string()),
            detail: None,
            role: Some("agent".to_string()),
            is_headless: Some(true),
            report_back_to_session_id: Some("coord".to_string()),
            latest_completion_report: None,
            live_attachments: None,
            status_age_secs: None,
            ..Default::default()
        },
        AgentInfo {
            session_id: "worker-ready".to_string(),
            friendly_name: None,
            files_touched: Vec::new(),
            status: Some("ready".to_string()),
            detail: None,
            role: Some("agent".to_string()),
            is_headless: Some(true),
            report_back_to_session_id: Some("coord".to_string()),
            latest_completion_report: None,
            live_attachments: None,
            status_age_secs: None,
            ..Default::default()
        },
    ];

    assert!(swarm_member_is_in_flight(&members[1]));
    assert!(!swarm_member_is_in_flight(&members[2]));
    assert_eq!(coordination_in_flight_count(&summary, &members, "coord"), 1);
}

#[test]
fn in_flight_count_excludes_foreign_queued_session() {
    // A stale, independent (non-owned, client-attached) session that merely shares
    // the swarm and happens to sit in `queued` must NOT count as in-flight for
    // run_plan: it is never auto-driven, so awaiting it would hang the run even
    // though no plan task is assigned to it. Regression for the run_plan stall.
    let summary = crate::protocol::PlanGraphStatus {
        swarm_id: Some("swarm-a".to_string()),
        version: 1,
        item_count: 1,
        ready_ids: Vec::new(),
        blocked_ids: Vec::new(),
        active_ids: Vec::new(),
        completed_ids: vec!["done-task".to_string()],
        failed_ids: Vec::new(),
        cycle_ids: Vec::new(),
        unresolved_dependency_ids: Vec::new(),
        next_ready_ids: Vec::new(),
        newly_ready_ids: Vec::new(),
        low_confidence_ids: Vec::new(),
        mode: "light".to_string(),
    };
    let members = vec![
        AgentInfo {
            session_id: "coord".to_string(),
            status: Some("running".to_string()),
            role: Some("coordinator".to_string()),
            is_headless: Some(false),
            report_back_to_session_id: None,
            ..Default::default()
        },
        AgentInfo {
            session_id: "foreign-human".to_string(),
            status: Some("queued".to_string()),
            role: Some("agent".to_string()),
            is_headless: Some(false),
            // Not owned by coord, and a live client is attached.
            report_back_to_session_id: None,
            live_attachments: Some(1),
            ..Default::default()
        },
    ];

    // It is technically "in flight" by status, but not a drivable worker, so the
    // scoped count is zero and run_plan can reach its terminal check.
    assert!(swarm_member_is_in_flight(&members[1]));
    assert!(!swarm_member_is_drivable_worker(&members[1], "coord"));
    assert_eq!(coordination_in_flight_count(&summary, &members, "coord"), 0);
}

#[test]
fn latest_assistant_report_uses_last_non_empty_assistant_message() {
    let messages = vec![
        HistoryMessage {
            role: "assistant".to_string(),
            content: " earlier ".to_string(),
            tool_calls: None,
            tool_data: None,
        },
        HistoryMessage {
            role: "user".to_string(),
            content: "ignored".to_string(),
            tool_calls: None,
            tool_data: None,
        },
        HistoryMessage {
            role: "assistant".to_string(),
            content: " final report ".to_string(),
            tool_calls: None,
            tool_data: None,
        },
    ];

    assert_eq!(
        latest_assistant_report(&messages).as_deref(),
        Some("final report")
    );
}

#[test]
fn format_awaited_members_includes_completion_reports() {
    let members = vec![AwaitedMemberStatus {
        session_id: "session_worker".to_string(),
        friendly_name: Some("worker".to_string()),
        status: "ready".to_string(),
        done: true,
        completion_report: Some("Structured report wins.".to_string()),
    }];
    let reports = HashMap::from([(
        "session_worker".to_string(),
        "Outcome: finished. Validation: tests passed.".to_string(),
    )]);

    let output = format_awaited_members_with_reports(
        true,
        "All 1 members are done: worker",
        &members,
        &reports,
    )
    .output;

    assert!(output.contains("Completion reports:"));
    assert!(output.contains("--- worker (ready) ---"));
    assert!(output.contains("Structured report wins."));
    assert!(!output.contains("Outcome: finished"));
}

#[test]
fn resolve_optional_target_session_defaults_to_current() {
    assert_eq!(
        resolve_optional_target_session(None, "session_current"),
        "session_current"
    );
    assert_eq!(
        resolve_optional_target_session(Some("current".to_string()), "session_current"),
        "session_current"
    );
    assert_eq!(
        resolve_optional_target_session(Some("session_other".to_string()), "session_current"),
        "session_other"
    );
}

#[test]
fn schema_still_requires_action() {
    let schema = CommunicateTool::new().parameters_schema();
    assert_eq!(schema["required"], json!(["action"]));
}

#[test]
fn schema_advertises_supported_swarm_fields() {
    let schema = CommunicateTool::new().parameters_schema();
    let props = schema["properties"]
        .as_object()
        .expect("swarm schema should have properties");

    assert!(props.contains_key("action"));
    assert!(props.contains_key("key"));
    assert!(props.contains_key("value"));
    assert!(props.contains_key("message"));
    assert!(props.contains_key("to_session"));
    assert_eq!(
        props["to_session"]["description"],
        json!(
            "Target session for actions that address one agent (dm, and as an alias for target_session). Accepts an exact session ID or a unique friendly name within the swarm. Interchangeable with target_session. If a friendly name is ambiguous, run swarm list and use the exact session ID."
        )
    );
    assert!(props.contains_key("channel"));
    assert!(props.contains_key("proposer_session"));
    assert!(props.contains_key("reason"));
    assert!(props.contains_key("target_session"));
    assert_eq!(
        props["target_session"]["description"],
        json!(
            "Target session for management actions (assign_role, summary, status, stop, start, resume, wake, etc.). Accepts an exact session ID or a unique friendly name. Interchangeable with to_session."
        )
    );
    assert!(props.contains_key("role"));
    assert!(props.contains_key("prompt"));
    assert!(props.contains_key("working_dir"));
    assert!(props.contains_key("limit"));
    assert!(props.contains_key("task_id"));
    assert!(props.contains_key("spawn_if_needed"));
    assert!(props.contains_key("prefer_spawn"));
    assert!(props.contains_key("session_ids"));
    assert!(props.contains_key("mode"));
    assert!(props.contains_key("target_status"));
    assert!(props.contains_key("timeout_minutes"));
    assert!(props.contains_key("concurrency_limit"));
    assert!(props.contains_key("wake"));
    assert!(props.contains_key("delivery"));
    assert!(props.contains_key("plan_items"));
    assert!(props.contains_key("initial_message"));
    assert!(props.contains_key("force"));
    assert!(props.contains_key("retain_agents"));
    assert!(props.contains_key("background"));
    assert!(
        props["background"]["description"]
            .as_str()
            .expect("background description")
            .contains("run_plan"),
        "background flag should document run_plan support"
    );
    assert!(props.contains_key("notify"));
    assert!(props.contains_key("status"));
    assert!(props.contains_key("validation"));
    assert!(props.contains_key("follow_up"));
    assert_eq!(
        props["delivery"]["enum"],
        json!(["notify", "interrupt", "wake"])
    );
    assert_eq!(
        props["plan_items"]["items"]["additionalProperties"],
        json!(true)
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("status"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("report"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("plan_status"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("start"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("start_task"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("assign_next"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("fill_slots"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("run_plan"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("cleanup"))
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("salvage"))
    );
}

struct EnvGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var_os(key);
        crate::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.original.take() {
            crate::env::set_var(self.key, value);
        } else {
            crate::env::remove_var(self.key);
        }
    }
}

struct DelayedTestProvider {
    delay: Duration,
}

#[async_trait]
impl Provider for DelayedTestProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        let delay = self.delay;
        let stream = futures::stream::once(async move {
            tokio::time::sleep(delay).await;
            Ok(StreamEvent::TextDelta("ok".to_string()))
        })
        .chain(futures::stream::once(async {
            Ok(StreamEvent::MessageEnd { stop_reason: None })
        }));
        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        "test"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self { delay: self.delay })
    }
}

struct RawClient {
    reader: BufReader<ReadHalf>,
    writer: WriteHalf,
    next_id: u64,
}

impl RawClient {
    async fn connect(path: &Path) -> Result<Self> {
        let stream = Stream::connect(path).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
            next_id: 1,
        })
    }

    async fn send_request(&mut self, request: Request) -> Result<u64> {
        let id = request.id();
        let json = serde_json::to_string(&request)? + "\n";
        self.writer.write_all(json.as_bytes()).await?;
        Ok(id)
    }

    async fn read_event(&mut self) -> Result<ServerEvent> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            anyhow::bail!("server disconnected")
        }
        Ok(serde_json::from_str(&line)?)
    }

    async fn read_until<F>(&mut self, timeout: Duration, mut predicate: F) -> Result<ServerEvent>
    where
        F: FnMut(&ServerEvent) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let event = tokio::time::timeout(remaining, self.read_event()).await??;
            if predicate(&event) {
                return Ok(event);
            }
        }
    }

    async fn subscribe(&mut self, working_dir: &Path) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_request(Request::Subscribe {
            id,
            working_dir: Some(working_dir.display().to_string()),
            selfdev: None,
            target_session_id: None,
            client_instance_id: None,
            client_has_local_history: false,
            allow_session_takeover: false,
            terminal_env: Vec::new(),
        })
        .await?;
        self.read_until(
            Duration::from_secs(5),
            |event| matches!(event, ServerEvent::Done { id: done_id } if *done_id == id),
        )
        .await?;
        Ok(())
    }

    async fn session_id(&mut self) -> Result<String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_request(Request::GetState { id }).await?;
        match self
            .read_until(
                Duration::from_secs(5),
                |event| matches!(event, ServerEvent::State { id: event_id, .. } if *event_id == id),
            )
            .await?
        {
            ServerEvent::State { session_id, .. } => Ok(session_id),
            other => anyhow::bail!("unexpected state response: {other:?}"),
        }
    }

    async fn send_message(&mut self, content: &str) -> Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_request(Request::Message {
            id,
            content: content.to_string(),
            images: vec![],
            system_reminder: None,
        })
        .await
    }

    async fn wait_for_done(&mut self, request_id: u64) -> Result<()> {
        self.read_until(
            Duration::from_secs(10),
            |event| matches!(event, ServerEvent::Done { id } if *id == request_id),
        )
        .await?;
        Ok(())
    }

    async fn comm_list(&mut self, session_id: &str) -> Result<Vec<AgentInfo>> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_request(Request::CommList {
            id,
            session_id: session_id.to_string(),
        })
        .await?;
        match self
                .read_until(Duration::from_secs(5), |event| {
                    matches!(event, ServerEvent::CommMembers { id: event_id, .. } if *event_id == id)
                })
                .await?
            {
                ServerEvent::CommMembers { members, .. } => Ok(members),
                other => anyhow::bail!("unexpected comm_list response: {other:?}"),
            }
    }

    async fn comm_status(
        &mut self,
        session_id: &str,
        target_session: &str,
    ) -> Result<AgentStatusSnapshot> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_request(Request::CommStatus {
            id,
            session_id: session_id.to_string(),
            target_session: target_session.to_string(),
        })
        .await?;
        match self
                .read_until(Duration::from_secs(5), |event| {
                    matches!(event, ServerEvent::CommStatusResponse { id: event_id, .. } if *event_id == id)
                })
                .await?
            {
                ServerEvent::CommStatusResponse { snapshot, .. } => Ok(snapshot),
                other => anyhow::bail!("unexpected comm_status response: {other:?}"),
            }
    }

    /// Wait for the next `Message` notification and return its scope
    /// ("dm", "channel", or "broadcast"). Other events are skipped.
    async fn next_message_notification(&mut self, timeout: Duration) -> Result<Option<String>> {
        match self
            .read_until(timeout, |event| {
                matches!(
                    event,
                    ServerEvent::Notification {
                        notification_type: NotificationType::Message { .. },
                        ..
                    }
                )
            })
            .await?
        {
            ServerEvent::Notification {
                notification_type: NotificationType::Message { scope, .. },
                ..
            } => Ok(scope),
            other => anyhow::bail!("unexpected notification response: {other:?}"),
        }
    }
}

async fn wait_for_server_socket(
    path: &Path,
    server_task: &mut tokio::task::JoinHandle<Result<()>>,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if server_task.is_finished() {
            let result = server_task.await?;
            return Err(anyhow::anyhow!(
                "server exited before socket became ready: {:?}",
                result
            ));
        }
        match Stream::connect(path).await {
            Ok(stream) => {
                drop(stream);
                return Ok(());
            }
            Err(err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(err.into());
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
}

fn test_ctx(session_id: &str, working_dir: &Path) -> ToolContext {
    ToolContext {
        session_id: session_id.to_string(),
        message_id: "msg-1".to_string(),
        tool_call_id: "call-1".to_string(),
        working_dir: Some(working_dir.to_path_buf()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    }
}

async fn wait_for_member_status(
    client: &mut RawClient,
    requester_session: &str,
    target_session: &str,
    expected_status: &str,
) -> Result<Vec<AgentInfo>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let members = client.comm_list(requester_session).await?;
        if members
            .iter()
            .find(|member| member.session_id == target_session)
            .and_then(|member| member.status.as_deref())
            == Some(expected_status)
        {
            return Ok(members);
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for member {} to reach status {}",
                target_session,
                expected_status
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_member_presence(
    client: &mut RawClient,
    requester_session: &str,
    target_session: &str,
) -> Result<Vec<AgentInfo>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let members = client.comm_list(requester_session).await?;
        if members
            .iter()
            .any(|member| member.session_id == target_session)
        {
            return Ok(members);
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for member {} to appear", target_session);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[test]
fn default_await_members_targets_include_ready() {
    assert_eq!(
        default_await_target_statuses(),
        vec!["ready", "completed", "stopped", "failed"]
    );
}

include!("communicate_tests/input_format.rs");
include!("communicate_tests/end_to_end.rs");
include!("communicate_tests/assignment.rs");
