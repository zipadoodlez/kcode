use super::state_ui::RestoredReloadInput;
use super::*;
use crate::tui::{backend, keybind};

impl App {
    pub(super) fn apply_restored_reload_input(&mut self, restored: RestoredReloadInput) {
        self.composer.input = restored.input;
        self.composer.cursor_pos = restored.cursor;
        self.composer.pending_images = restored.pending_images;
        self.submit_input_on_startup = restored.submit_on_restore
            && (!self.composer.input.is_empty() || !self.composer.pending_images.is_empty());
        crate::logging::info(&format!(
            "Startup input restored: submit_on_restore={} input_chars={} pending_images={} queued_messages={} hidden_system={} => submit_input_on_startup={}",
            restored.submit_on_restore,
            self.composer.input.chars().count(),
            self.composer.pending_images.len(),
            restored.queued_messages.len(),
            restored.hidden_queued_system_messages.len(),
            self.submit_input_on_startup,
        ));
        self.hidden_queued_system_messages = restored.hidden_queued_system_messages;
        if let Some(status_notice) = restored.startup_status_notice {
            self.set_status_notice(status_notice);
        } else if self.submit_input_on_startup {
            self.set_status_notice("Startup prompt queued");
        }
        if let Some((title, message)) = restored.startup_display_message {
            self.push_display_message(DisplayMessage::system(message).with_title(title));
        }
        self.interleave_message = None;
        self.interleave_images.clear();
        self.rate_limit_pending_message = restored.rate_limit_pending_message;
        self.rate_limit_reset = restored.rate_limit_reset;
        self.observe.page_markdown = restored.observe_page_markdown;
        self.observe.page_updated_at_ms = restored.observe_page_updated_at_ms;
        self.set_observe_mode_enabled(restored.observe_mode_enabled, restored.observe_mode_enabled);
        self.set_split_view_enabled(restored.split_view_enabled, restored.split_view_enabled);
        self.set_todos_view_enabled(restored.todos_view_enabled, restored.todos_view_enabled);
        self.todo_confidence_spike_challenged = restored.todo_confidence_spike_challenged;
        self.last_todo_ownership_fingerprint = restored.last_todo_ownership_fingerprint;

        let mut queued_messages = restored.queued_messages;
        let mut recovered_followups = Vec::new();
        if let Some(interleave_message) = restored.interleave_message
            && !interleave_message.trim().is_empty()
        {
            recovered_followups.push(interleave_message);
        }
        let recovered_interrupts = restored
            .pending_soft_interrupt_resend
            .unwrap_or(restored.pending_soft_interrupts);
        if !recovered_interrupts.is_empty() {
            crate::logging::info(&format!(
                "Recovered {} pending soft interrupt(s) after reload; re-queueing them as normal follow-ups",
                recovered_interrupts.len()
            ));
            recovered_followups.extend(recovered_interrupts);
        }
        if !recovered_followups.is_empty() {
            let mut recovered_queue = recovered_followups;
            recovered_queue.append(&mut queued_messages);
            queued_messages = recovered_queue;
            self.set_status_notice("Recovered pending prompts after reload");
        }

        self.queued_messages = queued_messages;
        if self.has_queued_followups() {
            if self.is_remote_client() {
                // Do not synthesize a processing turn for restored remote follow-ups.
                // After a reload, the server may still be running the previous turn;
                // the queue must remain a wait-until-turn-end queue until the history
                // bootstrap/Done event proves the remote turn is idle. The remote
                // post-connect/history/tick paths will dispatch once it is safe.
                self.set_status_notice("Restored queued follow-up after reload");
            } else {
                self.is_processing = true;
                self.status = ProcessingStatus::Sending;
                if self.processing_started.is_none() {
                    self.processing_started = Some(Instant::now());
                }
                self.pending_turn = true;
            }
        }
    }

    /// Re-parse keybinding snapshots when the config cache has reloaded.
    ///
    /// The parsed bindings are cached on `App` for cheap per-keystroke lookup,
    /// so without this poll a config.toml keybinding edit would only take
    /// effect after a restart. Called from the idle tick in both local and
    /// remote run loops, and again immediately before dispatching a key press
    /// so an edit lands on the very next keystroke even when the run loop is
    /// sitting at the 5s deep-idle cadence. The generation check makes the
    /// no-change path a single atomic load. Returns true when bindings were
    /// re-parsed.
    pub(super) fn refresh_keybindings_if_config_reloaded(&mut self) -> bool {
        // config() performs the throttled file-fingerprint staleness check and
        // bumps the reload generation when config.toml changed on disk.
        crate::config::config();
        let generation = crate::config::config_reload_generation();
        if generation == self.keybindings_config_generation {
            return false;
        }
        self.keybindings_config_generation = generation;
        self.keybinds = keybind::Keybinds::load();
        crate::logging::info("KEYBINDINGS: reloaded from config change");
        // Confirm the pickup to the user. Without this, an edit that is
        // already live is indistinguishable from one that silently did
        // nothing, which is the main source of "did that actually apply?".
        self.set_status_notice("Config reloaded from disk");
        true
    }

    pub(super) async fn begin_remote_send(
        &mut self,
        remote: &mut backend::RemoteConnection,
        content: String,
        images: Vec<(String, String)>,
        is_system: bool,
    ) -> Result<u64> {
        remote::begin_remote_send(self, remote, content, images, is_system, None, false, 0).await
    }

    pub(super) fn schedule_pending_remote_retry(&mut self, reason: &str) -> bool {
        self.schedule_pending_remote_retry_with_limit(reason, Self::AUTO_RETRY_MAX_ATTEMPTS)
    }

    pub(super) fn schedule_pending_remote_network_wait(&mut self, reason: &str) -> bool {
        self.schedule_pending_remote_network_wait_with_force(reason, false)
    }

    /// Hold the in-flight remote turn until the network recovers, then resume it.
    ///
    /// Connectivity failures (DNS, connection reset, no route, transient TLS,
    /// timeouts) are always transient: the request never reached the provider,
    /// so resending after the network comes back is both safe and correct. When
    /// `force` is set we wait regardless of the pending message's `auto_retry`
    /// flag and promote it to auto-retry so the tick-based resume re-sends it.
    /// This prevents a transient disconnect from being misclassified as a
    /// permanent, non-retryable failure that stops auto-poke.
    pub(super) fn schedule_pending_remote_network_wait_with_force(
        &mut self,
        reason: &str,
        force: bool,
    ) -> bool {
        let Some(pending) = self.rate_limit_pending_message.as_mut() else {
            return false;
        };
        if !pending.auto_retry {
            if force {
                pending.auto_retry = true;
            } else {
                return false;
            }
        }

        let plan = crate::network_retry::wait_plan();
        let retry_at = Instant::now() + Duration::from_secs(5);
        pending.retry_at = Some(retry_at);
        self.rate_limit_reset = Some(retry_at);
        self.status = ProcessingStatus::WaitingForNetwork {
            listener: plan.listener_summary.clone(),
        };
        self.status_detail = Some("offline; waiting for network before retry".to_string());

        let content = format!(
            "📡 Network appears offline - waiting to retry automatically. {} - {}",
            plan.listener_summary,
            reason.trim().trim_end_matches('.')
        );
        if let Some(idx) = self.transcript.messages().iter().rposition(|message| {
            message.role == "system"
                && (message.title.as_deref() == Some("Connection")
                    || message.content.starts_with("📡 Network appears offline"))
        }) {
            self.replace_display_message_title_and_content(
                idx,
                Some("Connection".to_string()),
                content,
            );
        } else {
            self.push_display_message(DisplayMessage {
                role: "system".to_string(),
                content,
                tool_calls: Vec::new(),
                duration_secs: None,
                title: Some("Connection".to_string()),
                tool_data: None,
            });
        }
        true
    }

    pub(super) fn schedule_pending_remote_retry_with_limit(
        &mut self,
        reason: &str,
        max_attempts: u8,
    ) -> bool {
        let Some(pending) = self.rate_limit_pending_message.as_mut() else {
            return false;
        };
        if !pending.auto_retry {
            return false;
        }
        let outcome = {
            let current_attempts = pending.retry_attempts;
            if current_attempts >= max_attempts {
                Err(current_attempts)
            } else {
                pending.retry_attempts += 1;
                let retry_attempts = pending.retry_attempts;
                let backoff_secs = Self::AUTO_RETRY_BASE_DELAY_SECS * u64::from(retry_attempts);
                let retry_at = Instant::now() + Duration::from_secs(backoff_secs);
                pending.retry_at = Some(retry_at);
                Ok((retry_attempts, backoff_secs, retry_at))
            }
        };

        match outcome {
            Err(current_attempts) => {
                self.rate_limit_pending_message = None;
                self.rate_limit_reset = None;
                self.push_display_message(DisplayMessage::error(format!(
                    "{} Auto-retry limit reached after {} attempt{}. Use `/poke` again to retry manually.",
                    reason,
                    current_attempts,
                    if current_attempts == 1 { "" } else { "s" }
                )));
                false
            }
            Ok((retry_attempts, backoff_secs, retry_at)) => {
                self.rate_limit_reset = Some(retry_at);
                let content = format!(
                    "⚡ Connection lost - retrying (attempt {}/{}, in {}s) - {}",
                    retry_attempts,
                    max_attempts,
                    backoff_secs,
                    reason
                        .trim()
                        .trim_start_matches("⚡ ")
                        .trim_start_matches("Connection lost")
                        .trim_start_matches('(')
                        .trim_end_matches('.')
                        .trim()
                );
                if let Some(idx) = self.transcript.messages().iter().rposition(|message| {
                    message.role == "system"
                        && (message.title.as_deref() == Some("Connection")
                            || message
                                .content
                                .starts_with("⚡ Server reload in progress - waiting for handoff")
                            || message.content.starts_with("⚡ Connection lost"))
                }) {
                    self.replace_display_message_title_and_content(
                        idx,
                        Some("Connection".to_string()),
                        content,
                    );
                } else {
                    self.push_display_message(DisplayMessage {
                        role: "system".to_string(),
                        content,
                        tool_calls: Vec::new(),
                        duration_secs: None,
                        title: Some("Connection".to_string()),
                        tool_data: None,
                    });
                }
                true
            }
        }
    }

    pub(super) fn clear_pending_remote_retry(&mut self) {
        self.rate_limit_pending_message = None;
        self.rate_limit_reset = None;
    }

    /// Track a failed turn for the credential-failure circuit breaker.
    ///
    /// Returns `true` when the error classifies as a credential/auth failure
    /// AND the consecutive-failure count has reached the breaker threshold,
    /// meaning the caller must stop all automatic resend paths. Non-credential
    /// errors reset the streak (the breaker only guards against retrying a
    /// dead credential, not mixed transient failures).
    pub(super) fn note_error_for_credential_breaker(&mut self, message: &str) -> bool {
        if crate::provider::error_looks_like_credential_failure(message) {
            self.consecutive_credential_failures =
                self.consecutive_credential_failures.saturating_add(1);
            self.consecutive_credential_failures >= Self::CREDENTIAL_FAILURE_BREAKER_THRESHOLD
        } else {
            self.consecutive_credential_failures = 0;
            false
        }
    }

    /// Reset the credential-failure streak. Called when a turn completes
    /// successfully or the user changes auth (login, provider/model switch),
    /// so a fixed credential gets a fresh retry budget.
    pub(super) fn reset_credential_failure_breaker(&mut self) {
        self.consecutive_credential_failures = 0;
    }

    /// Hard-stop every automatic resend path because the session has hit
    /// repeated credential/auth failures. Retrying the identical request
    /// against a dead credential can never succeed; before this breaker,
    /// auto-poke/queued-retry loops logged thousands of 401s in a single
    /// session (one failed turn per resend) until the user noticed.
    pub(super) fn trip_credential_failure_breaker(&mut self) {
        let failures = self.consecutive_credential_failures;
        self.clear_pending_remote_retry();
        let cleared_pokes = if self.auto_poke_incomplete_todos {
            super::commands::disable_auto_poke(self)
        } else {
            0
        };
        self.overnight_auto_poke = None;

        // Report the streak explicitly so "breaker tripped on a dead
        // credential" is distinguishable from a transient blip.
        let provider = self.provider_name().to_string();

        self.push_display_message(DisplayMessage::error(format!(
            "🛑 Stopped automatic retries: {failures} consecutive credential/auth failures. \
             The current login or API key for {provider} is not working, so resending the same \
             request cannot succeed.{} Run /login to re-authenticate (or /model to switch to a \
             working route), then send again.",
            if cleared_pokes == 0 {
                String::new()
            } else {
                format!(" Cleared {cleared_pokes} queued auto-poke follow-up(s).")
            }
        )));
        self.set_status_notice("Stopped: repeated auth failures");
        self.restore_failed_input_to_box();
        self.consecutive_credential_failures = 0;
    }

    pub(super) fn new_minimal_with_session(
        provider: Arc<dyn Provider>,
        registry: Registry,
        mut session: Session,
    ) -> Self {
        let skills = Arc::new(SkillRegistry::default());
        let mcp_manager = Arc::new(RwLock::new(McpManager::new()));
        if session.model.is_none() {
            session.model = Some(provider.model());
        }
        if session.provider_key.is_none() {
            session.provider_key = crate::session::derive_session_provider_key(provider.name());
        }
        let display = config().display.clone();
        let features = config().features.clone();
        let autoreview_enabled = session
            .autoreview_enabled
            .unwrap_or(config().autoreview.enabled);
        let autojudge_enabled = session
            .autojudge_enabled
            .unwrap_or(config().autojudge.enabled);
        let context_limit = provider.context_window() as u64;
        let mut runtime_memory_log = if crate::runtime_memory_log::client_logging_enabled() {
            Some(crate::runtime_memory_log::RuntimeMemoryLogController::new(
                crate::runtime_memory_log::client_logging_config(),
            ))
        } else {
            None
        };
        if let Some(controller) = runtime_memory_log.as_mut() {
            controller.defer_event(
                crate::runtime_memory_log::RuntimeMemoryLogEvent::new("startup", "client_started")
                    .with_session_id(session.id.clone())
                    .force_attribution(),
            );
        }
        let improve_mode = session.improve_mode.map(|mode| match mode {
            crate::session::SessionImproveMode::ImproveRun => ImproveMode::ImproveRun,
            crate::session::SessionImproveMode::ImprovePlan => ImproveMode::ImprovePlan,
            crate::session::SessionImproveMode::RefactorRun => ImproveMode::RefactorRun,
            crate::session::SessionImproveMode::RefactorPlan => ImproveMode::RefactorPlan,
        });

        crate::logging::info("App::new_minimal_with_session: skipping skill/prompt bootstrap");

        let mut app = Self {
            provider,
            registry,
            skills,
            mcp_manager,
            messages: Vec::new(),
            session,
            transcript: Default::default(),
            compacted_history_lazy: CompactedHistoryLazyState::default(),
            composer: Default::default(),
            command_suggestions: Default::default(),
            viewport: Default::default(),
            active_skill: None,
            is_processing: false,
            streaming: StreamingProgress::default(),
            power_inhibitor: crate::power_inhibit::PowerInhibitor::new(),
            should_quit: false,
            queued_messages: Vec::new(),
            hidden_queued_system_messages: Vec::new(),
            current_turn_system_reminder: None,
            upstream_provider: None,
            connection_type: None,
            status_detail: None,
            token_accounting: TokenAccounting::default(),
            kv_cache: KvCacheState::default(),
            cost: CostState::default(),
            context_limit,
            context_warning_shown: false,
            context_info: crate::prompt::ContextInfo::default(),
            context_revision: 0,
            last_stream_activity: None,
            last_user_interaction: None,
            stream_message_ended: false,
            deferred_stream_done_id: None,
            remote_resume_activity: None,
            queued_followup_starved_since: None,
            status: ProcessingStatus::default(),
            subagent_status: None,
            batch_progress: None,
            processing_started: None,
            visible_turn_started: None,
            last_api_response: Default::default(),
            pending_turn: false,
            auto_poke_incomplete_todos: features.auto_poke,
            auto_poke_default_on: features.auto_poke,
            todo_confidence_spike_challenged: false,
            todo_gate_digest_delivered: false,
            todo_completion_gate_attempts: 0,
            last_todo_ownership_fingerprint: None,
            todo_final_response_requested: false,
            last_auto_poke_fingerprint: None,
            turn_guardrail_stopped: false,
            consecutive_guardrail_stops: 0,
            overnight_auto_poke: None,
            pending_provider_failover: None,
            pending_fallback_offer: None,
            pending_fallback_resend: None,
            pending_merge_offer: None,
            session_save_pending: false,
            streaming_tool_calls: Vec::new(),
            attempt_committed_assistant_messages: 0,
            provider_session_id: None,
            rewind_undo_snapshot: None,
            cancel_requested: false,
            quit_pending: None,
            redraw: Default::default(),
            mcp_server_names: Vec::new(),
            connection_phase_started: None,
            stream_buffer: StreamBuffer::new(),
            reasoning: Default::default(),
            maintenance: Default::default(),
            route_next_prompt_to_new_session: false,
            submit_input_on_startup: false,
            startup_submit_deferred_reason: None,
            onboarding_preview_mode: false,
            onboarding_sim: None,
            update_sim: None,
            onboarding_flow: None,
            onboarding_auto_model_selection_active: Arc::new(AtomicBool::new(false)),
            onboarding_auto_model_selection_baseline: Arc::new(std::sync::Mutex::new(None)),
            onboarding_startup_checked: false,
            onboarding_import_in_progress: None,
            onboarding_import_error: None,
            onboarding_import_failed_provider: None,
            onboarding_pending_model_validation: None,
            onboarding_recent_project_prefetch: None,
            copy_badge_ui: CopyBadgeUiState::default(),
            copy_selection: Default::default(),
            debug_tx: None,
            remote_client_instance_id: crate::id::new_id("client"),
            remote_provider_name: None,
            remote_provider_model: None,
            remote_model_catalog_generation: 0,
            remote_resolved_credential: None,
            remote_startup: Default::default(),
            remote_reasoning_effort: None,
            remote_service_tier: None,
            remote_transport: None,
            remote_compaction_mode: None,
            remote_available_entries: Vec::new(),
            remote_model_options: Vec::new(),
            pending_remote_model_refresh_snapshot: None,
            remote_mcp_servers: Vec::new(),
            remote_skills: Vec::new(),
            remote_total_tokens: None,
            remote_token_usage_totals: None,
            server_info: Default::default(),
            current_message_id: None,
            runtime_mode: AppRuntimeMode::TestHarness,
            pending_remote_rewind_notice: None,
            history_recovery: Default::default(),
            server_spawning: false,
            suppress_terminal_title_updates: false,
            replay_elapsed_override: None,
            replay_processing_started_ms: None,
            tool_call_ids: HashSet::new(),
            tool_result_ids: HashSet::new(),
            tool_output_scan_index: 0,
            remote_session_id: None,
            resume_session_id: None,
            requested_exit_code: None,
            autoreview_enabled,
            autojudge_enabled,
            improve_mode,
            swarm_enabled: features.swarm,
            debug_force_inline_gallery: false,
            swarm: Default::default(),
            diff_mode: display.diff_mode,
            centered: display.centered,
            side_pane_ratio: 40,
            side_pane_ratio_user_adjusted: false,
            side_pane_dragging: false,
            diff_pane_scroll: 0,
            diff_pane_scroll_x: 0,
            diff_pane_focus: false,
            diff_pane_auto_scroll: true,
            side_panel: crate::side_panel::SidePanelSnapshot::default(),
            observe: Default::default(),
            split_view: Default::default(),
            todos_view: Default::default(),
            background_tasks: Default::default(),
            last_side_panel_refresh: None,
            last_side_panel_focus_id: None,
            side_panel_user_hidden: false,
            side_panel_explicit_hidden: false,
            chat_native_scrollbar: display.native_scrollbars.chat,
            side_panel_native_scrollbar: display.native_scrollbars.side_panel,
            inline_view_state: None,
            inline_interactive_state: None,
            model_picker: Default::default(),
            recent_authenticated_provider: None,
            auth_catalog_refresh_pending: false,
            pending_model_switch: None,
            pending_route_selection: None,
            pending_reasoning_effort: None,
            remote_model_switch_in_flight: false,
            pending_prompt_after_model_switch: None,
            pending_prompt_before_history: None,
            pending_startup_prompt_echo: None,
            keybinds: keybind::Keybinds::load(),
            keybindings_config_generation: crate::config::config_reload_generation(),
            status_notice: None,
            learn_hint: None,
            learn_hint_shown_this_session: false,
            terminal_setup_hint_shown_this_session: false,
            swarm_hint_shown_this_session: false,
            hotkey_feedback_state: Default::default(),
            experimental_feature_warnings_seen: HashSet::new(),
            active_experimental_feature_notice: None,
            interleave_message: None,
            interleave_images: Vec::new(),
            pending_soft_interrupts: Vec::new(),
            pending_soft_interrupt_requests: Vec::new(),
            autoreview_after_current_turn: false,
            autojudge_after_current_turn: false,
            pending_split: Default::default(),
            pending_local_transfer: None,
            queue_mode: display.queue_mode,
            auto_server_reload: display.auto_server_reload,
            pending_queued_dispatch: false,
            app_started: Instant::now(),
            client_focused: true,
            runtime_memory_log,
            idle_heap_release: Default::default(),
            rate_limit_reset: None,
            rate_limit_pending_message: None,
            consecutive_credential_failures: 0,
            last_stream_error: None,
            last_submitted_input: None,
            reload_info: Vec::new(),
            debug_trace: DebugTrace::new(),
            streaming_md_renderer: RefCell::new(IncrementalMarkdownRenderer::new(None)),
            pending_login: None,
            remote_login: None,
            remote_login_onboarding: Default::default(),
            pending_ssh_remote_name: None,
            last_wheel: None,
            overscroll_status_mode: display.overscroll_status,
            changelog_scroll: None,
            help_scroll: None,
            model_status_scroll: None,
            model_status_content: String::new(),
            session_picker: Default::default(),
            catchup: Default::default(),
            login_picker_overlay: None,
            account_picker: Default::default(),
            usage: Default::default(),
            overnight_card: Default::default(),
            workspace_client: crate::tui::workspace_client::WorkspaceClientState::default(),
            prompt_history: Default::default(),
        };

        for notice in app.provider.drain_startup_notices() {
            app.status_notice = Some((notice, Instant::now()));
        }

        app
    }

    pub fn new(provider: Arc<dyn Provider>, registry: Registry) -> Self {
        let t0 = std::time::Instant::now();
        let skills = SkillRegistry::shared_snapshot();
        let t_skills = t0.elapsed();
        let mcp_manager = Arc::new(RwLock::new(McpManager::new()));
        let mut session = Session::create(None, None);
        session.mark_active();
        session.model = Some(provider.model());
        session.provider_key = crate::session::derive_session_provider_key(provider.name());
        session.ensure_initial_session_context_message();
        let display = config().display.clone();
        let features = config().features.clone();
        let autoreview_enabled = session
            .autoreview_enabled
            .unwrap_or(config().autoreview.enabled);
        let autojudge_enabled = session
            .autojudge_enabled
            .unwrap_or(config().autojudge.enabled);
        let context_limit = provider.context_window() as u64;
        let mut runtime_memory_log = if crate::runtime_memory_log::client_logging_enabled() {
            Some(crate::runtime_memory_log::RuntimeMemoryLogController::new(
                crate::runtime_memory_log::client_logging_config(),
            ))
        } else {
            None
        };
        if let Some(controller) = runtime_memory_log.as_mut() {
            controller.defer_event(
                crate::runtime_memory_log::RuntimeMemoryLogEvent::new("startup", "client_started")
                    .with_session_id(session.id.clone())
                    .force_attribution(),
            );
        }
        let improve_mode = session.improve_mode.map(|mode| match mode {
            crate::session::SessionImproveMode::ImproveRun => ImproveMode::ImproveRun,
            crate::session::SessionImproveMode::ImprovePlan => ImproveMode::ImprovePlan,
            crate::session::SessionImproveMode::RefactorRun => ImproveMode::RefactorRun,
            crate::session::SessionImproveMode::RefactorPlan => ImproveMode::RefactorPlan,
        });
        let t_session = t0.elapsed();

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let provider_clone = Arc::clone(&provider);
            handle.spawn(async move {
                let _ = provider_clone.prefetch_models().await;
            });
        }

        // Pre-compute context info so it shows on startup
        let available_skills: Vec<crate::prompt::SkillInfo> = skills
            .list()
            .iter()
            .map(|s| crate::prompt::SkillInfo {
                name: s.name.clone(),
                description: s.description.clone(),
            })
            .collect();
        let (_, context_info) = crate::prompt::build_system_prompt_with_context(
            None,
            &available_skills,
            session.is_canary,
        );
        let t_prompt = t0.elapsed();
        crate::logging::info(&format!(
            "App::new timings: skills={:.1}ms session={:.1}ms prompt={:.1}ms total={:.1}ms",
            t_skills.as_secs_f64() * 1000.0,
            (t_session - t_skills).as_secs_f64() * 1000.0,
            (t_prompt - t_session).as_secs_f64() * 1000.0,
            t_prompt.as_secs_f64() * 1000.0,
        ));

        let mut app = Self {
            provider,
            registry,
            skills,
            mcp_manager,
            messages: Vec::new(),
            session,
            transcript: Default::default(),
            compacted_history_lazy: CompactedHistoryLazyState::default(),
            composer: Default::default(),
            command_suggestions: Default::default(),
            viewport: Default::default(),
            active_skill: None,
            is_processing: false,
            streaming: StreamingProgress::default(),
            power_inhibitor: crate::power_inhibit::PowerInhibitor::new(),
            should_quit: false,
            queued_messages: Vec::new(),
            hidden_queued_system_messages: Vec::new(),
            current_turn_system_reminder: None,
            upstream_provider: None,
            connection_type: None,
            status_detail: None,
            token_accounting: TokenAccounting::default(),
            kv_cache: KvCacheState::default(),
            cost: CostState::default(),
            context_limit,
            context_warning_shown: false,
            context_info,
            context_revision: 0,
            last_stream_activity: None,
            last_user_interaction: None,
            stream_message_ended: false,
            deferred_stream_done_id: None,
            remote_resume_activity: None,
            queued_followup_starved_since: None,
            status: ProcessingStatus::default(),
            subagent_status: None,
            batch_progress: None,
            processing_started: None,
            visible_turn_started: None,
            last_api_response: Default::default(),
            pending_turn: false,
            auto_poke_incomplete_todos: features.auto_poke,
            auto_poke_default_on: features.auto_poke,
            todo_confidence_spike_challenged: false,
            todo_gate_digest_delivered: false,
            todo_completion_gate_attempts: 0,
            last_todo_ownership_fingerprint: None,
            todo_final_response_requested: false,
            last_auto_poke_fingerprint: None,
            turn_guardrail_stopped: false,
            consecutive_guardrail_stops: 0,
            overnight_auto_poke: None,
            pending_provider_failover: None,
            pending_fallback_offer: None,
            pending_fallback_resend: None,
            pending_merge_offer: None,
            session_save_pending: false,
            streaming_tool_calls: Vec::new(),
            attempt_committed_assistant_messages: 0,
            provider_session_id: None,
            rewind_undo_snapshot: None,
            cancel_requested: false,
            quit_pending: None,
            redraw: Default::default(),
            mcp_server_names: Vec::new(), // Vec<(name, tool_count)>
            connection_phase_started: None,
            stream_buffer: StreamBuffer::new(),
            reasoning: Default::default(),
            maintenance: Default::default(),
            route_next_prompt_to_new_session: false,
            submit_input_on_startup: false,
            startup_submit_deferred_reason: None,
            onboarding_preview_mode: false,
            onboarding_sim: None,
            update_sim: None,
            onboarding_flow: None,
            onboarding_auto_model_selection_active: Arc::new(AtomicBool::new(false)),
            onboarding_auto_model_selection_baseline: Arc::new(std::sync::Mutex::new(None)),
            onboarding_startup_checked: false,
            onboarding_import_in_progress: None,
            onboarding_import_error: None,
            onboarding_import_failed_provider: None,
            onboarding_pending_model_validation: None,
            onboarding_recent_project_prefetch: None,
            copy_badge_ui: CopyBadgeUiState::default(),
            copy_selection: Default::default(),
            debug_tx: None,
            remote_client_instance_id: crate::id::new_id("client"),
            remote_provider_name: None,
            remote_provider_model: None,
            remote_model_catalog_generation: 0,
            remote_resolved_credential: None,
            remote_startup: Default::default(),
            remote_reasoning_effort: None,
            remote_service_tier: None,
            remote_transport: None,
            remote_compaction_mode: None,
            remote_available_entries: Vec::new(),
            remote_model_options: Vec::new(),
            pending_remote_model_refresh_snapshot: None,
            remote_mcp_servers: Vec::new(),
            remote_skills: Vec::new(),
            remote_total_tokens: None,
            remote_token_usage_totals: None,
            server_info: Default::default(),
            current_message_id: None,
            runtime_mode: AppRuntimeMode::TestHarness,
            pending_remote_rewind_notice: None,
            history_recovery: Default::default(),
            server_spawning: false,
            suppress_terminal_title_updates: false,
            replay_elapsed_override: None,
            replay_processing_started_ms: None,
            tool_call_ids: HashSet::new(),
            tool_result_ids: HashSet::new(),
            tool_output_scan_index: 0,
            remote_session_id: None,
            resume_session_id: None,
            requested_exit_code: None,
            autoreview_enabled,
            autojudge_enabled,
            improve_mode,
            swarm_enabled: features.swarm,
            debug_force_inline_gallery: false,
            swarm: Default::default(),
            diff_mode: display.diff_mode,
            centered: display.centered,
            side_pane_ratio: 40,
            side_pane_ratio_user_adjusted: false,
            side_pane_dragging: false,
            diff_pane_scroll: 0,
            diff_pane_scroll_x: 0,
            diff_pane_focus: false,
            diff_pane_auto_scroll: true,
            side_panel: crate::side_panel::SidePanelSnapshot::default(),
            observe: Default::default(),
            split_view: Default::default(),
            todos_view: Default::default(),
            background_tasks: Default::default(),
            last_side_panel_refresh: None,
            last_side_panel_focus_id: None,
            side_panel_user_hidden: false,
            side_panel_explicit_hidden: false,
            chat_native_scrollbar: display.native_scrollbars.chat,
            side_panel_native_scrollbar: display.native_scrollbars.side_panel,
            inline_view_state: None,
            inline_interactive_state: None,
            model_picker: Default::default(),
            recent_authenticated_provider: None,
            auth_catalog_refresh_pending: false,
            pending_model_switch: None,
            pending_route_selection: None,
            pending_reasoning_effort: None,
            remote_model_switch_in_flight: false,
            pending_prompt_after_model_switch: None,
            pending_prompt_before_history: None,
            pending_startup_prompt_echo: None,
            keybinds: keybind::Keybinds::load(),
            keybindings_config_generation: crate::config::config_reload_generation(),
            status_notice: None,
            learn_hint: None,
            learn_hint_shown_this_session: false,
            terminal_setup_hint_shown_this_session: false,
            swarm_hint_shown_this_session: false,

            hotkey_feedback_state: Default::default(),
            experimental_feature_warnings_seen: HashSet::new(),
            active_experimental_feature_notice: None,
            interleave_message: None,
            interleave_images: Vec::new(),
            pending_soft_interrupts: Vec::new(),
            pending_soft_interrupt_requests: Vec::new(),
            autoreview_after_current_turn: false,
            autojudge_after_current_turn: false,
            pending_split: Default::default(),
            pending_local_transfer: None,
            queue_mode: display.queue_mode,
            auto_server_reload: display.auto_server_reload,
            pending_queued_dispatch: false,
            app_started: Instant::now(),
            client_focused: true,
            runtime_memory_log,
            idle_heap_release: Default::default(),
            rate_limit_reset: None,
            rate_limit_pending_message: None,
            consecutive_credential_failures: 0,
            last_stream_error: None,
            last_submitted_input: None,
            reload_info: Vec::new(),
            debug_trace: DebugTrace::new(),
            streaming_md_renderer: RefCell::new(IncrementalMarkdownRenderer::new(None)),
            pending_login: None,
            remote_login: None,
            remote_login_onboarding: Default::default(),
            pending_ssh_remote_name: None,
            last_wheel: None,
            overscroll_status_mode: display.overscroll_status,
            changelog_scroll: None,
            help_scroll: None,
            model_status_scroll: None,
            model_status_content: String::new(),
            session_picker: Default::default(),
            catchup: Default::default(),
            login_picker_overlay: None,
            account_picker: Default::default(),
            usage: Default::default(),
            overnight_card: Default::default(),
            workspace_client: crate::tui::workspace_client::WorkspaceClientState::default(),
            prompt_history: Default::default(),
        };

        for notice in app.provider.drain_startup_notices() {
            app.status_notice = Some((notice, Instant::now()));
        }

        app
    }

    pub fn new_for_test_harness(provider: Arc<dyn Provider>, registry: Registry) -> Self {
        let mut app = Self::new(provider, registry);
        app.set_runtime_mode(AppRuntimeMode::TestHarness);
        app
    }

    /// Queue a startup message that should be auto-sent when the TUI starts.
    pub fn queue_startup_message(&mut self, message: String) {
        if message.trim().is_empty() {
            return;
        }
        self.queued_messages.push(message);
        self.is_processing = true;
        self.status = ProcessingStatus::Sending;
        self.processing_started = Some(Instant::now());
        self.pending_turn = true;
    }

    fn restore_remote_startup_history(&mut self, session_id: &str) {
        let load_start = Instant::now();
        let Ok(mut session) = Session::load_for_remote_startup(session_id) else {
            return;
        };

        let render_start = Instant::now();
        // Narrow scope so render intermediates (rendered messages, display
        // message buffers) drop before we strip and retain the session.
        {
            let (rendered_messages, _rendered_images) =
                crate::session::render_messages_and_images(&session);
            let display_messages =
                jcode_tui_messages::display_messages_from_rendered_messages(rendered_messages);
            self.replace_display_messages(display_messages);
        }
        let render_ms = render_start.elapsed().as_millis();

        let image_ms = 0;
        self.set_side_panel_snapshot(
            crate::side_panel::snapshot_for_session(session_id).unwrap_or_default(),
        );
        self.remote_session_id = Some(session_id.to_string());
        session.strip_transcript_for_remote_client();
        // Strip clears transcript vectors but keeps capacity; free buffers.
        session.messages.shrink_to_fit();
        session.env_snapshots.shrink_to_fit();
        session.memory_injections.shrink_to_fit();
        session.replay_events.shrink_to_fit();
        self.session = session;
        // The full deserialized transcript (raw file + Session structs) was a
        // large transient; return the freed arena pages to the OS now instead
        // of waiting for the post-connect client_history_loaded release.
        crate::process_memory::release_retained_heap("remote_startup_history_stripped");
        self.autoreview_enabled = self
            .session
            .autoreview_enabled
            .unwrap_or(crate::config::config().autoreview.enabled);
        self.autojudge_enabled = self
            .session
            .autojudge_enabled
            .unwrap_or(crate::config::config().autojudge.enabled);
        if let Some(model) = self.session.model.clone() {
            self.update_context_limit_for_model(&model);
        }
        self.viewport.follow_chat_bottom();
        crate::logging::info(&format!(
            "Remote startup fast restore: session={}, display_messages={}, load={}ms, render={}ms, total={}ms",
            session_id,
            self.transcript.messages().len(),
            load_start
                .elapsed()
                .as_millis()
                .saturating_sub(render_ms + image_ms),
            render_ms,
            load_start.elapsed().as_millis()
        ));
    }

    /// Create an App instance for remote mode (connecting to server)
    pub fn new_for_remote(resume_session: Option<String>) -> Self {
        Self::new_for_remote_with_options(resume_session, false)
    }

    pub fn new_for_remote_with_options(resume_session: Option<String>, fresh_spawn: bool) -> Self {
        let provider: Arc<dyn Provider> =
            Arc::new(InertRuntimeProvider::new(AppRuntimeMode::RemoteClient));
        let registry = Registry::empty();
        let session = resume_session
            .as_ref()
            .filter(|_| !crate::tui::is_ssh_remote())
            .and_then(|session_id| Session::load_startup_stub(session_id).ok())
            .unwrap_or_else(|| Session::create(None, None));
        let mut app = Self::new_minimal_with_session(provider, registry, session);
        app.set_runtime_mode(AppRuntimeMode::RemoteClient);
        app.remote_startup
            .set(super::RemoteStartupPhase::Connecting);

        if let Some(host) = crate::tui::ssh_remote_host() {
            // The server supplies history, credentials, models and project state.
            // Local reload files can belong to an unrelated session with the same id.
            app.onboarding_startup_checked = true;
            app.auto_server_reload = false;
            app.session.working_dir = None;
            app.resume_session_id = resume_session;
            app.set_status_notice(format!("SSH: {host} (remote server)"));
            return app;
        }

        // Minimal local clients start with empty skill registries. Load global
        // metadata once so autocomplete works before the first History event.
        // SSH clients above must use only the remote server's skill metadata.
        app.refresh_skills_snapshot();

        let reload_fast_start = std::env::var("JCODE_RELOAD_FAST_START")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        // One-shot handoff flag. A later ordinary resume in the same process
        // must retain the existing eager local-history behavior.
        crate::env::remove_var("JCODE_RELOAD_FAST_START");

        // Load session to get canary status (for "client self-dev" badge)
        if let Some(ref session_id) = resume_session {
            if reload_fast_start {
                crate::logging::info(&format!(
                    "Remote reload fast start: deferring persisted transcript for {} until server history",
                    session_id
                ));
            } else {
                app.restore_remote_startup_history(session_id);
            }
            if fresh_spawn && !reload_fast_start {
                crate::logging::info(&format!(
                    "Remote startup fresh-spawn path: restored persisted transcript for {} while awaiting server history",
                    session_id
                ));
            }
            if let Some(restored) = Self::restore_input_for_reload(session_id) {
                app.apply_restored_reload_input(restored);
            }
        }

        app.resume_session_id = resume_session;
        app
    }

    /// Mark that a server was just spawned - run_remote will retry initial connection
    /// instead of failing fatally, allowing the TUI to show while the server starts.
    pub fn set_server_spawning(&mut self) {
        self.server_spawning = true;
        self.remote_startup
            .set(super::RemoteStartupPhase::StartingServer);
    }
}
