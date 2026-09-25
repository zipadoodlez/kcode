use super::*;

impl Config {
    pub fn display_string(&self) -> String {
        let path = Self::path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let mut effective_disabled_tools: Vec<String> =
            self.tools.selection().disabled_tools.into_iter().collect();
        effective_disabled_tools.sort();

        format!(
            r#"**Configuration** (`{}`)

**Keybindings:**
- Scroll up: `{}`
- Scroll down: `{}`
- Scroll up fallback: `{}`
- Scroll down fallback: `{}`
- Page up: `{}`
- Page down: `{}`
- Model next: `{}`
- Model prev: `{}`
- Effort increase: `{}`
- Effort decrease: `{}`
- Centered toggle: `{}`
- Prompt up: `{}`
- Prompt down: `{}`
- Scroll bookmark: `{}`
- Auto-poke toggle: `{}`
- Workspace left: `{}`
- Workspace down: `{}`
- Workspace up: `{}`
- Workspace right: `{}`

**Display:**
- Diff mode: {}
- Centered: {}
- Markdown spacing: {}
- Diff line wrap: {}
- Queue mode: {}
- Auto server reload: {}
- Mouse capture: {}
- Debug socket: {}
- Emoji: {}
- Compact notifications: {}
- Chat native scrollbar: {}
- Side panel native scrollbar: {}
- Redraw FPS: {}
- Copy badge Alt label: {}
- Show agentgrep output: {}
- Tool call details: {}
- Theme: {}
- Custom colors: {}
- Palette slots: {}

**Features:**
- Check updates: {}
- Swarm: {}
- Auto-poke: {}
- Message timestamps: {}
- KV cache miss notices: {}
- Update channel: {}

**Tools:**
- Profile: {}
- Enabled allow-list: {}
- Disabled tools: {}
- Disable base tools: {}
- MCP tools: {}
- MCP auto threshold: {} tokens

**Provider:**
- Default model: {}
- Default provider: {}
- OpenAI reasoning effort: {}
- Anthropic reasoning effort: {}
- OpenAI transport: {}
- OpenAI service tier: {}
- OpenAI native compaction: {}
- OpenAI native compaction threshold ratio: {:.2}
- Cross-provider failover: {}

**Agent models:**
- Swarm / subagent: {}
- Swarm spawn mode: {}
- Swarm root effort: {}
- Deep swarm root effort: {}
- Spawn hook: {}
- Review: {}
- Judge: {}

*Edit the config file or set environment variables to customize.*
*Environment variables (e.g., `JCODE_SCROLL_UP_KEY`, `JCODE_SCROLL_DOWN_KEY`) override file settings.*"#,
            path,
            self.keybindings.scroll_up,
            self.keybindings.scroll_down,
            self.keybindings.scroll_up_fallback,
            self.keybindings.scroll_down_fallback,
            self.keybindings.scroll_page_up,
            self.keybindings.scroll_page_down,
            self.keybindings.model_switch_next,
            self.keybindings.model_switch_prev,
            self.keybindings.effort_increase,
            self.keybindings.effort_decrease,
            self.keybindings.centered_toggle,
            self.keybindings.scroll_prompt_up,
            self.keybindings.scroll_prompt_down,
            self.keybindings.scroll_bookmark,
            if self.keybindings.auto_poke_toggle.trim().is_empty() {
                "disabled"
            } else {
                self.keybindings.auto_poke_toggle.trim()
            },
            self.keybindings.workspace_left,
            self.keybindings.workspace_down,
            self.keybindings.workspace_up,
            self.keybindings.workspace_right,
            self.display.diff_mode.label(),
            self.display.centered,
            self.display.markdown_spacing.label(),
            self.display.diff_line_wrap,
            self.display.queue_mode,
            self.display.auto_server_reload,
            self.display.mouse_capture,
            self.display.debug_socket,
            self.display.emoji,
            self.display.compact_notifications,
            self.display.native_scrollbars.chat,
            self.display.native_scrollbars.side_panel,
            self.display.redraw_fps,
            if self.display.copy_badge_alt_label.trim().is_empty() {
                "auto"
            } else {
                self.display.copy_badge_alt_label.trim()
            },
            self.display.show_agentgrep_output,
            self.display.tool_call_details,
            if self.display.theme.trim().is_empty() {
                "dark"
            } else {
                self.display.theme.trim()
            },
            if self.display.colors.is_empty() {
                "default (run /colors to customize)".to_string()
            } else {
                format!(
                    "{} custom ({})",
                    self.display.colors.len(),
                    self.display
                        .colors
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
            if self.display.palette.is_empty() {
                "default".to_string()
            } else {
                format!("{} slot(s)", self.display.palette.len())
            },
            self.features.check_updates,
            self.features.swarm,
            self.features.auto_poke,
            self.features.message_timestamps,
            self.features.kv_cache_miss_notices,
            self.features.update_channel,
            if self.tools.profile.trim().is_empty() {
                "full"
            } else {
                self.tools.profile.trim()
            },
            if self.tools.enabled.is_empty() {
                "(none)".to_string()
            } else {
                self.tools.enabled.join(", ")
            },
            if effective_disabled_tools.is_empty() {
                "(none)".to_string()
            } else {
                effective_disabled_tools.join(", ")
            },
            self.tools.disable_base_tools,
            self.tools.mcp_tools.as_str(),
            self.tools.mcp_tools_token_threshold,
            self.provider
                .default_model
                .as_deref()
                .unwrap_or("(provider default)"),
            self.provider
                .default_provider
                .as_deref()
                .unwrap_or("(auto)"),
            self.provider
                .openai_reasoning_effort
                .as_deref()
                .unwrap_or("(provider default)"),
            self.provider
                .anthropic_reasoning_effort
                .as_deref()
                .unwrap_or("(provider default)"),
            self.provider
                .openai_transport
                .as_deref()
                .unwrap_or("(auto)"),
            self.provider
                .openai_service_tier
                .as_deref()
                .unwrap_or("(default)"),
            self.provider.openai_native_compaction_mode.as_str(),
            self.provider.openai_native_compaction_threshold_tokens,
            self.provider.cross_provider_failover.as_str(),
            self.agents
                .swarm_model
                .as_deref()
                .unwrap_or("(inherit current session)"),
            self.agents.swarm_spawn_mode.as_str(),
            self.agents.root_effort_for_swarm(false),
            self.agents.root_effort_for_swarm(true),
            self.terminal
                .spawn_hook
                .as_deref()
                .unwrap_or("(built-in terminal detection)"),
            self.autoreview
                .model
                .as_deref()
                .unwrap_or("(inherit current session)"),
            self.autojudge
                .model
                .as_deref()
                .unwrap_or("(inherit current session)"),
        )
    }
}
