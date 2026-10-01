use super::*;

/// App-side command-suggestion state: the candidate and suggestion memo caches,
/// the epoch bounding the suggestion cache's lifetime, and the selected row.
/// One home; the completion behavior stays on `App`.
#[derive(Default)]
pub(super) struct CommandSuggestions {
    pub(super) candidates_cache: std::cell::RefCell<Option<CommandCandidatesCache>>,
    /// Per-input memo for `command_suggestions()`; see `CommandSuggestionsCache`.
    pub(super) cache: std::cell::RefCell<Option<CommandSuggestionsCache>>,
    /// Monotonic frame counter bounding the lifetime of `cache` to a single frame.
    pub(super) epoch: std::cell::Cell<u64>,
    /// Selected row in the visible command suggestion list.
    pub(super) selected: usize,
}

impl CommandSuggestions {
    pub(super) fn invalidate_candidates_cache(&self) {
        *self.candidates_cache.borrow_mut() = None;
    }

    /// Advance the suggestion memo epoch, invalidating it. Called once per
    /// rendered frame so the memo only ever collapses reads *within* a frame
    /// and never serves data that predates a state change.
    pub(crate) fn advance_epoch(&self) {
        self.epoch.set(self.epoch.get().wrapping_add(1));
    }
}

#[derive(Clone, Copy)]
struct RegisteredCommand {
    name: &'static str,
    help: &'static str,
    hidden: bool,
}

impl RegisteredCommand {
    const fn public(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            hidden: false,
        }
    }

    const fn remote(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            hidden: false,
        }
    }

    const fn hidden(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            hidden: true,
        }
    }
}

const REGISTERED_COMMANDS: &[RegisteredCommand] = &[
    RegisteredCommand::public("/help", "Show help and keyboard shortcuts"),
    RegisteredCommand::public("/?", "Show help and keyboard shortcuts"),
    RegisteredCommand::public("/commands", "Alias for /help"),
    RegisteredCommand::public("/model", "List or switch models"),
    RegisteredCommand::public("/models", "Alias for /model"),
    RegisteredCommand::public(
        "/provider-test-coverage",
        "Show live-test evidence for the current provider/model",
    ),
    RegisteredCommand::hidden("/model-status", "Alias for /provider-test-coverage"),
    RegisteredCommand::public("/refresh-model-list", "Refresh provider model catalogs"),
    RegisteredCommand::public("/agents", "Configure models for agent roles"),
    RegisteredCommand::public(
        "/swarm-prompt",
        "Open the active swarm routing prompt in your editor",
    ),
    RegisteredCommand::public("/subagent", "Launch a subagent manually"),
    RegisteredCommand::public("/observe", "Show the latest tool context in the side panel"),
    RegisteredCommand::public("/todos", "Show the session todo list as a card in the chat"),
    RegisteredCommand::hidden("/todo", "Alias for /todos"),
    RegisteredCommand::public("/splitview", "Mirror the current chat in the side panel"),
    RegisteredCommand::public("/split-view", "Alias for /splitview"),
    RegisteredCommand::public("/btw", "Ask a side question in the side panel"),
    RegisteredCommand::public("/ssh", "Connect to a remote machine using system SSH"),
    RegisteredCommand::public("/git", "Show git status for the session working directory"),
    RegisteredCommand::public("/colors", "List, configure, and score every TUI color"),
    RegisteredCommand::hidden("/color", "Alias for /colors"),
    RegisteredCommand::public("/hotkeys", "List hotkeys with your personal usage"),
    RegisteredCommand::public("/terminal-setup", "Fix Shift+Enter newlines"),
    RegisteredCommand::public("/commit", "Make logical commits from current changes"),
    RegisteredCommand::public(
        "/commit-push",
        "Make logical commits from current changes, then push",
    ),
    RegisteredCommand::hidden("/commit-and-push", "Alias for /commit-push"),
    RegisteredCommand::public(
        "/fast-release",
        "Publish Linux immediately from the warm selfdev cache; CI adds other platforms",
    ),
    RegisteredCommand::public(
        "/fast-macos-release",
        "Publish a prepared macOS arm64 build immediately; CI adds other platforms",
    ),
    RegisteredCommand::public(
        "/remote-release",
        "Push the release tag immediately; CI builds and publishes every platform",
    ),
    RegisteredCommand::hidden("/cut-release", "Alias for /fast-release"),
    RegisteredCommand::hidden("/commit-push-release", "Alias for /cut-release"),
    RegisteredCommand::public(
        "/triage",
        "Triage new GitHub issues and autonomously fix the safe ones",
    ),
    RegisteredCommand::public("/transcript", "Open the current session transcript file"),
    RegisteredCommand::public("/subagent-model", "Show/change subagent model policy"),
    RegisteredCommand::public("/autoreview", "Show/toggle automatic end-of-turn review"),
    RegisteredCommand::public("/autojudge", "Show/toggle automatic end-of-turn judging"),
    RegisteredCommand::public("/review", "Launch a one-shot headed review session"),
    RegisteredCommand::public("/judge", "Launch a one-shot headed judge session"),
    RegisteredCommand::public("/effort", crate::tui::keybind::EFFORT_HELP),
    RegisteredCommand::public("/fast", "Toggle fast mode"),
    RegisteredCommand::public("/transport", "Show/change connection transport"),
    RegisteredCommand::public("/alignment", "Show/change default text alignment"),
    RegisteredCommand::public(
        "/compact-notifications",
        "Show/toggle single-line swarm/file-activity notifications",
    ),
    RegisteredCommand::public(
        "/show-kgrep-output",
        "Show/toggle full kgrep search output inline in chat",
    ),
    RegisteredCommand::public(
        "/tool-call-details",
        "Show/toggle dimmed technical details on tool rows with an intent",
    ),
    RegisteredCommand::public(
        "/thinking-display",
        "Show/hide the model's thinking text (off/full/current)",
    ),
    RegisteredCommand::hidden("/thinking", "Alias for /thinking-display"),
    RegisteredCommand::hidden("/reasoning", "Alias for /thinking-display"),
    RegisteredCommand::public("/cancel", "Cancel the current prompt or operation"),
    RegisteredCommand::public("/clear", "Clear conversation history"),
    RegisteredCommand::public("/cls", "Clear the view only, keeping context"),
    RegisteredCommand::hidden("/clear-view", "Alias for /cls"),
    RegisteredCommand::public("/rewind", "Rewind conversation to previous message"),
    RegisteredCommand::public("/plan", "Create a plan-only response as a plan card"),
    RegisteredCommand::public("/improve", "Autonomously improve the repository"),
    RegisteredCommand::public("/refactor", "Run a safe refactor loop"),
    RegisteredCommand::public("/compact", "Compact context"),
    RegisteredCommand::public("/fix", "Recover when the model cannot continue"),
    RegisteredCommand::public("/test", "Verify a claim/current changes with layered tests"),
    RegisteredCommand::public(
        "/initiatives",
        "Open initiatives overview / resume tracked initiatives",
    ),
    RegisteredCommand::public("/goals", "Legacy alias for /initiatives"),
    RegisteredCommand::public("/swarm", "Toggle swarm feature"),
    RegisteredCommand::public("/auto", "Work the list on its own after this turn"),
    RegisteredCommand::public("/context", "Show the full session context snapshot"),
    RegisteredCommand::public(
        "/skills",
        "Show loaded skills and kcode-endorsed recommendations",
    ),
    RegisteredCommand::public("/version", "Show current version"),
    RegisteredCommand::public("/changelog", "Show recent changes in this build"),
    RegisteredCommand::public("/info", "Show session info and tokens"),
    RegisteredCommand::public("/usage", "Show connected provider usage limits"),
    RegisteredCommand::public("/config", "Show or edit configuration"),
    RegisteredCommand::public("/log", "Mark the current location in the kcode logs"),
    RegisteredCommand::public(
        "/diff",
        "Cycle or set diff display mode (off/inline/full/pinned/file)",
    ),
    RegisteredCommand::public("/reload", "Restart the client with the current binary"),
    RegisteredCommand::public("/restart", "Restart with current binary"),
    RegisteredCommand::public("/rebuild", "Background rebuild and auto reload"),
    RegisteredCommand::public("/selfdev", "Open a new self-dev kcode session"),
    RegisteredCommand::public("/update", "Background update and auto reload"),
    RegisteredCommand::public("/update-sim", "Preview update UI safely (Alt+_)"),
    RegisteredCommand::public("/resume", "Open session picker"),
    RegisteredCommand::public("/sessions", "Alias for /resume"),
    RegisteredCommand::public("/session", "Alias for /resume"),
    RegisteredCommand::public("/active", "Manage live sessions (working vs ready)"),
    RegisteredCommand::public("/catchup", "Open Catch Up picker"),
    RegisteredCommand::public("/back", "Return to the previous Catch Up session"),
    RegisteredCommand::public("/save", "Bookmark session for easy access"),
    RegisteredCommand::public("/unsave", "Remove bookmark from session"),
    RegisteredCommand::public("/rename", "Rename current session"),
    RegisteredCommand::public("/fork", "Fork session into a new window (optional prompt)"),
    RegisteredCommand::hidden("/split", "Alias for /fork"),
    RegisteredCommand::public("/transfer", "Compact context into a fresh handoff session"),
    RegisteredCommand::public("/workspace", "Niri-style session workspace"),
    RegisteredCommand::public("/quit", "Exit kcode"),
    RegisteredCommand::public("/auth", "Show authentication status"),
    RegisteredCommand::public("/login", "Login to a provider"),
    RegisteredCommand::public("/logout", "Log out of a provider"),
    RegisteredCommand::public("/account", "Open the combined account picker"),
    RegisteredCommand::public("/accounts", "Alias for /account"),
    RegisteredCommand::public("/cache", "Show cache stats or set cache TTL"),
    RegisteredCommand::public("/debug-visual", "Toggle visual debug overlay"),
    RegisteredCommand::public("/screenshot-mode", "Toggle screenshot capture mode"),
    RegisteredCommand::public("/screenshot", "Capture a screenshot debug state"),
    RegisteredCommand::public("/record", "Record a demo capture"),
    RegisteredCommand::remote("/client-reload", "Force reload client binary"),
    RegisteredCommand::remote("/server-reload", "Force reload server binary"),
    RegisteredCommand::remote(
        "/continue",
        "Continue every interrupted live session that would auto-resume",
    ),
    RegisteredCommand::remote("/resumeall", "Alias for /continue"),
    RegisteredCommand::hidden("/resume-all", "Alias for /continue"),
    RegisteredCommand::hidden("/z", "Secret premium-mode command"),
    RegisteredCommand::hidden("/zz", "Secret premium-mode command"),
    RegisteredCommand::hidden("/zzz", "Secret premium-mode command"),
    RegisteredCommand::hidden("/zstatus", "Secret premium-mode status command"),
];

/// Every non-hidden slash command with its one-line description, in
/// registration order. The `/help` overlay uses this to list commands its
/// hand-written sections have not covered, so a newly registered command can
/// never be invisible to users.
pub(crate) fn registered_command_entries() -> impl Iterator<Item = (&'static str, &'static str)> {
    REGISTERED_COMMANDS
        .iter()
        .filter(|command| !command.hidden)
        .map(|command| (command.name, command.help))
}

impl App {
    pub fn input(&self) -> &str {
        &self.composer.input
    }

    #[cfg(test)]
    pub(crate) fn set_input_for_test(&mut self, input: impl Into<String>) {
        self.composer.input = input.into();
        self.composer.cursor_pos = self.composer.input.len();
    }

    /// Typo-resistant fuzzy score. Higher is better; `None` means no match.
    /// Delegates to the shared [`crate::tui::fuzzy`] matcher so slash-command
    /// ranking and highlight positions stay in sync.
    pub(super) fn fuzzy_score(needle: &str, haystack: &str) -> Option<i32> {
        crate::tui::fuzzy::fuzzy_score(needle, haystack)
    }

    pub(super) fn rank_suggestions(
        &self,
        needle: &str,
        candidates: Vec<(String, &'static str)>,
    ) -> Vec<(String, &'static str)> {
        let needle = needle.to_lowercase();
        // Bucket 1 = literal prefix matches (exact typing always wins).
        // Bucket 0 = typo-tolerant fuzzy matches by descending score.
        let mut scored: Vec<(u8, i32, String, &'static str)> = Vec::new();
        for (cmd, help) in candidates {
            let lower = cmd.to_lowercase();
            if lower.starts_with(&needle) {
                scored.push((1, i32::MAX, cmd, help));
            } else if let Some(score) = Self::fuzzy_score(&needle, &lower) {
                scored.push((0, score, cmd, help));
            }
        }
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.len().cmp(&b.2.len()))
                .then_with(|| a.2.cmp(&b.2))
        });
        scored
            .into_iter()
            .map(|(_, _, cmd, help)| (cmd, help))
            .collect()
    }

    fn command_candidates(&self) -> Vec<(String, &'static str)> {
        if let Some(cache) = self.command_suggestions.candidates_cache.borrow().as_ref() {
            return cache.candidates.clone();
        }

        fn push_skill_commands(
            commands: &mut Vec<(String, &'static str)>,
            seen: &mut std::collections::HashSet<String>,
            skills: &crate::skill::SkillRegistry,
        ) {
            for skill in skills.list() {
                let command = format!("/{}", skill.name);
                if seen.insert(command.clone()) {
                    commands.push((command, "Activate skill"));
                }
            }
        }

        let mut seen = std::collections::HashSet::new();
        let mut commands: Vec<(String, &'static str)> = REGISTERED_COMMANDS
            .iter()
            .filter(|command| !command.hidden)
            .filter_map(|command| {
                let name = command.name.to_string();
                seen.insert(name.clone()).then_some((name, command.help))
            })
            .collect();

        let skills = self.current_skills_snapshot();
        push_skill_commands(&mut commands, &mut seen, &skills);

        if self.is_remote_client() && !self.remote_skills.is_empty() {
            for skill in &self.remote_skills {
                let command = format!("/{skill}");
                if seen.insert(command.clone()) {
                    commands.push((command, "Activate skill"));
                }
            }
        }

        *self.command_suggestions.candidates_cache.borrow_mut() = Some(CommandCandidatesCache {
            candidates: commands.clone(),
        });
        commands
    }

    fn model_suggestion_candidates(&self) -> Vec<(String, &'static str)> {
        fn push_unique(
            seen: &mut std::collections::HashSet<String>,
            entries: &mut Vec<String>,
            model: String,
        ) {
            if !model.is_empty() && seen.insert(model.clone()) {
                entries.push(model);
            }
        }

        let mut seen = std::collections::HashSet::new();
        let mut models = Vec::new();

        if self.is_remote_client() {
            if let Some(current) = self.remote_provider_model.clone() {
                push_unique(&mut seen, &mut models, current);
            }

            let routes = if !self.remote_model_options.is_empty() {
                self.remote_model_options.clone()
            } else {
                self.build_remote_model_routes_fallback()
            };

            for route in routes {
                push_unique(&mut seen, &mut models, route.model);
            }

            for model in &self.remote_available_entries {
                push_unique(&mut seen, &mut models, model.clone());
            }
        } else {
            push_unique(&mut seen, &mut models, self.provider.model());
            for model in self.provider.available_models_display() {
                push_unique(&mut seen, &mut models, model);
            }
        }

        models
            .into_iter()
            .map(|model| (format!("/model {}", model), "Switch to model"))
            .collect()
    }

    fn model_provider_suggestion_candidates(&self, model: &str) -> Vec<(String, &'static str)> {
        fn push_unique(
            seen: &mut std::collections::HashSet<String>,
            entries: &mut Vec<(String, &'static str)>,
            command: String,
            help: &'static str,
        ) {
            if !command.is_empty() && seen.insert(command.clone()) {
                entries.push((command, help));
            }
        }

        let model = model.trim();
        if model.is_empty() {
            return Vec::new();
        }
        let Some(openrouter_model) = crate::provider::openrouter_catalog_model_id(model) else {
            return Vec::new();
        };

        let mut seen = std::collections::HashSet::new();
        let mut suggestions = Vec::new();
        push_unique(
            &mut seen,
            &mut suggestions,
            format!("/model {}@auto", openrouter_model),
            "Use automatic OpenRouter provider routing",
        );

        if self.is_remote_client() {
            let routes = if !self.remote_model_options.is_empty() {
                self.remote_model_options.clone()
            } else {
                self.build_remote_model_routes_fallback()
            };

            for route in routes {
                if route.model == model && route.api_method == "openrouter" {
                    let help = if route.provider == "auto" {
                        "Use automatic OpenRouter provider routing"
                    } else {
                        "Pin OpenRouter provider"
                    };
                    push_unique(
                        &mut seen,
                        &mut suggestions,
                        format!("/model {}@{}", openrouter_model, route.provider),
                        help,
                    );
                }
            }
        } else {
            for provider in self.provider.available_providers_for_model(model) {
                push_unique(
                    &mut seen,
                    &mut suggestions,
                    format!("/model {}@{}", openrouter_model, provider),
                    "Pin OpenRouter provider",
                );
            }
        }

        suggestions
    }

    /// Get command suggestions based on current input (or base input for cycling)
    pub(super) fn get_suggestions_for(&self, input: &str) -> Vec<(String, &'static str)> {
        let input = input.trim_start();

        if crate::tui::is_ssh_remote() {
            // Do not enumerate local account labels, projects, or goals while
            // completing a command intended for a different host.
            if input.starts_with("/model ") || input.starts_with("/models ") {
                return self.rank_suggestions(input, self.model_suggestion_candidates());
            }
            return if input.starts_with('/') {
                self.rank_suggestions(input, self.command_candidates())
            } else {
                Vec::new()
            };
        }

        // Only show suggestions when input starts with /
        if !input.starts_with('/') {
            return vec![];
        }

        let prefix = input.to_lowercase();
        let prefix_trimmed = prefix.trim_end();

        if prefix.starts_with("/model ") || prefix.starts_with("/models ") {
            if let Some(model_spec) = input
                .strip_prefix("/model ")
                .or_else(|| input.strip_prefix("/models "))
                && let Some((model, _provider_prefix)) = model_spec.rsplit_once('@')
            {
                let suggestions = self.model_provider_suggestion_candidates(model);
                if !suggestions.is_empty() {
                    return self.rank_suggestions(input, suggestions);
                }
            }

            let suggestions = self.model_suggestion_candidates();
            if suggestions.is_empty() {
                return vec![("/model".into(), "Open model picker")];
            }
            return self.rank_suggestions(input, suggestions);
        }

        if prefix.starts_with("/agents ") {
            return self.rank_suggestions(
                input,
                vec![
                    ("/agents swarm".into(), "Configure swarm/subagent model"),
                    ("/agents review".into(), "Configure code review model"),
                    ("/agents judge".into(), "Configure judge model"),
                ],
            );
        }

        if prefix.starts_with("/subagent-model ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/subagent-model inherit".into(),
                        "Use the current active model",
                    ),
                    (
                        "/subagent-model show".into(),
                        "Show the current subagent model policy",
                    ),
                ],
            );
        }

        if prefix.starts_with("/autoreview ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/autoreview status".into(),
                        "Show current autoreview status",
                    ),
                    ("/autoreview on".into(), "Enable end-of-turn autoreview"),
                    ("/autoreview off".into(), "Disable end-of-turn autoreview"),
                    ("/autoreview now".into(), "Launch a reviewer immediately"),
                ],
            );
        }

        if prefix_trimmed == "/autoreview" {
            return vec![
                (
                    "/autoreview status".into(),
                    "Show current autoreview status",
                ),
                ("/autoreview on".into(), "Enable end-of-turn autoreview"),
                ("/autoreview off".into(), "Disable end-of-turn autoreview"),
                ("/autoreview now".into(), "Launch a reviewer immediately"),
            ];
        }

        if prefix.starts_with("/autojudge ") {
            return self.rank_suggestions(
                input,
                vec![
                    ("/autojudge status".into(), "Show current autojudge status"),
                    ("/autojudge on".into(), "Enable end-of-turn autojudge"),
                    ("/autojudge off".into(), "Disable end-of-turn autojudge"),
                    ("/autojudge now".into(), "Launch a judge immediately"),
                ],
            );
        }

        if prefix_trimmed == "/autojudge" {
            return vec![
                ("/autojudge status".into(), "Show current autojudge status"),
                ("/autojudge on".into(), "Enable end-of-turn autojudge"),
                ("/autojudge off".into(), "Disable end-of-turn autojudge"),
                ("/autojudge now".into(), "Launch a judge immediately"),
            ];
        }

        if prefix.starts_with("/review ") {
            return self.rank_suggestions(
                input,
                vec![("/review".into(), "Launch a one-shot review immediately")],
            );
        }

        if prefix_trimmed == "/review" {
            return vec![("/review".into(), "Launch a one-shot review immediately")];
        }

        if prefix.starts_with("/judge ") {
            return self.rank_suggestions(
                input,
                vec![("/judge".into(), "Launch a one-shot judge immediately")],
            );
        }

        if prefix_trimmed == "/judge" {
            return vec![("/judge".into(), "Launch a one-shot judge immediately")];
        }

        if prefix_trimmed == "/subagent-model" {
            return vec![
                (
                    "/subagent-model show".into(),
                    "Show the current subagent model policy",
                ),
                (
                    "/subagent-model inherit".into(),
                    "Use the current active model",
                ),
            ];
        }

        if prefix.starts_with("/subagent ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/subagent --type general ".into(),
                        "Launch a general-purpose subagent",
                    ),
                    (
                        "/subagent --model ".into(),
                        "Launch a subagent with an explicit model",
                    ),
                    (
                        "/subagent --continue ".into(),
                        "Resume an existing subagent session",
                    ),
                ],
            );
        }

        if prefix_trimmed == "/subagent" {
            return vec![("/subagent ".into(), "Launch a subagent with a prompt")];
        }

        // /model opens the interactive picker, and `/model <name>` supports direct completion.
        if prefix_trimmed == "/model" || prefix_trimmed == "/models" {
            return vec![("/model".into(), "Open model picker or type `/model <name>`")];
        }

        if prefix_trimmed == "/agents" {
            return vec![("/agents".into(), "Open agent model config picker")];
        }

        if prefix.starts_with("/help ") || prefix.starts_with("/? ") {
            let base = if prefix.starts_with("/? ") {
                "/?"
            } else {
                "/help"
            };
            let topics = self
                .command_candidates()
                .into_iter()
                .map(|(cmd, help)| (format!("{} {}", base, cmd.trim_start_matches('/')), help))
                .collect();
            return self.rank_suggestions(input, topics);
        }

        if prefix.starts_with("/colors ") || prefix.starts_with("/color ") {
            let base = if prefix.starts_with("/color ") {
                "/color"
            } else {
                "/colors"
            };
            let mut suggestions: Vec<(String, &'static str)> = vec![
                (format!("{base} reset"), "Reset every color to its default"),
                (format!("{base} export"), "Print the palette as config TOML"),
            ];
            suggestions.extend(
                kcode_tui_style::ALL_ROLES
                    .iter()
                    .map(|role| (format!("{base} {} #", role.key()), "Set this color role")),
            );
            return self.rank_suggestions(input, suggestions);
        }

        if prefix.starts_with("/git ") {
            return self.rank_suggestions(
                input,
                vec![("/git status".into(), "Show branch and working tree status")],
            );
        }

        if prefix_trimmed == "/git" {
            return vec![("/git status".into(), "Show branch and working tree status")];
        }

        if prefix.starts_with("/transcript ") {
            return self.rank_suggestions(
                input,
                vec![(
                    "/transcript path".into(),
                    "Print transcript path without opening",
                )],
            );
        }

        if prefix_trimmed == "/transcript" {
            return vec![(
                "/transcript path".into(),
                "Print transcript path without opening",
            )];
        }

        if prefix.starts_with("/effort ") {
            let efforts = [
                "none",
                "minimal",
                "low",
                "medium",
                "high",
                "xhigh",
                "max",
                "swarm",
                "swarm-deep",
            ];
            return self.rank_suggestions(
                input,
                efforts
                    .iter()
                    .map(|e| (format!("/effort {}", e), effort_display_label(e)))
                    .collect(),
            );
        }

        if prefix.starts_with("/fast ") {
            let modes = [
                "on",
                "off",
                "status",
                "default on",
                "default off",
                "default status",
            ];
            return self.rank_suggestions(
                input,
                modes.iter().map(|m| (format!("/fast {}", m), *m)).collect(),
            );
        }

        if prefix.starts_with("/transport ") {
            let transports = ["auto", "https", "websocket"];
            return self.rank_suggestions(
                input,
                transports
                    .iter()
                    .map(|t| (format!("/transport {}", t), *t))
                    .collect(),
            );
        }

        if prefix.starts_with("/compact ") {
            let suggestions = vec![
                ("/compact mode".into(), "Show/change compaction mode"),
                (
                    "/compact mode status".into(),
                    "Show the current compaction mode",
                ),
                ("/compact mode reactive".into(), "Use reactive compaction"),
                ("/compact mode proactive".into(), "Use proactive compaction"),
                ("/compact mode semantic".into(), "Use semantic compaction"),
            ];
            return self.rank_suggestions(input, suggestions);
        }

        if prefix.starts_with("/compact mode ") {
            let modes = ["reactive", "proactive"];
            let mut suggestions: Vec<(String, &'static str)> = vec![(
                "/compact mode status".into(),
                "Show the current compaction mode",
            )];
            suggestions.extend(
                modes
                    .iter()
                    .map(|mode| (format!("/compact mode {}", mode), *mode)),
            );
            return self.rank_suggestions(input, suggestions);
        }

        if prefix.starts_with("/cache ") {
            let suggestions = vec![
                ("/cache stats".into(), "Show KV cache stats"),
                ("/cache status".into(), "Alias for /cache stats"),
                ("/cache 1h".into(), "Use 1 hour cache TTL"),
                ("/cache 5m".into(), "Use 5 minute cache TTL"),
            ];
            return self.rank_suggestions(input, suggestions);
        }

        if prefix.starts_with("/login ") || prefix.starts_with("/auth ") {
            let base = if prefix.starts_with("/auth ") {
                "/auth"
            } else {
                "/login"
            };
            let mut suggestions: Vec<(String, &'static str)> = Vec::new();
            if base == "/auth" {
                suggestions.push(("/auth doctor".into(), "Diagnose provider auth issues"));
            }
            suggestions.extend(
                crate::provider_catalog::tui_login_providers()
                    .iter()
                    .map(|provider| (format!("{} {}", base, provider.id), provider.menu_detail)),
            );
            return self.rank_suggestions(input, suggestions);
        }

        if prefix.starts_with("/account ") || prefix.starts_with("/accounts ") {
            let mut suggestions = vec![
                ("/account list".into(), "Open all provider/account actions"),
                ("/account switch".into(), "Switch active account by label"),
                (
                    "/account default-provider".into(),
                    "Set preferred default provider",
                ),
                (
                    "/account default-model".into(),
                    "Set preferred default model",
                ),
                (
                    "/account openai-compatible settings".into(),
                    "Inspect custom OpenAI-compatible settings",
                ),
                (
                    "/account openai-compatible api-base".into(),
                    "Set custom OpenAI-compatible API base",
                ),
            ];
            for provider in crate::provider_catalog::login_providers() {
                suggestions.push((
                    format!("/account {}", provider.id),
                    "Open this provider's account/settings actions",
                ));
                suggestions.push((
                    format!("/account {} settings", provider.id),
                    "Show provider-specific settings",
                ));
                suggestions.push((
                    format!("/account {} login", provider.id),
                    "Start or refresh login for this provider",
                ));
            }
            suggestions.push(("/account claude add".into(), "Add a new Claude account"));
            suggestions.push(("/account openai add".into(), "Add a new OpenAI account"));
            suggestions.push((
                "/account openai transport".into(),
                "Set OpenAI transport preference",
            ));
            suggestions.push((
                "/account openai effort".into(),
                "Set OpenAI reasoning effort preference",
            ));
            if let Ok(accounts) = crate::auth::claude::list_accounts() {
                for account in accounts {
                    suggestions.push((
                        format!("/account claude switch {}", account.label),
                        "Switch to this Claude account",
                    ));
                }
            }
            if let Ok(accounts) = crate::auth::codex::list_accounts() {
                for account in accounts {
                    suggestions.push((
                        format!("/account openai switch {}", account.label),
                        "Switch to this OpenAI account",
                    ));
                }
            }
            return self.rank_suggestions(input, suggestions);
        }

        if prefix.starts_with("/improve ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/improve plan".into(),
                        "Generate a ranked improve todo list without editing",
                    ),
                    (
                        "/improve resume".into(),
                        "Resume the last saved improve mode for this session",
                    ),
                    (
                        "/improve status".into(),
                        "Show current improve batch and inferred status",
                    ),
                    (
                        "/improve stop".into(),
                        "Stop improvement mode after the next safe point",
                    ),
                ],
            );
        }

        if prefix.starts_with("/refactor ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/refactor plan".into(),
                        "Generate a ranked refactor todo list without editing",
                    ),
                    (
                        "/refactor resume".into(),
                        "Resume the last saved refactor mode for this session",
                    ),
                    (
                        "/refactor status".into(),
                        "Show current refactor batch and inferred status",
                    ),
                    (
                        "/refactor stop".into(),
                        "Stop refactor mode after the next safe point",
                    ),
                ],
            );
        }

        if prefix.starts_with("/swarm ") {
            return self.rank_suggestions(
                input,
                vec![
                    ("/swarm on".into(), "Enable swarm for this session"),
                    ("/swarm off".into(), "Disable swarm for this session"),
                    ("/swarm status".into(), "Show swarm feature status"),
                ],
            );
        }

        if prefix.starts_with("/subscription ") {
            return self.rank_suggestions(
                input,
                vec![("/subscription status".into(), "Show subscription status")],
            );
        }

        if prefix.starts_with("/alignment ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/alignment status".into(),
                        "Show current and saved alignment",
                    ),
                    (
                        "/alignment centered".into(),
                        "Save centered alignment and apply it now",
                    ),
                    (
                        "/alignment left".into(),
                        "Save left-aligned layout and apply it now",
                    ),
                ],
            );
        }

        if prefix.starts_with("/compact-notifications ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/compact-notifications status".into(),
                        "Show whether notifications are compact",
                    ),
                    (
                        "/compact-notifications on".into(),
                        "Collapse swarm/file-activity notifications to one line",
                    ),
                    (
                        "/compact-notifications off".into(),
                        "Show full multi-line notification cards",
                    ),
                ],
            );
        }

        if prefix.starts_with("/tool-call-details ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/tool-call-details status".into(),
                        "Show whether technical details render on intent rows",
                    ),
                    (
                        "/tool-call-details on".into(),
                        "Show the dimmed technical detail next to tool intents",
                    ),
                    (
                        "/tool-call-details off".into(),
                        "Show only the intent on tool rows that have one",
                    ),
                ],
            );
        }

        if prefix.starts_with("/show-kgrep-output ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/show-kgrep-output status".into(),
                        "Show whether kgrep output is shown inline",
                    ),
                    (
                        "/show-kgrep-output on".into(),
                        "Render full kgrep search results inline in chat",
                    ),
                    (
                        "/show-kgrep-output off".into(),
                        "Show only the one-line kgrep summary",
                    ),
                ],
            );
        }

        if prefix.starts_with("/config ") {
            return self.rank_suggestions(
                input,
                vec![
                    ("/config init".into(), "Create a default config file"),
                    ("/config create".into(), "Alias for /config init"),
                    ("/config edit".into(), "Open the config file in $EDITOR"),
                ],
            );
        }

        if prefix.starts_with("/goals show ") {
            let relevant_goals = crate::goal::list_relevant_goals(
                self.session
                    .working_dir
                    .as_deref()
                    .map(std::path::Path::new),
            )
            .unwrap_or_default();
            let suggestions = relevant_goals
                .into_iter()
                .map(|goal| (format!("/goals show {}", goal.id), "Open this goal"))
                .collect();
            return self.rank_suggestions(input, suggestions);
        }

        if prefix.starts_with("/goals ") {
            return self.rank_suggestions(
                input,
                vec![
                    ("/goals resume".into(), "Resume the current goal"),
                    ("/goals show".into(), "Open a specific goal by id"),
                ],
            );
        }

        if prefix.starts_with("/selfdev ") {
            return self.rank_suggestions(
                input,
                vec![
                    (
                        "/selfdev status".into(),
                        "Show current self-dev/build status",
                    ),
                    ("/selfdev enter".into(), "Open a blank self-dev session"),
                    (
                        "/selfdev enter ".into(),
                        "Open a self-dev session with a prompt",
                    ),
                ],
            );
        }

        if prefix.starts_with("/rewind ") {
            let arg = prefix.strip_prefix("/rewind ").unwrap_or_default().trim();
            let visible_count = self.session.rewind_target_count();

            // Rewind targets are 1-based visible conversation message numbers.
            // Do not fuzzy-rank numeric arguments: `/rewind 10` should never be
            // completed or preview-accepted as `/rewind 1` just because `1` is a
            // fuzzy prefix match. If a complete numeric target is present, only
            // surface the exact valid command.
            if !arg.is_empty() && arg.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(n) = arg.parse::<usize>()
                    && (1..=visible_count).contains(&n)
                {
                    return vec![(format!("/rewind {}", n), "Rewind to this message")];
                }
                return Vec::new();
            }

            let suggestions = (1..=visible_count)
                .map(|n| (format!("/rewind {}", n), "Rewind to this message"))
                .collect();
            return self.rank_suggestions(input, suggestions);
        }

        self.rank_suggestions(&prefix, self.command_candidates())
    }

    /// Get command suggestions based on current input
    pub fn command_suggestions(&self) -> Vec<(String, &'static str)> {
        // Read up to eight times per frame; recomputing each time re-ranks
        // every registered command and skill (and can touch disk for some
        // prefixes). Memoize on the exact input plus the guard state the
        // branches below consult, so any transition still recomputes.
        let signature = self.command_suggestions_signature();
        let epoch = self.command_suggestions.epoch.get();
        if let Some(cache) = self.command_suggestions.cache.borrow().as_ref()
            && cache.epoch == epoch
            && cache.signature == signature
            && cache.input == self.composer.input
        {
            return cache.suggestions.clone();
        }

        let suggestions = self.command_suggestions_uncached(&signature);
        *self.command_suggestions.cache.borrow_mut() = Some(CommandSuggestionsCache {
            input: self.composer.input.clone(),
            signature,
            epoch,
            suggestions: suggestions.clone(),
        });
        suggestions
    }

    /// Snapshot the non-input state that `command_suggestions` branches on
    /// before consulting the input buffer.
    pub(super) fn command_suggestions_signature(&self) -> CommandSuggestionsSignature {
        CommandSuggestionsSignature {
            pending_login: self.pending_login.is_some(),
            pending_account_input: self.account_picker.pending_input.is_some(),
            pending_ssh_remote_name: self.pending_ssh_remote_name.is_some(),
            inline_preview_kind: self
                .inline_interactive_state
                .as_ref()
                .filter(|picker| picker.preview)
                .map(|picker| picker.kind),
        }
    }

    /// Uncached body of [`Self::command_suggestions`].
    pub(super) fn command_suggestions_uncached(
        &self,
        signature: &CommandSuggestionsSignature,
    ) -> Vec<(String, &'static str)> {
        // While an interactive prompt is waiting for typed input (API key,
        // OAuth callback, account label, SSH target), the composer is an
        // answer box, not a command line. Rendering the full command palette
        // there is misleading (issue #496): the only command those prompts
        // advertise is /cancel, so suggest exactly that and nothing else.
        if signature.pending_login
            || signature.pending_account_input
            || signature.pending_ssh_remote_name
        {
            let input = self.composer.input.trim_start();
            let typed = input.trim_end();
            if !typed.is_empty() && typed.starts_with('/') && "/cancel".starts_with(typed) {
                return vec![("/cancel".into(), "Cancel the pending prompt")];
            }
            return Vec::new();
        }

        // While an inline picker preview is open for the command being typed,
        // the picker itself is the suggestion surface. Rendering the textual
        // suggestion list underneath would duplicate it (and its rows are not
        // arrow-navigable anyway, since the preview claims Up/Down first).
        if let Some(kind) = signature.inline_preview_kind {
            let input = self.composer.input.trim_start();
            let suppress = match kind {
                crate::tui::PickerKind::Model => {
                    input.starts_with("/model") || input.starts_with("/models")
                }
                crate::tui::PickerKind::Login => input.starts_with("/login"),
                _ => false,
            };
            if suppress {
                return Vec::new();
            }
        }
        self.get_suggestions_for(&self.composer.input)
    }

    fn clamp_command_suggestion_selection(&mut self) -> Vec<(String, &'static str)> {
        let suggestions = self.command_suggestions();
        if suggestions.is_empty() {
            self.command_suggestions.selected = 0;
        } else {
            self.command_suggestions.selected = self
                .command_suggestions
                .selected
                .min(suggestions.len().saturating_sub(1));
        }
        suggestions
    }

    pub(super) fn move_command_suggestion_selection(&mut self, delta: i32) -> bool {
        let suggestions = self.clamp_command_suggestion_selection();
        if suggestions.is_empty() {
            return false;
        }

        let len = suggestions.len() as i32;
        let selected = self.command_suggestions.selected as i32;
        self.command_suggestions.selected = (selected + delta).rem_euclid(len) as usize;
        true
    }

    fn arrow_modifiers_allow_command_suggestion_navigation(modifiers: KeyModifiers) -> bool {
        !modifiers.intersects(
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::HYPER,
        )
    }

    pub(super) fn handle_command_suggestion_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if self.command_suggestions().is_empty() {
            return false;
        }

        match code {
            KeyCode::Down
                if Self::arrow_modifiers_allow_command_suggestion_navigation(modifiers) =>
            {
                self.move_command_suggestion_selection(1)
            }
            KeyCode::Up if Self::arrow_modifiers_allow_command_suggestion_navigation(modifiers) => {
                self.move_command_suggestion_selection(-1)
            }
            KeyCode::Char('j') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_command_suggestion_selection(1)
            }
            KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_command_suggestion_selection(-1)
            }
            KeyCode::Enter if modifiers.is_empty() => self.accept_selected_command_suggestion(),
            _ => false,
        }
    }

    pub(super) fn accept_selected_command_suggestion(&mut self) -> bool {
        let suggestions = self.clamp_command_suggestion_selection();
        let Some((cmd, _)) = suggestions.get(self.command_suggestions.selected).cloned() else {
            return false;
        };
        if cmd == self.composer.input.trim() {
            return false;
        }

        self.composer.remember_input_undo_state();
        self.composer.input = cmd;
        self.composer.cursor_pos = self.composer.input.len();
        self.composer.tab_completion_state = None;
        self.command_suggestions.selected = 0;
        self.sync_model_picker_preview_from_input();
        true
    }

    /// The starter line for the empty first screen: one hint, and only when it
    /// is actionable. Setup guidance otherwise lives in the README, so this does
    /// not grow a roster again.
    pub fn suggestion_prompts(&self) -> Vec<(String, String)> {
        if crate::tui::is_ssh_remote() {
            return Vec::new();
        }
        let is_canary = if self.is_remote_client() {
            self.server_info.is_canary.unwrap_or(self.session.is_canary)
        } else {
            self.session.is_canary
        };
        if is_canary {
            return Vec::new();
        }

        // One actionable line, and only when it is actionable. Setup guidance
        // otherwise lives in the README, so this does not grow a roster again.
        // `any_provider_usable` also counts a configured OpenAI-compatible
        // profile, which `has_any_available` alone misses.
        if !crate::auth::AuthStatus::any_provider_usable() {
            return vec![("Log in to get started".to_string(), "/login".to_string())];
        }

        Vec::new()
    }

    /// Autocomplete current input - cycles through suggestions on repeated Tab
    pub fn autocomplete(&mut self) -> bool {
        // Get suggestions for current input
        let current_suggestions = self.get_suggestions_for(&self.composer.input);

        // Check if we're continuing a tab cycle from a previous base
        if let Some((ref base, idx)) = self.composer.tab_completion_state.clone() {
            let base_suggestions = self.get_suggestions_for(base);

            // If current input is in base suggestions AND there are multiple options, continue cycling
            if base_suggestions.len() > 1
                && base_suggestions
                    .iter()
                    .any(|(cmd, _)| cmd == &self.composer.input)
            {
                let next_index = (idx + 1) % base_suggestions.len();
                let (cmd, _) = &base_suggestions[next_index];
                self.composer.remember_input_undo_state();
                self.composer.input = cmd.clone();
                self.composer.cursor_pos = self.composer.input.len();
                self.composer.tab_completion_state = Some((base.clone(), next_index));
                return true;
            }
            // Otherwise, fall through to start a new cycle with current input
        }

        // Start fresh cycle with current input
        if current_suggestions.is_empty() {
            self.composer.tab_completion_state = None;
            return false;
        }

        // If only one suggestion and it matches exactly, add trailing space for commands
        // that accept arguments, then we're done
        if current_suggestions.len() == 1 && current_suggestions[0].0 == self.composer.input {
            if !self.composer.input.ends_with(' ')
                && Self::command_accepts_args(&self.composer.input)
            {
                self.composer.remember_input_undo_state();
                self.composer.input.push(' ');
                self.composer.cursor_pos = self.composer.input.len();
                return true;
            }
            self.composer.tab_completion_state = None;
            return false;
        }

        // Apply first suggestion and start tracking the cycle
        let selected = self
            .command_suggestions
            .selected
            .min(current_suggestions.len().saturating_sub(1));
        let (cmd, _) = &current_suggestions[selected];
        let base = self.composer.input.clone();
        self.composer.remember_input_undo_state();
        self.composer.input = cmd.clone();
        // If unique match, add trailing space for arg-accepting commands
        if current_suggestions.len() == 1 && Self::command_accepts_args(&self.composer.input) {
            self.composer.input.push(' ');
        }
        self.composer.cursor_pos = self.composer.input.len();
        self.composer.tab_completion_state = Some((base, selected));
        self.command_suggestions.selected = 0;
        true
    }

    /// Reset tab completion state (call when user types/modifies input)
    pub fn reset_tab_completion(&mut self) {
        self.composer.tab_completion_state = None;
        self.command_suggestions.selected = 0;
    }

    pub(super) fn undo_input_change(&mut self) {
        if let Some((input, cursor_pos)) = self.composer.input_undo_stack.pop() {
            self.composer.input = input;
            self.composer.cursor_pos = cursor_pos.min(self.composer.input.len());
            self.reset_tab_completion();
            self.sync_model_picker_preview_from_input();
            self.set_status_notice("↶ Input restored");
        } else {
            self.set_status_notice("Nothing to undo");
        }
    }

    pub(super) fn command_accepts_args(cmd: &str) -> bool {
        matches!(
            cmd.trim(),
            "/help"
                | "/?"
                | "/btw"
                | "/fork"
                | "/git"
                | "/transcript"
                | "/observe"
                | "/todos"
                | "/splitview"
                | "/split-view"
                | "/model"
                | "/agents"
                | "/effort"
                | "/fast"
                | "/transport"
                | "/login"
                | "/auth"
                | "/account"
                | "/account claude"
                | "/account switch"
                | "/account openai"
                | "/account openai-compatible"
                | "/account default-provider"
                | "/account default-model"
                | "/account claude switch"
                | "/account claude remove"
                | "/account openai switch"
                | "/account openai remove"
                | "/usage"
                | "/test"
                | "/initiatives"
                | "/initiatives show"
                | "/goals"
                | "/goals show"
                | "/swarm"
                | "/plan"
                | "/improve"
                | "/refactor"
                | "/rewind"
                | "/compact"
                | "/compact mode"
                | "/alignment"
                | "/compact-notifications"
                | "/show-kgrep-output"
                | "/reasoning"
                | "/thinking"
                | "/thinking-display"
                | "/config"
                | "/save"
                | "/rename"
                | "/cache"
        )
    }
}

#[cfg(test)]
mod registered_command_tests {
    use super::*;

    /// Every slash command must be registered exactly once. Duplicate entries
    /// mean two different handlers claim the same name, so which one runs
    /// depends on dispatch order rather than on the registry the palette and
    /// `/help` show the user.
    #[test]
    fn registered_commands_have_no_duplicate_names() {
        let mut seen = std::collections::HashSet::new();
        let duplicates: Vec<&str> = REGISTERED_COMMANDS
            .iter()
            .filter(|command| !seen.insert(command.name))
            .map(|command| command.name)
            .collect();
        assert!(
            duplicates.is_empty(),
            "duplicate slash command registrations: {:?}",
            duplicates
        );
    }

    /// Aliases users can actually type must be discoverable through the
    /// registry, otherwise autocomplete silently omits working commands.
    #[test]
    fn known_aliases_are_registered() {
        let names: std::collections::HashSet<&str> =
            REGISTERED_COMMANDS.iter().map(|c| c.name).collect();
        for alias in ["/commit-and-push", "/resume-all", "/hotkeys"] {
            assert!(names.contains(alias), "{alias} is not registered");
        }
    }
}
