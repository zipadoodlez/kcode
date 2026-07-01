#![cfg_attr(test, allow(clippy::await_holding_lock))]

use super::{Tool, ToolContext, ToolOutput};
use crate::plan::PlanItem;
use crate::protocol::{
    AgentInfo, AgentStatusSnapshot, AwaitedMemberStatus, CommDeliveryMode, ContextEntry,
    HistoryMessage, PlanGraphStatus, Request, ServerEvent, SwarmChannelInfo, ToolCallSummary,
    comm_cleanup_candidate_session_ids, default_comm_await_target_statuses,
    default_comm_cleanup_target_statuses, default_comm_run_await_statuses,
    format_comm_awaited_members_with_reports, format_comm_channels, format_comm_context_entries,
    format_comm_context_history, format_comm_members, format_comm_plan_followup,
    format_comm_plan_status, format_comm_status_snapshot, format_comm_tool_summary,
    latest_assistant_comm_report, resolve_optional_comm_target_session,
};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const REQUEST_ID: u64 = 1;

/// Default number of workers `run_plan` keeps active at once for a **light**-mode
/// plan. Light mode is the cheap fan-out preset, so this stays small. Deep mode
/// instead uses `agents.swarm_max_concurrent_agents` (high, configurable).
const LIGHT_MODE_DEFAULT_CONCURRENCY: usize = 4;

mod transport;
use transport::{send_request, send_request_with_timeout};

fn fresh_spawn_request_nonce(ctx: &ToolContext) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{}-{}-{}", ctx.session_id, ctx.message_id, now_ms)
}

fn check_error(response: &ServerEvent) -> Option<&str> {
    if let ServerEvent::Error { message, .. } = response {
        Some(message)
    } else {
        None
    }
}

fn ensure_success(response: &ServerEvent) -> Result<()> {
    if let Some(message) = check_error(response) {
        Err(anyhow::anyhow!(message.to_string()))
    } else {
        Ok(())
    }
}

async fn fetch_plan_status(session_id: &str) -> Result<PlanGraphStatus> {
    let request = Request::CommPlanStatus {
        id: REQUEST_ID,
        session_id: session_id.to_string(),
    };
    match send_request(request).await {
        Ok(ServerEvent::CommPlanStatusResponse { summary, .. }) => Ok(summary),
        Ok(response) => {
            ensure_success(&response)?;
            Err(anyhow::anyhow!("No plan status returned."))
        }
        Err(e) => Err(anyhow::anyhow!("Failed to get plan status: {}", e)),
    }
}

fn format_plan_followup(summary: &PlanGraphStatus) -> String {
    format_comm_plan_followup(summary)
}

fn default_cleanup_target_statuses() -> Vec<String> {
    default_comm_cleanup_target_statuses()
}

fn default_run_await_statuses() -> Vec<String> {
    default_comm_run_await_statuses()
}

fn cleanup_candidate_session_ids(
    owner_session_id: &str,
    members: &[AgentInfo],
    target_status: &[String],
    requested_session_ids: &[String],
    force: bool,
) -> Vec<String> {
    comm_cleanup_candidate_session_ids(
        owner_session_id,
        members,
        target_status,
        requested_session_ids,
        force,
    )
}

fn auto_assignment_needs_spawn(response: &ServerEvent) -> bool {
    check_error(response).is_some_and(|message| {
        message.contains(
            "No ready or completed swarm agents are available for automatic task assignment",
        )
    })
}

async fn fetch_swarm_members(session_id: &str) -> Result<Vec<AgentInfo>> {
    let request = Request::CommList {
        id: REQUEST_ID,
        session_id: session_id.to_string(),
    };
    match send_request(request).await {
        Ok(ServerEvent::CommMembers { members, .. }) => Ok(members),
        Ok(response) => {
            ensure_success(&response)?;
            Ok(Vec::new())
        }
        Err(e) => Err(anyhow::anyhow!("Failed to list swarm members: {}", e)),
    }
}

fn swarm_member_is_in_flight(member: &AgentInfo) -> bool {
    matches!(
        member.status.as_deref(),
        Some("queued" | "running" | "running_stale")
    )
}

fn coordination_in_flight_count(
    summary: &PlanGraphStatus,
    members: &[AgentInfo],
    current_session_id: &str,
) -> usize {
    summary.active_ids.len().max(
        members
            .iter()
            .filter(|member| member.session_id != current_session_id)
            .filter(|member| swarm_member_is_in_flight(member))
            .filter(|member| swarm_member_is_drivable_worker(member, current_session_id))
            .count(),
    )
}

/// Sessions `run_plan` should await as genuinely in-flight on *this* plan.
///
/// A member counts only when it is both in-flight (`queued`/`running`) **and** a
/// drivable worker for this run: headless, or owned by the coordinator
/// (`report_back_to_session_id == coordinator`). This deliberately excludes
/// independent, client-attached human sessions that merely share the swarm and
/// happen to sit in a `queued` status. Awaiting those would hang `run_plan`
/// forever even though every plan task is already terminal (they are never auto
/// driven), which is exactly the stall this scoping prevents.
async fn fetch_in_flight_swarm_sessions(session_id: &str) -> Result<Vec<String>> {
    let members = fetch_swarm_members(session_id).await?;
    Ok(members
        .into_iter()
        .filter(|member| member.session_id != session_id)
        .filter(swarm_member_is_in_flight)
        .filter(|member| swarm_member_is_drivable_worker(member, session_id))
        .map(|member| member.session_id)
        .collect())
}

/// Whether `member` is a worker `run_plan` can rely on to autonomously execute an
/// assignment (and therefore one it is safe to await): a spawned headless worker,
/// or one owned by the coordinator that issued the run. Foreign client-attached
/// sessions are not drivable and must not gate `run_plan` completion.
fn swarm_member_is_drivable_worker(member: &AgentInfo, coordinator_session_id: &str) -> bool {
    member.is_headless.unwrap_or(false)
        || member.report_back_to_session_id.as_deref() == Some(coordinator_session_id)
}

async fn cleanup_swarm_workers(ctx: &ToolContext, params: &CommunicateInput) -> Result<String> {
    let members = fetch_swarm_members(&ctx.session_id).await?;
    let target_status = params
        .target_status
        .clone()
        .unwrap_or_else(default_cleanup_target_statuses);
    let session_ids = params.session_ids.clone().unwrap_or_default();
    let force = params.force.unwrap_or(false);
    let candidates = cleanup_candidate_session_ids(
        &ctx.session_id,
        &members,
        &target_status,
        &session_ids,
        force,
    );

    if candidates.is_empty() {
        return Ok(format!(
            "No cleanup candidates found. Default cleanup only stops sessions spawned by this coordinator with status in [{}].",
            target_status.join(", ")
        ));
    }

    let mut stopped = Vec::new();
    let mut failed = Vec::new();
    for target in candidates {
        let request = Request::CommStop {
            id: REQUEST_ID,
            session_id: ctx.session_id.clone(),
            target_session: target.clone(),
            force: Some(force),
        };
        match send_request(request).await {
            Ok(response) => match ensure_success(&response) {
                Ok(()) => stopped.push(target),
                Err(error) => failed.push(format!("{} ({})", target, error)),
            },
            Err(error) => failed.push(format!("{} ({})", target, error)),
        }
    }

    let mut output = String::new();
    if stopped.is_empty() {
        output.push_str("Stopped no swarm workers.");
    } else {
        output.push_str(&format!(
            "Stopped {} swarm worker(s): {}",
            stopped.len(),
            stopped.join(", ")
        ));
    }
    if !failed.is_empty() {
        output.push_str(&format!(
            "\nFailed to stop {} worker(s): {}",
            failed.len(),
            failed.join(", ")
        ));
    }
    Ok(output)
}

async fn await_swarm_progress(
    ctx: &ToolContext,
    session_ids: Vec<String>,
    timeout_minutes: u64,
) -> Result<()> {
    let request = Request::CommAwaitMembers {
        id: REQUEST_ID,
        session_id: ctx.session_id.clone(),
        target_status: default_run_await_statuses(),
        session_ids,
        mode: Some("any".to_string()),
        timeout_secs: Some(timeout_minutes.max(1) * 60),
        // run_plan needs the result inline to drive its coordination loop, so it
        // explicitly opts out of the background-by-default behavior.
        background: false,
        notify: false,
        wake: false,
    };
    let socket_timeout = std::time::Duration::from_secs(timeout_minutes.max(1) * 60 + 30);
    match send_request_with_timeout(request, Some(socket_timeout)).await {
        Ok(ServerEvent::CommAwaitMembersResponse {
            completed, summary, ..
        }) => {
            if completed {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Timed out waiting for swarm progress: {}",
                    summary
                ))
            }
        }
        Ok(response) => ensure_success(&response),
        Err(e) => Err(anyhow::anyhow!(
            "Failed while awaiting swarm progress: {}",
            e
        )),
    }
}

/// Decide how many swarm workers `run_plan` keeps active at once.
///
/// Policy:
///   * an explicit `requested` limit always wins (clamped to >= 1);
///   * deep mode with no explicit limit fans out wide: use `deep_cap`, where
///     `0` means "no extra cap" (`usize::MAX`) so the whole ready set is
///     dispatched, bounded only by the swarm member cap;
///   * light mode with no explicit limit keeps the small, cheap fan-out default.
///
/// Pure and side-effect free so the concurrency contract is unit-testable
/// without a live swarm.
fn resolve_run_plan_concurrency(requested: Option<usize>, is_deep: bool, deep_cap: usize) -> usize {
    match requested {
        Some(explicit) => explicit.max(1),
        None if is_deep => {
            if deep_cap == 0 {
                usize::MAX
            } else {
                deep_cap
            }
        }
        None => LIGHT_MODE_DEFAULT_CONCURRENCY,
    }
}

/// Running tally of how well a `run_plan` drive used its concurrency budget.
///
/// Deep mode's promise is comprehensiveness through parallel fan-out, so a run
/// that finishes with peak parallelism ~1 despite a 32+ slot budget means the
/// graph was decomposed serially and the budget was wasted. Tracking this per
/// loop (max in-flight, plus how often open slots sat idle with no ready work)
/// turns "did we actually use the budget?" into a measured, reportable number
/// instead of a hope.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RunPlanUtilization {
    /// Highest number of simultaneously in-flight tasks observed.
    peak_in_flight: usize,
    /// Coordination loops observed.
    loops: usize,
    /// Loops where open worker slots existed but the plan had nothing ready to
    /// dispatch into them (budget idle due to graph narrowness, not the cap).
    starved_loops: usize,
}

impl RunPlanUtilization {
    /// Record one coordination loop. `open_slots` is `None` when the budget is
    /// unbounded (`concurrency_limit == usize::MAX`): an infinite budget has no
    /// meaningful starvation denominator, so only peak parallelism is tracked.
    fn record_loop(&mut self, in_flight: usize, open_slots: Option<usize>, dispatched: usize) {
        self.loops += 1;
        self.peak_in_flight = self.peak_in_flight.max(in_flight + dispatched);
        if let Some(open_slots) = open_slots
            && open_slots > 0
            && dispatched < open_slots
        {
            self.starved_loops += 1;
        }
    }

    /// Render the utilization line for the terminal report. In deep mode a
    /// starved run also gets an actionable hint, because the fix (wider
    /// decomposition) belongs to the model reading this output.
    fn report(&self, concurrency_limit: usize, is_deep: bool) -> String {
        let limit_label = if concurrency_limit == usize::MAX {
            "unbounded".to_string()
        } else {
            concurrency_limit.to_string()
        };
        let mut line = format!(
            "Budget utilization: peak {} of {} concurrent worker slot(s); {} of {} loop(s) had idle capacity with nothing ready.",
            self.peak_in_flight, limit_label, self.starved_loops, self.loops
        );
        let mostly_starved = self.loops > 0 && self.starved_loops * 2 >= self.loops;
        let ran_narrow = self.loops >= 3 && self.peak_in_flight <= 2;
        if is_deep && (mostly_starved || ran_narrow) {
            line.push_str(
                "\nDeep-mode hint: the graph ran much narrower than the agent budget. If coverage \
                 matters, expand remaining or follow-up work into MANY independent sibling nodes \
                 (depends_on only for real data dependencies) so the ready set fills the budget.",
            );
        }
        line
    }
}

async fn run_swarm_plan_to_terminal(
    ctx: &ToolContext,
    params: &CommunicateInput,
) -> Result<ToolOutput> {
    let initial_summary = fetch_plan_status(&ctx.session_id).await?;
    let is_deep = initial_summary.mode.eq_ignore_ascii_case("deep");

    let configured_deep_cap = crate::config::config().agents.swarm_max_concurrent_agents;
    let concurrency_limit =
        resolve_run_plan_concurrency(params.concurrency_limit, is_deep, configured_deep_cap);
    let timeout_minutes = params.timeout_minutes.unwrap_or(60).max(1);
    let retain_agents = params.retain_agents.unwrap_or(false);
    let spawn_if_needed = params.spawn_if_needed.or(Some(true));
    // Default to a fresh worker per task-graph node. Reusing a worker that already
    // completed a *different* node carries that node's conversation into the next
    // assignment, and the model often just re-reports its prior result instead of
    // doing the new work (observed leaving gap/synthesis nodes stuck). The task-DAG
    // model assumes clean, isolated workers, so unless the caller explicitly opts
    // into reuse (`prefer_spawn=false`), prefer spawning a fresh worker per node.
    let prefer_spawn = params.prefer_spawn.or(Some(true));
    let mut assignment_count = 0usize;
    let mut loop_count = 0usize;
    let max_loops = 200usize;
    let mut utilization = RunPlanUtilization::default();
    // Consecutive loops where an active task exists but no drivable worker is
    // awaitable. This is normally a brief transition (a composite re-waking to
    // synthesize, or a just-finished task whose member status has not propagated),
    // so we back off and re-check a few times before declaring a real stall.
    let mut transient_stall_loops = 0usize;
    let max_transient_stall_loops = 5usize;

    loop {
        loop_count += 1;
        if loop_count > max_loops {
            return Err(anyhow::anyhow!(
                "run_plan exceeded {} coordination loops; leaving workers untouched for inspection",
                max_loops
            ));
        }

        let summary = fetch_plan_status(&ctx.session_id).await?;
        if summary.item_count == 0 {
            return Ok(ToolOutput::new("No swarm plan items to run."));
        }

        let in_flight_sessions = fetch_in_flight_swarm_sessions(&ctx.session_id).await?;

        let terminal_count =
            summary.completed_ids.len() + summary.blocked_ids.len() + summary.cycle_ids.len();
        let no_more_runnable = summary.active_ids.is_empty()
            && summary.next_ready_ids.is_empty()
            && in_flight_sessions.is_empty();
        if no_more_runnable || terminal_count >= summary.item_count {
            let mut output = format!(
                "Swarm plan reached terminal/blocked state after {} loop(s). completed={} blocked={} cycles={} active={} assignments={}",
                loop_count,
                summary.completed_ids.len(),
                summary.blocked_ids.len(),
                summary.cycle_ids.len(),
                summary.active_ids.len(),
                assignment_count
            );
            output.push_str(&format!(
                "\n{}",
                utilization.report(concurrency_limit, is_deep)
            ));
            if retain_agents {
                output.push_str("\nRetained spawned workers because retain_agents=true.");
            } else {
                let cleanup = cleanup_swarm_workers(ctx, params).await?;
                output.push_str(&format!("\n{}", cleanup));
            }
            return Ok(ToolOutput::new(output));
        }

        let active_count = summary.active_ids.len().max(in_flight_sessions.len());
        let available_slots = concurrency_limit.saturating_sub(active_count);
        let mut assigned_sessions = Vec::new();
        for _ in 0..available_slots {
            let request = Request::CommAssignNext {
                id: REQUEST_ID,
                session_id: ctx.session_id.clone(),
                target_session: params.target_session.clone(),
                working_dir: params.working_dir.clone(),
                prefer_spawn,
                spawn_if_needed,
                message: params.message.clone(),
            };
            match send_request(request).await {
                Ok(ServerEvent::CommAssignTaskResponse { target_session, .. }) => {
                    assignment_count += 1;
                    assigned_sessions.push(target_session);
                }
                Ok(ServerEvent::Error { message, .. })
                    if message.contains("No runnable unassigned tasks")
                        || message.contains("No ready or completed swarm agents") =>
                {
                    break;
                }
                Ok(response) => ensure_success(&response)?,
                Err(e) => return Err(anyhow::anyhow!("Failed to assign next swarm task: {}", e)),
            }
        }
        utilization.record_loop(
            active_count,
            (concurrency_limit != usize::MAX).then_some(available_slots),
            assigned_sessions.len(),
        );

        let await_sessions = if assigned_sessions.is_empty() {
            in_flight_sessions
        } else {
            assigned_sessions
        };

        if await_sessions.is_empty() {
            if active_count > 0 {
                // An active task exists but nothing drivable is awaitable. This is
                // usually transient: a composite is re-waking to synthesize, or a
                // worker just finished and its member status has not propagated yet.
                // Re-check a few times with a short backoff before giving up, and
                // bail early if the plan reaches a terminal state in the meantime.
                transient_stall_loops += 1;
                if transient_stall_loops <= max_transient_stall_loops {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }
                return Err(anyhow::anyhow!(
                    "run_plan found {} active task(s) but no running swarm members to await after {} re-checks; inspect plan_status and member list before retrying",
                    active_count,
                    max_transient_stall_loops
                ));
            }
            // Nothing was assigned this loop, nothing is in flight, yet the plan is
            // not terminal. This means some non-terminal task cannot be driven, e.g.
            // it is already assigned to a session run_plan cannot drive (a foreign or
            // stale member). Spinning here would busy-loop to the max-loop cap, so
            // surface the stuck state with the offending tasks instead.
            let stuck: Vec<String> = summary
                .next_ready_ids
                .iter()
                .chain(summary.ready_ids.iter())
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let detail = if stuck.is_empty() {
                "no ready tasks and no in-flight workers".to_string()
            } else {
                format!(
                    "runnable task(s) {} could not be assigned to any drivable worker",
                    stuck.join(", ")
                )
            };
            return Err(anyhow::anyhow!(
                "run_plan stalled after {} loop(s): {}. This usually means a task is assigned to a session run_plan cannot drive (foreign or stale member). Reassign with an explicit target_session, or clear the stale assignment, then retry.",
                loop_count,
                detail
            ));
        }
        await_swarm_progress(ctx, await_sessions, timeout_minutes).await?;
        // Real progress (an await completed); clear the transient-stall backoff so
        // a later genuine stall starts counting fresh.
        transient_stall_loops = 0;
    }
}

async fn spawn_assignment_session(ctx: &ToolContext, params: &CommunicateInput) -> Result<String> {
    let spawn_request = Request::CommSpawn {
        id: REQUEST_ID,
        session_id: ctx.session_id.clone(),
        working_dir: params.working_dir.clone(),
        initial_message: None,
        request_nonce: Some(fresh_spawn_request_nonce(ctx)),
        spawn_mode: params.spawn_mode.clone(),
    };

    match send_request(spawn_request).await {
        Ok(ServerEvent::CommSpawnResponse { new_session_id, .. }) if !new_session_id.is_empty() => {
            Ok(new_session_id)
        }
        Ok(spawn_response) => {
            ensure_success(&spawn_response)?;
            Err(anyhow::anyhow!(
                "Spawn succeeded but new session ID was not returned."
            ))
        }
        Err(e) => Err(anyhow::anyhow!(
            "Failed to spawn agent for task assignment: {}",
            e
        )),
    }
}

async fn assign_task_to_session(
    ctx: &ToolContext,
    params: &CommunicateInput,
    target_session: String,
    spawned_suffix: &str,
) -> Result<ToolOutput> {
    let retry_request = Request::CommAssignTask {
        id: REQUEST_ID,
        session_id: ctx.session_id.clone(),
        target_session: Some(target_session.clone()),
        task_id: params.task_id.clone(),
        message: params.message.clone(),
    };

    match send_request(retry_request).await {
        Ok(ServerEvent::CommAssignTaskResponse { task_id, .. }) => Ok(ToolOutput::new(format!(
            "Task '{}' assigned to {}{}",
            task_id, target_session, spawned_suffix
        ))),
        Ok(retry_response) => {
            ensure_success(&retry_response)?;
            Ok(ToolOutput::new(format!(
                "Assigned next runnable task to {}{}",
                target_session, spawned_suffix
            )))
        }
        Err(e) => Err(anyhow::anyhow!(
            "Failed to assign task after selecting {}: {}",
            target_session,
            e
        )),
    }
}

fn format_context_entries(entries: &[ContextEntry]) -> ToolOutput {
    ToolOutput::new(format_comm_context_entries(entries))
}

fn format_members(ctx: &ToolContext, members: &[AgentInfo]) -> ToolOutput {
    ToolOutput::new(format_comm_members(&ctx.session_id, members))
}

fn format_tool_summary(target: &str, calls: &[ToolCallSummary]) -> ToolOutput {
    ToolOutput::new(format_comm_tool_summary(target, calls))
}

fn format_status_snapshot(snapshot: &AgentStatusSnapshot) -> ToolOutput {
    ToolOutput::new(format_comm_status_snapshot(snapshot))
}

fn format_plan_status(summary: &PlanGraphStatus) -> ToolOutput {
    let mut output = format_comm_plan_status(summary);
    if let Some(budget_line) = plan_status_budget_line(
        summary,
        crate::config::config().agents.swarm_max_concurrent_agents,
    ) {
        output.push_str(&budget_line);
    }
    ToolOutput::new(output)
}

/// Deep-mode budget line for `plan_status`: how wide the ready frontier is
/// versus the concurrency budget, with a widen-the-graph nudge when the ready
/// set cannot fill the slots. This makes under-utilization visible at plan
/// time, before `run_plan` even starts, so the coordinator can restructure the
/// graph instead of discovering the waste after the run. Pure over its inputs
/// for unit testing; returns `None` for light plans.
fn plan_status_budget_line(summary: &PlanGraphStatus, deep_cap: usize) -> Option<String> {
    if !summary.mode.eq_ignore_ascii_case("deep") {
        return None;
    }
    let budget = resolve_run_plan_concurrency(None, true, deep_cap);
    let budget_label = if budget == usize::MAX {
        format!("{} (member cap)", jcode_swarm_core::MAX_SWARM_MEMBERS)
    } else {
        budget.to_string()
    };
    let ready_width = summary.ready_ids.len();
    let active_width = summary.active_ids.len();
    let mut line = format!(
        "  Parallel budget: {} concurrent worker slot(s); ready set is {} wide ({} active).\n",
        budget_label, ready_width, active_width
    );
    let effective_budget = if budget == usize::MAX {
        jcode_swarm_core::MAX_SWARM_MEMBERS
    } else {
        budget
    };
    // Nudge only when narrowness is structural: the frontier cannot fill the
    // budget while other non-terminal work exists but is serialized behind
    // depends_on edges. A small plan that is simply almost done gets no nudge.
    let frontier = ready_width + active_width;
    let terminal = summary.completed_ids.len() + summary.cycle_ids.len();
    let serialized_remaining = summary.item_count > terminal + frontier;
    if frontier < effective_budget && serialized_remaining {
        line.push_str(
            "  The ready frontier is narrower than the budget while more work waits behind \
             depends_on edges: prefer expand_node with MANY independent siblings (depends_on \
             only for real data dependencies) to widen it.\n",
        );
    }
    Some(line)
}

fn format_context_history(target: &str, messages: &[HistoryMessage]) -> ToolOutput {
    ToolOutput::new(format_comm_context_history(target, messages))
}

#[cfg(test)]
fn format_awaited_members(
    completed: bool,
    summary: &str,
    members: &[AwaitedMemberStatus],
) -> ToolOutput {
    format_awaited_members_with_reports(completed, summary, members, &HashMap::new())
}

fn latest_assistant_report(messages: &[HistoryMessage]) -> Option<String> {
    latest_assistant_comm_report(messages)
}

fn resolve_optional_target_session(target: Option<String>, current_session: &str) -> String {
    resolve_optional_comm_target_session(target, current_session)
}

fn format_awaited_members_with_reports(
    completed: bool,
    summary: &str,
    members: &[AwaitedMemberStatus],
    reports: &HashMap<String, String>,
) -> ToolOutput {
    ToolOutput::new(format_comm_awaited_members_with_reports(
        completed, summary, members, reports,
    ))
}

async fn fetch_awaited_member_reports(
    ctx: &ToolContext,
    members: &[AwaitedMemberStatus],
) -> HashMap<String, String> {
    let mut reports = HashMap::new();
    for member in members.iter().filter(|member| member.done) {
        let request = Request::CommReadContext {
            id: REQUEST_ID,
            session_id: ctx.session_id.clone(),
            target_session: member.session_id.clone(),
        };
        match send_request(request).await {
            Ok(ServerEvent::CommContextHistory { messages, .. }) => {
                if let Some(report) = latest_assistant_report(&messages) {
                    reports.insert(member.session_id.clone(), report);
                }
            }
            Ok(response) => {
                if check_error(&response).is_some() {
                    continue;
                }
            }
            Err(_) => continue,
        }
    }
    reports
}

fn default_await_target_statuses() -> Vec<String> {
    default_comm_await_target_statuses()
}

fn format_channels(channels: &[SwarmChannelInfo]) -> ToolOutput {
    ToolOutput::new(format_comm_channels(channels))
}

pub struct CommunicateTool;

impl CommunicateTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Deserialize)]
struct CommunicateInput {
    action: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    to_session: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    proposer_session: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    target_session: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    initial_message: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    spawn_if_needed: Option<bool>,
    #[serde(default)]
    prefer_spawn: Option<bool>,
    #[serde(default)]
    plan_items: Option<Vec<PlanItem>>,
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default)]
    gate_id: Option<String>,
    /// Task-DAG node specs for task_graph/expand_node/inject_gap actions.
    #[serde(default)]
    nodes: Option<Vec<crate::protocol::TaskGraphNodeSpec>>,
    /// Handoff artifact (object) for complete_node.
    #[serde(default)]
    artifact: Option<serde_json::Value>,
    #[serde(default)]
    target_status: Option<Vec<String>>,
    #[serde(default)]
    session_ids: Option<Vec<String>>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    timeout_minutes: Option<u64>,
    #[serde(default)]
    wake: Option<bool>,
    #[serde(default)]
    background: Option<bool>,
    #[serde(default)]
    notify: Option<bool>,
    #[serde(default)]
    delivery: Option<CommDeliveryMode>,
    #[serde(default)]
    concurrency_limit: Option<usize>,
    #[serde(default)]
    force: Option<bool>,
    #[serde(default)]
    retain_agents: Option<bool>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    validation: Option<String>,
    #[serde(default)]
    follow_up: Option<String>,
    #[serde(default)]
    spawn_mode: Option<String>,
}

impl CommunicateInput {
    fn spawn_initial_message(&self) -> Option<String> {
        self.initial_message.clone().or_else(|| self.prompt.clone())
    }
}

/// Map common action synonyms/typos to the canonical swarm action name. Models
/// frequently invent near-miss verbs (e.g. `inbox` for reading messages, `send`
/// for `message`), which previously produced an "Unknown action" error. Unknown
/// inputs are returned unchanged so the normal validation path still reports them.
fn canonical_swarm_action(action: &str) -> &str {
    match action.trim().to_ascii_lowercase().as_str() {
        "inbox" | "messages" | "check_messages" | "read_messages" | "read_inbox" => "read",
        "send" | "msg" | "send_message" => "message",
        "dm_session" | "direct_message" | "whisper" => "dm",
        "broadcast_all" | "announce" => "broadcast",
        "agents" | "members" | "list_agents" | "list_members" | "roster" => "list",
        "plan" | "status_plan" => "plan_status",
        "assign" => "assign_task",
        "kill" | "terminate" => "stop",
        _ => action,
    }
}

#[async_trait]
impl Tool for CommunicateTool {
    fn name(&self) -> &str {
        "swarm"
    }

    fn description(&self) -> &str {
        "Coordinate agents. Any agent can spawn child agents, and those children can spawn their own, forming a recursive spawn tree capped at depth 5. For spawn, prefer providing a prompt so the new agent starts with a concrete task instead of idling. Spawned/assigned agents automatically report their final response back to the agent that spawned them; you can stop any agent in the subtree you spawned."
    }

    fn parameters_schema(&self) -> Value {
        let mut schema = json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ["share", "share_append", "read", "message", "broadcast", "dm", "channel", "list", "list_channels", "channel_members",
                             "propose_plan", "approve_plan", "reject_plan", "spawn", "stop", "assign_role",
                             "status", "report", "plan_status", "summary", "read_context", "resync_plan", "assign_task", "assign_next", "fill_slots", "run_plan", "cleanup",
                             "task_graph", "expand_node", "complete_node", "inject_gap",
                             "start", "start_task", "wake", "resume", "retry", "reassign", "replace", "salvage",
                             "subscribe_channel", "unsubscribe_channel", "await_members"],
                    "description": "Action. For spawn, prefer including prompt with the initial task so the new agent starts useful work immediately."
                },
                "key": {
                    "type": "string"
                },
                "value": {
                    "type": "string"
                },
                "message": {
                    "type": "string",
                    "description": "Message body. For action=message, routes by fields provided: with to_session it is a DM, with channel it posts to that channel, with neither it broadcasts to the whole swarm. For action=broadcast it always goes to the whole swarm. For action=report, this is the completion report body."
                },
                "status": {
                    "type": "string",
                    "description": "For action=report: completion status to record, usually ready, blocked, failed, or completed. Defaults to ready."
                },
                "validation": {
                    "type": "string",
                    "description": "For action=report: tests or validation performed."
                },
                "follow_up": {
                    "type": "string",
                    "description": "For action=report: blockers or follow-up work."
                },
                "to_session": {
                    "type": "string",
                    "description": "Target session for actions that address one agent (dm, and as an alias for target_session). Accepts an exact session ID or a unique friendly name within the swarm. Interchangeable with target_session. If a friendly name is ambiguous, run swarm list and use the exact session ID."
                },
                "channel": {
                    "type": "string",
                    "description": "Channel name. For action=channel (or action=message with a channel) the message goes to subscribers of this channel. Also used by subscribe_channel/unsubscribe_channel/channel_members."
                },
                "proposer_session": { "type": "string" },
                "reason": { "type": "string" },
                "target_session": {
                    "type": "string",
                    "description": "Target session for management actions (assign_role, summary, status, stop, start, resume, wake, etc.). Accepts an exact session ID or a unique friendly name. Interchangeable with to_session."
                },
                "role": {
                    "type": "string",
                    "enum": ["agent", "coordinator", "worktree_manager"]
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory for spawn."
                },
                "prompt": {
                    "type": "string",
                    "description": "Preferred for spawn. Initial task/instructions for the new agent. Spawning without prompt usually creates an idle agent that needs follow-up assignment."
                },
                "initial_message": {
                    "type": "string",
                    "description": "Explicit initial task/instructions for spawn. If both initial_message and prompt are supplied, initial_message wins."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional max items for summary-style reads."
                },
                "task_id": {
                    "type": "string",
                    "description": "Optional plan task ID. If omitted for assign_task/assign_next, the coordinator picks a runnable task. If omitted for resume/wake/retry/start with target_session, the server resumes the unique assigned task for that session."
                },
                "spawn_if_needed": {
                    "type": "boolean",
                    "description": "For assign_task without an explicit target_session: if no reusable agent is available, spawn a fresh agent and retry the assignment automatically."
                },
                "prefer_spawn": {
                    "type": "boolean",
                    "description": "For assign_task without an explicit target_session: prefer a fresh spawned agent even if reusable workers are available."
                },
                "spawn_mode": {
                    "type": "string",
                    "enum": ["visible", "headless", "inline", "auto"],
                    "description": "Per-call spawn mode for swarm-created agents. Overrides agents.swarm_spawn_mode config when set. 'visible' opens a terminal window, 'headless' runs in-process with no UI, 'inline' runs in-process and renders a live gallery viewport in the coordinator, 'auto' tries visible then falls back to headless. Defaults to visible/headed behavior."
                },
                "session_ids": {
                    "type": "array",
                    "items": {"type": "string"}
                },
                "mode": {
                    "type": "string",
                    "enum": ["all", "any"],
                    "description": "For await_members: wait for all targeted members or wake when any targeted member matches."
                },
                "target_status": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Optional completion statuses for await_members. Defaults to ready/completed/stopped/failed."
                },
                "timeout_minutes": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional timeout for await_members."
                },
                "background": {
                    "type": "boolean",
                    "description": "For await_members: run the wait as a detached background watcher (default true) so you stay responsive and can keep working. The result is delivered later via notify/wake. Set false to block this turn until the wait resolves."
                },
                "notify": {
                    "type": "boolean",
                    "description": "For await_members: surface a notification card when a background wait resolves. Defaults to true."
                },
                "concurrency_limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Max swarm worker agents active at once. For fill_slots this is required. For run_plan it is optional and overrides the mode-based default (deep fans out wide up to agents.swarm_max_concurrent_agents; light uses a small default). Total agents over the whole run is still bounded only by the swarm member cap."
                },
                "force": {
                    "type": "boolean",
                    "description": "For stop/cleanup: allow stopping non-owned/user-created swarm sessions. Defaults to false."
                },
                "retain_agents": {
                    "type": "boolean",
                    "description": "For run_plan: keep spawned workers after the plan reaches a terminal state. Defaults to false, so owned workers are cleaned up."
                },
                "wake": {
                    "type": "boolean",
                    "description": "Optional wake hint for messages. For await_members: wake this agent with the result when a background wait resolves (default true); if false, only notify."
                },
                "delivery": {
                    "type": "string",
                    "enum": ["notify", "interrupt", "wake"],
                    "description": "Optional delivery mode for dm/channel messaging."
                },
                "plan_items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "additionalProperties": true
                    }
                }
            }
        });

        // Task-DAG properties are added after the macro to keep `json!` nesting
        // depth under the macro recursion limit.
        if let Some(props) = schema
            .get_mut("properties")
            .and_then(|value| value.as_object_mut())
        {
            props.insert(
                "node_id".to_string(),
                json!({
                    "type": "string",
                    "description": "Task-DAG node id for expand_node/complete_node."
                }),
            );
            props.insert(
                "gate_id".to_string(),
                json!({
                    "type": "string",
                    "description": "Gate node id for inject_gap (a critique/verify gate the caller owns)."
                }),
            );
            props.insert(
                "nodes".to_string(),
                json!({
                    "type": "array",
                    "description": "Task-DAG node specs for task_graph (seed), expand_node (children), or inject_gap (gap/fix nodes). Each: {id, content, kind?, depends_on?, priority?}. kind is one of explore|implement|verify|fix|synthesize.",
                    "items": { "type": "object", "additionalProperties": true }
                }),
            );
            props.insert(
                "artifact".to_string(),
                json!({
                    "type": "object",
                    "description": "Typed handoff artifact for complete_node. In deep mode requires non-empty 'findings' and a 'what_i_did_not_check' list. Fields: findings, evidence[], edge_cases_considered[], validation, open_questions[], confidence, what_i_did_not_check[].",
                    "additionalProperties": true
                }),
            );
        }

        schema
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let mut params: CommunicateInput = serde_json::from_value(input)?;

        // `to_session` and `target_session` both name a single session id. Historically
        // different actions required different field names (e.g. `dm` wanted `to_session`
        // while `assign_role`/`summary`/`status`/`start`/`resume` wanted `target_session`),
        // which models frequently confuse, producing repeated "'to_session' is required" /
        // "'target_session' is required" errors. Treat the two fields as interchangeable
        // aliases so either name works for any action that targets a session.
        match (params.to_session.is_some(), params.target_session.is_some()) {
            (true, false) => params.target_session = params.to_session.clone(),
            (false, true) => params.to_session = params.target_session.clone(),
            _ => {}
        }

        // Normalize common action synonyms that models invent (e.g. `inbox`, `send`,
        // `msg`) so a near-miss verb maps to the real action instead of erroring out.
        params.action = canonical_swarm_action(&params.action).to_string();

        match params.action.as_str() {
            "share" | "share_append" => {
                let key = params
                    .key
                    .ok_or_else(|| anyhow::anyhow!("'key' is required for share action"))?;
                let value = params
                    .value
                    .ok_or_else(|| anyhow::anyhow!("'value' is required for share action"))?;

                let request = Request::CommShare {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    key: key.clone(),
                    value: value.clone(),
                    append: params.action == "share_append",
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        let verb = if params.action == "share_append" {
                            "Appended shared context"
                        } else {
                            "Shared with other agents"
                        };
                        Ok(ToolOutput::new(format!("{}: {} = {}", verb, key, value)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to share: {}", e)),
                }
            }

            "read" => {
                let request = Request::CommRead {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    key: params.key.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommContext { entries, .. }) => {
                        Ok(format_context_entries(&entries))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No shared context found."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to read shared context: {}", e)),
                }
            }

            "message" => {
                // `message` is the general-purpose send: it routes by the fields
                // provided. With `to_session` it acts as a DM, with `channel` it
                // posts to that channel, and with neither it broadcasts to the
                // whole swarm. `broadcast` is the explicit group-only send.
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for message action"))?;
                let to_session = params.to_session.clone();
                let channel = params.channel.clone();

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: to_session.clone(),
                    channel: channel.clone(),
                    wake: params.wake,
                    delivery: params.delivery,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        let confirmation = match (to_session, channel) {
                            (Some(target), _) => {
                                format!("Direct message sent to {}: {}", target, message)
                            }
                            (None, Some(channel)) => {
                                format!("Channel message sent to #{}: {}", channel, message)
                            }
                            (None, None) => {
                                format!("Broadcast sent to all agents: {}", message)
                            }
                        };
                        Ok(ToolOutput::new(confirmation))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to send message: {}", e)),
                }
            }

            "broadcast" => {
                // `broadcast` always targets the whole swarm. Any `to_session`/
                // `channel` is intentionally ignored so the action stays an
                // unambiguous group send; use `message`/`dm`/`channel` to target.
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for broadcast action"))?;

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: None,
                    channel: None,
                    wake: params.wake,
                    delivery: params.delivery,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Broadcast sent to all agents: {}",
                            message
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to broadcast message: {}", e)),
                }
            }

            "dm" => {
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for dm action"))?;
                let to_session = params.to_session.ok_or_else(|| {
                    anyhow::anyhow!("'to_session' (or 'target_session') is required for dm action")
                })?;

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: Some(to_session.clone()),
                    channel: None,
                    delivery: params.delivery,
                    wake: params.wake,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Direct message sent to {}: {}",
                            to_session, message
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to send DM: {}", e)),
                }
            }

            "channel" => {
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for channel action"))?;
                let channel = params
                    .channel
                    .ok_or_else(|| anyhow::anyhow!("'channel' is required for channel action"))?;

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: None,
                    channel: Some(channel.clone()),
                    delivery: params.delivery,
                    wake: params.wake,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Channel message sent to #{}: {}",
                            channel, message
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to send channel message: {}", e)),
                }
            }

            "list" => {
                let request = Request::CommList {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommMembers { members, .. }) => {
                        Ok(format_members(&ctx, &members))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No agents found."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to list agents: {}", e)),
                }
            }

            "list_channels" => {
                let request = Request::CommListChannels {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommChannels { channels, .. }) => {
                        Ok(format_channels(&channels))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No channels found."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to list channels: {}", e)),
                }
            }

            "channel_members" => {
                let channel = params.channel.ok_or_else(|| {
                    anyhow::anyhow!("'channel' is required for channel_members action")
                })?;
                let request = Request::CommChannelMembers {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    channel: channel.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommMembers { members, .. }) => {
                        let mut output = format!("Members subscribed to #{}:\n\n", channel);
                        if members.is_empty() {
                            output.push_str("  (none)\n");
                        } else {
                            for member in members {
                                let name = member.friendly_name.unwrap_or(member.session_id);
                                let status = member.status.unwrap_or_else(|| "unknown".to_string());
                                output.push_str(&format!("  {} ({})\n", name, status));
                            }
                        }
                        Ok(ToolOutput::new(output))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No channel members found."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to list channel members: {}", e)),
                }
            }

            "propose_plan" => {
                let items = params.plan_items.ok_or_else(|| {
                    anyhow::anyhow!("'plan_items' is required for propose_plan action")
                })?;
                if items.is_empty() {
                    return Err(anyhow::anyhow!(
                        "'plan_items' must include at least one item"
                    ));
                }
                let item_count = items.len() as u64;

                let request = Request::CommProposePlan {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    items,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Plan proposal submitted ({} items).",
                            item_count
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to propose plan: {}", e)),
                }
            }

            "approve_plan" => {
                let proposer = params.proposer_session.ok_or_else(|| {
                    anyhow::anyhow!("'proposer_session' is required for approve_plan action")
                })?;

                let request = Request::CommApprovePlan {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    proposer_session: proposer.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Approved plan proposal from {}",
                            proposer
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to approve plan: {}", e)),
                }
            }

            "reject_plan" => {
                let proposer = params.proposer_session.ok_or_else(|| {
                    anyhow::anyhow!("'proposer_session' is required for reject_plan action")
                })?;
                let reason = params.reason.clone();

                let request = Request::CommRejectPlan {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    proposer_session: proposer.clone(),
                    reason: reason.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        let reason_msg = reason
                            .as_ref()
                            .map(|r| format!(" (reason: {})", r))
                            .unwrap_or_default();
                        Ok(ToolOutput::new(format!(
                            "Rejected plan proposal from {}{}",
                            proposer, reason_msg
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to reject plan: {}", e)),
                }
            }

            "task_graph" | "seed_graph" => {
                let nodes = params
                    .nodes
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("'nodes' is required for task_graph action"))?;
                if nodes.is_empty() {
                    return Err(anyhow::anyhow!("'nodes' must include at least one node"));
                }
                let count = nodes.len();
                let request = Request::CommSeedGraph {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    mode: params.mode.clone(),
                    nodes,
                };
                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Seeded task graph ({} nodes).",
                            count
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to seed task graph: {}", e)),
                }
            }

            "expand_node" => {
                let node_id = params.node_id.clone().ok_or_else(|| {
                    anyhow::anyhow!("'node_id' is required for expand_node action")
                })?;
                let children = params.nodes.clone().ok_or_else(|| {
                    anyhow::anyhow!("'nodes' (children) is required for expand_node action")
                })?;
                if children.is_empty() {
                    return Err(anyhow::anyhow!("expand_node requires at least one child"));
                }
                let count = children.len();
                let request = Request::CommExpandNode {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    node_id: node_id.clone(),
                    children,
                };
                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Decomposed '{}' into {} children.",
                            node_id, count
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to expand node: {}", e)),
                }
            }

            "complete_node" => {
                let node_id = params.node_id.clone().ok_or_else(|| {
                    anyhow::anyhow!("'node_id' is required for complete_node action")
                })?;
                let artifact_json = match params.artifact.clone() {
                    Some(value) => serde_json::to_string(&value)
                        .map_err(|e| anyhow::anyhow!("invalid artifact: {}", e))?,
                    None => {
                        return Err(anyhow::anyhow!(
                            "'artifact' object is required for complete_node action"
                        ));
                    }
                };
                let request = Request::CommCompleteNode {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    node_id: node_id.clone(),
                    artifact_json,
                };
                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!("Completed node '{}'.", node_id)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to complete node: {}", e)),
                }
            }

            "inject_gap" => {
                let gate_id = params
                    .gate_id
                    .clone()
                    .or_else(|| params.node_id.clone())
                    .ok_or_else(|| {
                        anyhow::anyhow!("'gate_id' is required for inject_gap action")
                    })?;
                let nodes = params
                    .nodes
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("'nodes' is required for inject_gap action"))?;
                if nodes.is_empty() {
                    return Err(anyhow::anyhow!("inject_gap requires at least one node"));
                }
                let count = nodes.len();
                let request = Request::CommInjectGap {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    gate_id: gate_id.clone(),
                    nodes,
                };
                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Injected {} gap node(s) from gate '{}'.",
                            count, gate_id
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to inject gap nodes: {}", e)),
                }
            }

            "spawn" => {
                let request = Request::CommSpawn {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    working_dir: params.working_dir.clone(),
                    initial_message: params.spawn_initial_message(),
                    request_nonce: None,
                    spawn_mode: params.spawn_mode.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommSpawnResponse { new_session_id, .. })
                        if !new_session_id.is_empty() =>
                    {
                        Ok(ToolOutput::new(format!(
                            "Spawned new agent: {}",
                            new_session_id
                        )))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Err(anyhow::anyhow!(
                            "Spawn succeeded but new session ID was not returned."
                        ))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to spawn agent: {}", e)),
                }
            }

            "stop" => {
                let target = params.target_session.ok_or_else(|| {
                    anyhow::anyhow!("'target_session' is required for stop action")
                })?;

                let request = Request::CommStop {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_session: target.clone(),
                    force: params.force,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!("Stopped agent: {}", target)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to stop agent: {}", e)),
                }
            }

            "cleanup" => cleanup_swarm_workers(&ctx, &params)
                .await
                .map(ToolOutput::new),

            "assign_role" => {
                let target_raw = params.target_session.ok_or_else(|| {
                    anyhow::anyhow!("'target_session' is required for assign_role action")
                })?;
                let role = params
                    .role
                    .ok_or_else(|| anyhow::anyhow!("'role' is required for assign_role action"))?;

                // Resolve "current" to the caller's own session ID
                let target = if target_raw == "current" {
                    ctx.session_id.clone()
                } else {
                    target_raw
                };

                let request = Request::CommAssignRole {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_session: target.clone(),
                    role: role.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Assigned role '{}' to {}",
                            role, target
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to assign role: {}", e)),
                }
            }

            "status" => {
                let target =
                    resolve_optional_target_session(params.target_session, &ctx.session_id);

                let request = Request::CommStatus {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_session: target.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommStatusResponse { snapshot, .. }) => {
                        Ok(format_status_snapshot(&snapshot))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No status snapshot returned."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to get status snapshot: {}", e)),
                }
            }

            "report" => {
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for report action"))?;
                let request = Request::CommReport {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    status: params.status,
                    message,
                    validation: params.validation,
                    follow_up: params.follow_up,
                };
                match send_request(request).await {
                    Ok(ServerEvent::CommReportResponse {
                        status, message, ..
                    }) => Ok(ToolOutput::new(format!(
                        "Report recorded with status `{status}`. {message}"
                    ))),
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("Report recorded."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to record report: {}", e)),
                }
            }

            "plan_status" => {
                let summary = fetch_plan_status(&ctx.session_id).await?;
                Ok(format_plan_status(&summary))
            }

            "summary" => {
                let target = params.target_session.ok_or_else(|| {
                    anyhow::anyhow!("'target_session' is required for summary action")
                })?;

                let request = Request::CommSummary {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_session: target.clone(),
                    limit: params.limit,
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommSummaryResponse { tool_calls, .. }) => {
                        Ok(format_tool_summary(&target, &tool_calls))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No tool call data returned."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to get summary: {}", e)),
                }
            }

            "read_context" => {
                let target = params.target_session.ok_or_else(|| {
                    anyhow::anyhow!("'target_session' is required for read_context action")
                })?;

                let request = Request::CommReadContext {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_session: target.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommContextHistory { messages, .. }) => {
                        Ok(format_context_history(&target, &messages))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No context data returned."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to read context: {}", e)),
                }
            }

            "resync_plan" => {
                let request = Request::CommResyncPlan {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("Swarm plan re-synced to your session."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to resync plan: {}", e)),
                }
            }

            "assign_task" => {
                let target = params
                    .target_session
                    .clone()
                    .unwrap_or_else(|| "next available agent".to_string());
                let spawn_if_needed = params.spawn_if_needed.unwrap_or(false);
                let prefer_spawn = params.prefer_spawn.unwrap_or(false);

                if prefer_spawn && params.target_session.is_none() {
                    let spawned_session = spawn_assignment_session(&ctx, &params).await?;
                    return assign_task_to_session(
                        &ctx,
                        &params,
                        spawned_session,
                        " (spawned by planner preference)",
                    )
                    .await;
                }

                let request = Request::CommAssignTask {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_session: params.target_session.clone(),
                    task_id: params.task_id.clone(),
                    message: params.message.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommAssignTaskResponse {
                        task_id,
                        target_session,
                        ..
                    }) => {
                        let mut output =
                            format!("Task '{}' assigned to {}", task_id, target_session);
                        if let Ok(summary) = fetch_plan_status(&ctx.session_id).await {
                            output.push_str(&format!("\n{}", format_plan_followup(&summary)));
                        }
                        Ok(ToolOutput::new(output))
                    }
                    Ok(response)
                        if spawn_if_needed
                            && params.target_session.is_none()
                            && auto_assignment_needs_spawn(&response) =>
                    {
                        let spawned_session = spawn_assignment_session(&ctx, &params).await?;
                        assign_task_to_session(
                            &ctx,
                            &params,
                            spawned_session,
                            " (spawned automatically)",
                        )
                        .await
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        let msg = params.task_id.as_deref().map_or_else(
                            || format!("Assigned next runnable task to {}", target),
                            |task_id| format!("Task '{}' assigned to {}", task_id, target),
                        );
                        Ok(ToolOutput::new(msg))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to assign task: {}", e)),
                }
            }

            "assign_next" => {
                let target = params
                    .target_session
                    .clone()
                    .unwrap_or_else(|| "next available agent".to_string());

                let request = Request::CommAssignNext {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_session: params.target_session.clone(),
                    working_dir: params.working_dir.clone(),
                    prefer_spawn: params.prefer_spawn,
                    spawn_if_needed: params.spawn_if_needed,
                    message: params.message.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommAssignTaskResponse {
                        task_id,
                        target_session,
                        ..
                    }) => Ok(ToolOutput::new(format!(
                        "Task '{}' assigned to {}",
                        task_id, target_session
                    ))),
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Assigned next runnable task to {}",
                            target
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to assign next task: {}", e)),
                }
            }

            "fill_slots" => {
                let concurrency_limit = params.concurrency_limit.ok_or_else(|| {
                    anyhow::anyhow!("'concurrency_limit' is required for fill_slots action")
                })?;

                let summary = fetch_plan_status(&ctx.session_id).await?;
                let members = fetch_swarm_members(&ctx.session_id).await?;

                let active_count =
                    coordination_in_flight_count(&summary, &members, &ctx.session_id);
                if active_count >= concurrency_limit {
                    return Ok(ToolOutput::new(format!(
                        "Window already full: {} active/in-flight task(s) >= limit {}",
                        active_count, concurrency_limit
                    )));
                }

                let mut assignments = Vec::new();
                let available_slots = concurrency_limit.saturating_sub(active_count);
                for _ in 0..available_slots {
                    let request = Request::CommAssignNext {
                        id: REQUEST_ID,
                        session_id: ctx.session_id.clone(),
                        target_session: params.target_session.clone(),
                        working_dir: params.working_dir.clone(),
                        prefer_spawn: params.prefer_spawn,
                        spawn_if_needed: params.spawn_if_needed,
                        message: params.message.clone(),
                    };

                    match send_request(request).await {
                        Ok(ServerEvent::CommAssignTaskResponse {
                            task_id,
                            target_session,
                            ..
                        }) => assignments.push(format!("{} -> {}", task_id, target_session)),
                        Ok(ServerEvent::Error { message, .. })
                            if message.contains("No runnable unassigned tasks")
                                || message.contains("No ready or completed swarm agents") =>
                        {
                            break;
                        }
                        Ok(response) => {
                            ensure_success(&response)?;
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!("Failed to fill slots: {}", e));
                        }
                    }
                }

                if assignments.is_empty() {
                    Ok(ToolOutput::new(format!(
                        "No assignments made. Active: {}, limit: {}",
                        active_count, concurrency_limit
                    )))
                } else {
                    let mut output = format!(
                        "Filled {} slot(s):\n{}",
                        assignments.len(),
                        assignments
                            .into_iter()
                            .map(|line| format!("- {}", line))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    if let Ok(summary) = fetch_plan_status(&ctx.session_id).await {
                        output.push_str(&format!("\n{}", format_plan_followup(&summary)));
                    }
                    Ok(ToolOutput::new(output))
                }
            }

            "run_plan" => run_swarm_plan_to_terminal(&ctx, &params).await,

            "start" | "start_task" | "wake" | "resume" | "retry" | "reassign" | "replace"
            | "salvage" => {
                let task_id = match params.task_id.clone() {
                    Some(task_id) => task_id,
                    None if params.target_session.is_some() => String::new(),
                    None => {
                        return Err(anyhow::anyhow!(
                            "'task_id' is required for {} action unless 'target_session' uniquely identifies the assigned task. Use `swarm list`/`swarm plan_status` to inspect assignments, or pass task_id explicitly.",
                            params.action
                        ));
                    }
                };
                if matches!(params.action.as_str(), "reassign" | "replace" | "salvage")
                    && params.target_session.is_none()
                {
                    return Err(anyhow::anyhow!(
                        "'target_session' is required for {} action",
                        params.action
                    ));
                }

                let control_action = if params.action == "start_task" {
                    "start".to_string()
                } else {
                    params.action.clone()
                };

                let request = Request::CommTaskControl {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    action: control_action.clone(),
                    task_id: task_id.clone(),
                    target_session: params.target_session.clone(),
                    message: params.message.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommTaskControlResponse {
                        task_id,
                        action,
                        target_session,
                        status,
                        summary,
                        ..
                    }) => {
                        let mut output = format!("Task '{}' {}", task_id, action);
                        if let Some(target_session) = target_session {
                            output.push_str(&format!(" -> {}", target_session));
                        }
                        output.push_str(&format!("\nStatus: {}", status));
                        if !summary.next_ready_ids.is_empty() {
                            output.push_str(&format!(
                                "\nNext ready: {}",
                                summary.next_ready_ids.join(", ")
                            ));
                        }
                        if !summary.newly_ready_ids.is_empty() {
                            output.push_str(&format!(
                                "\nNewly ready: {}",
                                summary.newly_ready_ids.join(", ")
                            ));
                        }
                        Ok(ToolOutput::new(output))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        let target_suffix = params
                            .target_session
                            .as_deref()
                            .map(|target| format!(" -> {}", target))
                            .unwrap_or_default();
                        Ok(ToolOutput::new(format!(
                            "Task '{}' {}{}",
                            task_id, params.action, target_suffix
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to {} task: {}", control_action, e)),
                }
            }

            "subscribe_channel" => {
                let channel = params.channel.ok_or_else(|| {
                    anyhow::anyhow!("'channel' is required for subscribe_channel action")
                })?;

                let request = Request::CommSubscribeChannel {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    channel: channel.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!("Subscribed to #{}", channel)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to subscribe: {}", e)),
                }
            }

            "unsubscribe_channel" => {
                let channel = params.channel.ok_or_else(|| {
                    anyhow::anyhow!("'channel' is required for unsubscribe_channel action")
                })?;

                let request = Request::CommUnsubscribeChannel {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    channel: channel.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!("Unsubscribed from #{}", channel)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to unsubscribe: {}", e)),
                }
            }

            "await_members" => {
                let target_status = params
                    .target_status
                    .unwrap_or_else(default_await_target_statuses);
                let mut session_ids = params.session_ids.unwrap_or_default();
                if let Some(target_session) = params.target_session.clone()
                    && !session_ids.iter().any(|id| id == &target_session)
                {
                    session_ids.push(target_session);
                }
                let timeout_minutes = params.timeout_minutes.unwrap_or(60);
                let timeout_secs = timeout_minutes * 60;
                // Background-by-default: the watch runs server-side and reports
                // back via notify/wake, so the agent stays responsive instead of
                // parking the whole turn. Pass background=false to block inline.
                let background = params.background.unwrap_or(true);
                let notify = params.notify.unwrap_or(true);
                let wake = params.wake.unwrap_or(true);

                let request = Request::CommAwaitMembers {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_status,
                    session_ids,
                    mode: params.mode.clone(),
                    timeout_secs: Some(timeout_secs),
                    background,
                    notify,
                    wake,
                };

                // Background waits return promptly with a snapshot; only blocking
                // waits need the long socket timeout that covers the full wait.
                let socket_timeout = if background {
                    std::time::Duration::from_secs(30)
                } else {
                    std::time::Duration::from_secs(timeout_secs + 30)
                };

                match send_request_with_timeout(request, Some(socket_timeout)).await {
                    Ok(ServerEvent::CommAwaitMembersResponse {
                        completed,
                        members,
                        summary,
                        background_started,
                        ..
                    }) => {
                        if background_started {
                            return Ok(ToolOutput::new(format!(
                                "{}\n\n(You can keep working; this wait runs in the background.)",
                                summary
                            )));
                        }
                        let reports = fetch_awaited_member_reports(&ctx, &members).await;
                        Ok(format_awaited_members_with_reports(
                            completed, &summary, &members, &reports,
                        ))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("Await completed."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to await members: {}", e)),
                }
            }

            _ => Err(anyhow::anyhow!(
                "Unknown action '{}'. Valid actions: share, share_append, read, message, broadcast, dm, channel, list, list_channels, channel_members, \
                 propose_plan, approve_plan, reject_plan, spawn, stop, assign_role, status, report, plan_status, summary, read_context, \
                 resync_plan, assign_task, assign_next, fill_slots, run_plan, cleanup, start, start_task, wake, resume, retry, reassign, replace, salvage, subscribe_channel, unsubscribe_channel, await_members. \
                 To read messages addressed to you, use action='read'.",
                params.action
            )),
        }
    }
}

#[cfg(test)]
#[path = "communicate_tests.rs"]
mod tests;
