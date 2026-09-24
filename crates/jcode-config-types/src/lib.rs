use serde::{Deserialize, Serialize};

mod display;
pub use display::DisplayConfig;
pub mod keybindings;
mod serde_lenient;
pub use keybindings::{
    KEYBINDING_DEFAULTS, KeybindingDefault, KeybindingIssue, KeybindingIssueKind,
    KeybindingPlatform, KeybindingProvenance, PlatformDefault, default_binding, default_binding_or,
    keybinding_default, keybinding_defaults_report, validate_keybinding_defaults,
};

/// Compaction mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CompactionMode {
    /// Compact when context hits a fixed threshold (default)
    #[default]
    Reactive,
    /// Compact early based on predicted token growth rate
    Proactive,
}

impl CompactionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reactive => "reactive",
            Self::Proactive => "proactive",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "reactive" => Some(Self::Reactive),
            "proactive" => Some(Self::Proactive),
            _ => None,
        }
    }
}

/// Session picker Enter action: "current-terminal" (default) or "new-terminal".
/// Ctrl+Enter performs the alternate action.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SessionPickerResumeAction {
    NewTerminal,
    #[default]
    CurrentTerminal,
}

impl SessionPickerResumeAction {
    pub fn alternate(self) -> Self {
        match self {
            Self::NewTerminal => Self::CurrentTerminal,
            Self::CurrentTerminal => Self::NewTerminal,
        }
    }
}

/// How to display file diffs from edit/write tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffDisplayMode {
    /// Don't show diffs at all.
    Off,
    /// Show diffs inline in the chat (default).
    #[default]
    Inline,
    /// Show the full inline diff in the chat without preview truncation.
    #[serde(
        rename = "full-inline",
        alias = "full_inline",
        alias = "fullinline",
        alias = "inline-full",
        alias = "inline_full",
        alias = "inlinefull",
        alias = "full"
    )]
    FullInline,
    /// Show diffs in a dedicated pinned pane.
    Pinned,
    /// Show full file with diff highlights in side panel, synced to scroll position.
    File,
}

impl DiffDisplayMode {
    pub fn is_inline(&self) -> bool {
        matches!(self, Self::Inline | Self::FullInline)
    }

    pub fn is_full_inline(&self) -> bool {
        matches!(self, Self::FullInline)
    }

    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::Pinned)
    }

    pub fn is_file(&self) -> bool {
        matches!(self, Self::File)
    }

    pub fn has_side_pane(&self) -> bool {
        matches!(self, Self::Pinned | Self::File)
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::Inline,
            Self::Inline => Self::FullInline,
            Self::FullInline => Self::Pinned,
            Self::Pinned => Self::File,
            Self::File => Self::Off,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Inline => "Inline",
            Self::FullInline => "Inline Full",
            Self::Pinned => "Pinned",
            Self::File => "File",
        }
    }
}

/// When to show the overscroll status line (model/provider/context info below
/// the input).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverscrollStatusMode {
    /// Never show the status line.
    Off,
    /// Always show the status line below the input.
    On,
    /// Elastic reveal: show it briefly when scrolling past the bottom (default).
    #[default]
    Overscroll,
}

impl OverscrollStatusMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Overscroll => "overscroll",
        }
    }
}

/// How much vertical spacing to use when rendering markdown blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkdownSpacingMode {
    /// Compact chat/TUI-oriented spacing.
    #[default]
    Compact,
    /// Document-style spacing between top-level blocks.
    Document,
}

impl MarkdownSpacingMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Document => "Document",
        }
    }
}

/// How to display the model's reasoning/thinking content in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningDisplayMode {
    /// Never display reasoning content.
    #[default]
    Off,
    /// Keep every reasoning trace in the transcript (classic behavior).
    Full,
    /// Show only the *current* reasoning live; collapse it once the model
    /// commits an assistant message or tool call, then show the next one.
    Current,
}

impl ReasoningDisplayMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Full => "Full",
            Self::Current => "Current",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::Current,
            Self::Current => Self::Full,
            Self::Full => Self::Off,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "off" | "none" | "false" | "0" | "no" => Some(Self::Off),
            "full" | "all" | "true" | "1" | "yes" | "on" => Some(Self::Full),
            "current" | "live" | "ephemeral" | "collapse" => Some(Self::Current),
            _ => None,
        }
    }
}

/// Update channel: how aggressively to receive updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    /// Only update from tagged GitHub Releases (default).
    #[default]
    Stable,
    /// Update from latest commit on main branch (bleeding edge).
    Main,
}

impl UpdateChannel {
    /// Parse a channel name, returning `None` for unknown values.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "stable" | "release" => Some(Self::Stable),
            "main" | "nightly" | "edge" => Some(Self::Main),
            _ => None,
        }
    }
}

/// Config deserialization is deliberately lenient: an unknown or removed
/// channel name (e.g. a stale `update_channel = "manual"` left in
/// config.toml) falls back to the default channel instead of failing the
/// entire config parse. A strict enum here once made the freshly exec'd
/// server die during the reload handoff, leaving the handoff marker stuck
/// in `starting` and clients re-requesting the reload forever (issue #349).
impl<'de> Deserialize<'de> for UpdateChannel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::parse(&value).unwrap_or_default())
    }
}

impl std::fmt::Display for UpdateChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stable => write!(f, "stable"),
            Self::Main => write!(f, "main"),
        }
    }
}

/// Cross-provider failover behavior when the same input would be resent elsewhere.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CrossProviderFailoverMode {
    /// Show a 3-second cancelable countdown, then resend on another provider.
    #[default]
    Countdown,
    /// Do not resend the prompt to another provider automatically.
    #[serde(alias = "off", alias = "false", alias = "disabled", alias = "none")]
    Manual,
}

impl CrossProviderFailoverMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Countdown => "countdown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "manual" | "off" | "false" | "disabled" | "none" => Some(Self::Manual),
            "countdown" | "auto" | "automatic" => Some(Self::Countdown),
            _ => None,
        }
    }
}

/// Compaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompactionConfig {
    /// Compaction mode: reactive (default) or proactive
    pub mode: CompactionMode,

    /// [proactive] Number of turns to look ahead when projecting token growth
    pub lookahead_turns: usize,

    /// [proactive] EWMA alpha for token growth smoothing (0.0-1.0, higher = more recency bias)
    pub ewma_alpha: f32,

    /// [proactive] Minimum context fill level before any proactive check fires (0.0-1.0)
    pub proactive_floor: f32,

    /// [proactive] Minimum number of token snapshots needed before proactive check
    pub min_samples: usize,

    /// [proactive] Number of stable turns (no growth) before suppressing proactive compact
    pub stall_window: usize,

    /// [proactive] Minimum turns between two compactions (cooldown)
    pub min_turns_between_compactions: usize,

    /// Hard cap on the token budget compaction measures against, regardless of
    /// the model's advertised context window. 0 = no cap (use the model window).
    ///
    /// Every turn re-sends the whole transcript, so on a 1M-window model the
    /// default 80%-of-window trigger lets a session reach ~800k tokens per
    /// request before anything folds. Set this to e.g. 200000 to compact earlier
    /// on large-window providers. This bounds the compaction trigger budget,
    /// not the final request size when recent messages cannot be compacted.
    pub max_context_tokens: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            mode: CompactionMode::Reactive,
            lookahead_turns: 15,
            ewma_alpha: 0.3,
            proactive_floor: 0.40,
            min_samples: 3,
            stall_window: 5,
            min_turns_between_compactions: 10,
            max_context_tokens: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NamedProviderType {
    #[serde(alias = "openai-compatible", alias = "openai_compatible")]
    #[default]
    OpenAiCompatible,
    #[serde(alias = "anthropic-compatible", alias = "anthropic_compatible")]
    AnthropicCompatible,
    OpenRouter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NamedProviderAuth {
    #[serde(alias = "Bearer", alias = "BEARER")]
    #[default]
    Bearer,
    #[serde(alias = "Header", alias = "HEADER")]
    Header,
    #[serde(alias = "None", alias = "NONE")]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct NamedProviderModelConfig {
    pub id: String,
    /// Explicitly enable or disable `/effort` for this model. When omitted,
    /// the provider-level setting and built-in model-family detection apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Reasoning effort selected when this model becomes active. This overrides
    /// `[provider].openai_reasoning_effort` for this model only.
    #[serde(
        default,
        alias = "reasoning-effort",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_effort: Option<String>,
    #[serde(
        default,
        alias = "context_limit",
        alias = "context-length",
        alias = "context-window",
        alias = "context_length",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_window: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NamedProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: NamedProviderType,
    pub base_url: String,
    pub api: Option<String>,
    pub auth: NamedProviderAuth,
    pub auth_header: Option<String>,
    /// Extra HTTP headers sent with every request to this provider.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub headers: std::collections::BTreeMap<String, String>,
    pub api_key_env: Option<String>,
    pub api_key: Option<String>,
    pub env_file: Option<String>,
    pub default_model: Option<String>,
    pub requires_api_key: Option<bool>,
    #[serde(default)]
    pub provider_routing: bool,
    #[serde(default)]
    pub model_catalog: bool,
    #[serde(default)]
    pub allow_provider_pinning: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<NamedProviderModelConfig>,
    /// Extra top-level JSON fields merged into every chat/completions request
    /// body sent to this provider. Lets users inject non-standard parameters
    /// some OpenAI-compatible backends require (e.g. NVIDIA NIM DeepSeek-V4
    /// needs `chat_template_kwargs = { thinking = true, reasoning_effort = "high" }`).
    /// Must be a JSON object; keys here override jcode-generated body fields.
    #[serde(default, alias = "extra-body", skip_serializing_if = "Option::is_none")]
    pub extra_body: Option<serde_json::Value>,
    /// Whether this endpoint accepts the DeepSeek-style top-level
    /// `reasoning_effort` request field (`/effort` support). When unset, jcode
    /// auto-detects it from the active model id (DeepSeek-family models
    /// support it regardless of which gateway serves them). Set `false` to
    /// suppress auto-detection for strict-schema endpoints.
    #[serde(
        default,
        alias = "supports-reasoning-effort",
        alias = "reasoning_effort",
        skip_serializing_if = "Option::is_none"
    )]
    pub supports_reasoning_effort: Option<bool>,
    /// Disable model-name based reasoning detection for this profile. Explicit
    /// provider/model capability settings continue to work.
    #[serde(default, alias = "disable-reasoning-heuristics")]
    pub disable_reasoning_heuristics: bool,
}

impl Default for NamedProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: NamedProviderType::OpenAiCompatible,
            base_url: String::new(),
            api: None,
            auth: NamedProviderAuth::Bearer,
            auth_header: None,
            headers: std::collections::BTreeMap::new(),
            api_key_env: None,
            api_key: None,
            env_file: None,
            default_model: None,
            requires_api_key: None,
            provider_routing: false,
            model_catalog: false,
            allow_provider_pinning: false,
            models: Vec::new(),
            extra_body: None,
            supports_reasoning_effort: None,
            disable_reasoning_heuristics: false,
        }
    }
}

/// Remembered trust decisions for external auth sources managed by other tools.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AuthConfig {
    /// External auth source ids that the user has approved jcode to read/use.
    pub trusted_external_sources: Vec<String>,
    /// Path-bound approvals for external auth sources managed by other tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_external_source_paths: Vec<String>,
}

/// Agent-specific model defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentsConfig {
    /// Optional default model override for spawned swarm/subagent sessions.
    ///
    /// Leave unset (or use `"inherit"` / `"coordinator"`) to have spawned swarm
    /// agents inherit the spawning coordinator's model. Set to a concrete model
    /// string to change the worker default. An explicit `model` in the swarm
    /// tool overrides this default for newly spawned workers.
    pub swarm_model: Option<String>,
    /// Optional default reasoning effort for spawned swarm/subagent sessions
    /// (`"low"`, `"medium"`, `"high"`, ...). Applied when a `swarm spawn`
    /// call does not pass an explicit `effort`. Leave unset to let workers
    /// inherit the provider-wide reasoning effort.
    pub swarm_effort: Option<String>,
    /// Root reasoning effort in light swarm mode. Unset or invalid means `max`.
    /// This does not change worker effort (`swarm_effort`).
    pub swarm_root_effort: Option<String>,
    /// Root reasoning effort in deep swarm mode. Unset or invalid means `max`.
    pub swarm_deep_root_effort: Option<String>,
    /// Default terminal mode for swarm-created agents.
    pub swarm_spawn_mode: SwarmSpawnMode,
    /// Maximum percentage (1-90) of the chat column height the inline swarm
    /// gallery band may occupy. Leave unset to use the built-in default (40%).
    /// Lower values keep more of the transcript visible; set near the minimum
    /// to effectively collapse the gallery to a thin strip.
    pub swarm_gallery_max_pct: Option<u8>,
    /// Layout of the inline swarm strip above the status line:
    /// `"vertical"` (default) lists one agent per row (session icon + status
    /// glyph + task), capped to a few lines; `"horizontal"` packs all agents
    /// as chips on a single row.
    #[serde(default)]
    pub swarm_strip_layout: SwarmStripLayout,
    /// Maximum number of live swarm worker agents in one swarm. This is the RAM
    /// safety budget for both recursive ad hoc spawning and deep-mode `run_plan`
    /// parallelism. Completed/stopped workers do not consume slots. Light mode
    /// still uses a smaller fixed fan-out. `0` disables this configurable guard,
    /// leaving only the absolute `MAX_SWARM_MEMBERS` hard cap.
    /// Env override: `JCODE_SWARM_MAX_CONCURRENT_AGENTS`.
    #[serde(default = "default_swarm_max_concurrent_agents")]
    pub swarm_max_concurrent_agents: usize,
}

fn default_swarm_max_concurrent_agents() -> usize {
    32
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            swarm_model: None,
            swarm_effort: None,
            swarm_root_effort: None,
            swarm_deep_root_effort: None,
            swarm_spawn_mode: SwarmSpawnMode::default(),
            swarm_gallery_max_pct: None,
            swarm_strip_layout: SwarmStripLayout::default(),
            swarm_max_concurrent_agents: default_swarm_max_concurrent_agents(),
        }
    }
}

impl AgentsConfig {
    /// Resolve a swarm mode's root effort without allowing orchestration
    /// sentinels to recurse into another mode. Unknown values preserve the
    /// historical maximum-effort behavior without invalidating other settings.
    pub fn root_effort_for_swarm(&self, deep: bool) -> &'static str {
        let configured = if deep {
            self.swarm_deep_root_effort.as_deref()
        } else {
            self.swarm_root_effort.as_deref()
        };
        let value = configured.unwrap_or("max").trim();
        ["none", "minimal", "low", "medium", "high", "xhigh", "max"]
            .into_iter()
            .find(|level| level.eq_ignore_ascii_case(value))
            .unwrap_or("max")
    }
}

/// How swarm-created agents should be spawned.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SwarmSpawnMode {
    /// Open a visible/headed terminal window. This was the historical default.
    Visible,
    /// Create the worker in-process without opening a terminal window.
    Headless,
    /// Like headless (no terminal window), but the coordinator renders a live
    /// inline gallery viewport of each worker's streaming output.
    #[default]
    Inline,
    /// Try visible first and fall back to headless if a window cannot be opened.
    Auto,
}

impl SwarmSpawnMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "visible" | "headed" => Some(Self::Visible),
            "headless" => Some(Self::Headless),
            "inline" => Some(Self::Inline),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    /// Canonical lowercase string for this mode (matches the config/env values).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Headless => "headless",
            Self::Inline => "inline",
            Self::Auto => "auto",
        }
    }
}

/// Layout of the inline swarm strip shown above the status line.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SwarmStripLayout {
    /// One agent per row: session icon + status glyph + task label, capped to
    /// a few lines with a `+N more` overflow marker.
    #[default]
    Vertical,
    /// All agents packed as chips on a single row (the historical layout).
    Horizontal,
}

impl SwarmStripLayout {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "vertical" | "list" => Some(Self::Vertical),
            "horizontal" | "chips" | "strip" => Some(Self::Horizontal),
            _ => None,
        }
    }

    /// Canonical lowercase string for this layout (matches config/env values).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
        }
    }
}

/// Terminal window/pane spawning configuration.
///
/// Without a `spawn_hook`, Unix clients inside tmux are opened in a right-side
/// pane by the built-in launcher. `JCODE_TERMINAL` explicitly selects a terminal
/// emulator instead.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TerminalConfig {
    /// External command that takes over headed session spawns (new terminal
    /// windows for swarm agents, resume-in-new-terminal, self-dev, restarts).
    ///
    /// When set, jcode runs `<spawn_hook> <jcode-binary> <args...>` instead of
    /// opening a terminal emulator itself, with `JCODE_SPAWN_*` metadata env
    /// vars describing the spawn (kind, session id, title, cwd, full command).
    /// This lets multiplexers and wrappers (tmux, kitty remote, zellij, herd
    /// runners, window managers) decide where and how the session appears.
    ///
    /// Example: `spawn_hook = "tmux new-window"` opens each headed spawn as a
    /// tmux window in the current server. If the hook fails to launch, jcode
    /// falls back to its built-in terminal detection.
    ///
    /// Env override: `JCODE_SPAWN_HOOK` (set empty to disable a config hook).
    pub spawn_hook: Option<String>,
    /// External command used to focus/raise an existing session window.
    ///
    /// When set, jcode runs the hook (instead of wmctrl/xdotool) whenever it
    /// wants to bring a session's window to the foreground, with
    /// `JCODE_FOCUS_SESSION_ID` and `JCODE_FOCUS_TITLE` env vars. Pair this
    /// with `spawn_hook` so wrappers that own placement (tmux, kitty remote,
    /// herd) also own focus (e.g. `tmux select-window`, Wayland compositor
    /// IPC like `niri msg`).
    ///
    /// Env override: `JCODE_FOCUS_HOOK` (set empty to disable a config hook).
    pub focus_hook: Option<String>,
    /// Terminal used for in-app session spawns.
    ///
    /// One of: `ghostty`, `iterm2`, `wezterm`, `warp`, `alacritty`, `vscode`,
    /// `terminal` (Apple Terminal). When set, this is the source of truth for
    /// which terminal jcode launches into and is preferred over the legacy
    /// `~/.jcode/preferred_terminal.json` file.
    ///
    /// macOS only; ignored on other platforms.
    pub preferred: Option<String>,
}

/// Lifecycle hooks: external commands jcode runs at well-defined points.
///
/// Hook commands are parsed shell-style (quotes work) but executed directly,
/// with `JCODE_HOOK_*` env vars describing the event (`JCODE_HOOK_EVENT`,
/// `JCODE_HOOK_SESSION_ID`, `JCODE_HOOK_CWD`, event-specific fields, and a
/// `JCODE_HOOK_PAYLOAD` JSON mirror). Hook processes get
/// `JCODE_HOOKS_DISABLED=1` so nested jcode invocations don't recurse.
///
/// All hooks except `pre_tool` are observers: detached, fire-and-forget,
/// failures only logged. `pre_tool` is a gate: jcode waits for it and exit
/// code 2 blocks the tool call (stderr becomes the error shown to the model);
/// exit 0 allows; anything else fails open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCommands(Vec<String>);

impl HookCommands {
    pub fn one(command: impl Into<String>) -> Self {
        Self(vec![command.into()])
    }

    pub fn many(commands: Vec<String>) -> Self {
        Self(commands)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    pub fn first(&self) -> Option<&str> {
        self.0.first().map(String::as_str)
    }
}

impl Serialize for HookCommands {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let [command] = self.0.as_slice() {
            command.serialize(serializer)
        } else {
            self.0.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for HookCommands {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            One(String),
            Many(Vec<String>),
        }

        Ok(match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(command) => Self::one(command),
            OneOrMany::Many(commands) => Self::many(commands),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HooksConfig {
    /// Runs when an agent turn begins (after the user message is added and
    /// before the model starts generating). Fires before the first `pre_tool`,
    /// so integrations can detect that the agent is actively working even while
    /// it is only thinking/streaming text. Fields: MODEL, SOURCE
    /// ("chat"/"resume"/"ambient"). Env override: JCODE_HOOK_TURN_START.
    pub turn_start: Option<HookCommands>,
    /// Runs when an agent turn completes.
    /// Fields: STATUS ("ok"/"error"), DURATION_MS, MODEL, LAST_ASSISTANT_TEXT.
    /// Env override: JCODE_HOOK_TURN_END.
    pub turn_end: Option<HookCommands>,
    /// Runs when a session becomes active (created or resumed).
    /// Fields: SOURCE ("create"/"resume").
    /// Env override: JCODE_HOOK_SESSION_START.
    pub session_start: Option<HookCommands>,
    /// Runs when a session closes normally.
    /// Env override: JCODE_HOOK_SESSION_END.
    pub session_end: Option<HookCommands>,
    /// Gate hook before each tool call. Receives TOOL_NAME and the tool input
    /// JSON on stdin (also truncated in TOOL_INPUT). Exit 0 allows, exit 2
    /// blocks (stderr is fed back to the model), anything else fails open.
    /// Env override: JCODE_HOOK_PRE_TOOL.
    pub pre_tool: Option<HookCommands>,
    /// Runs after each tool call completes.
    /// Fields: TOOL_NAME, STATUS ("ok"/"error"), DURATION_MS, OUTPUT_BYTES.
    /// Env override: JCODE_HOOK_POST_TOOL.
    pub post_tool: Option<HookCommands>,
    /// Max milliseconds to wait for the pre_tool gate before failing open
    /// (default: 5000). Env override: JCODE_HOOK_PRE_TOOL_TIMEOUT_MS.
    pub pre_tool_timeout_ms: u64,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            turn_start: None,
            turn_end: None,
            session_start: None,
            session_end: None,
            pre_tool: None,
            post_tool: None,
            pre_tool_timeout_ms: 5000,
        }
    }
}

/// Automatic end-of-turn code review configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AutoReviewConfig {
    /// Enable autoreview by default for new/resumed sessions (default: false)
    pub enabled: bool,
    /// Optional model override for autoreview reviewer sessions.
    pub model: Option<String>,
}

/// Automatic end-of-turn execution judging configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AutoJudgeConfig {
    /// Enable autojudge by default for new/resumed sessions (default: false)
    pub enabled: bool,
    /// Optional model override for autojudge sessions.
    pub model: Option<String>,
}

/// Keybinding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    /// Scroll up key (default: "ctrl+k")
    pub scroll_up: String,
    /// Scroll down key (default: "ctrl+j")
    pub scroll_down: String,
    /// Page up key (default: "alt+u")
    pub scroll_page_up: String,
    /// Page down key (default: "alt+d")
    pub scroll_page_down: String,
    /// Model switch next key (default: "ctrl+tab")
    pub model_switch_next: String,
    /// Model switch previous key (default: "ctrl+shift+tab")
    pub model_switch_prev: String,
    /// Accept the post-error fallback offer: switch to the next best
    /// model/auth-method and resend the failed turn (default: "ctrl+y").
    pub fallback_switch: String,
    /// Effort increase key (default: "cmd+right" on macOS, "alt+right" elsewhere)
    pub effort_increase: String,
    /// Effort decrease key (default: "cmd+left" on macOS, "alt+left" elsewhere)
    pub effort_decrease: String,
    /// Centered mode toggle key (default: "alt+c")
    pub centered_toggle: String,
    /// Scroll to previous prompt key (default: "ctrl+[")
    pub scroll_prompt_up: String,
    /// Scroll to next prompt key (default: "ctrl+]")
    pub scroll_prompt_down: String,
    /// Scroll bookmark toggle key (default: "ctrl+g")
    pub scroll_bookmark: String,
    /// Toggle auto-poke (default: "ctrl+p"). Set "" to disable.
    pub auto_poke_toggle: String,
    /// Scroll up fallback key (default: unset; Cmd+K moves up by prompt on macOS)
    pub scroll_up_fallback: String,
    /// Scroll down fallback key (default: unset; Cmd+J moves down by prompt on macOS)
    pub scroll_down_fallback: String,
    /// Workspace navigation left key (default: "alt+h")
    pub workspace_left: String,
    /// Workspace navigation down key (default: "alt+j")
    pub workspace_down: String,
    /// Workspace navigation up key (default: "alt+k")
    pub workspace_up: String,
    /// Workspace navigation right key (default: "alt+l")
    pub workspace_right: String,
    /// Toggle the side panel (default: "alt+m")
    pub side_panel_toggle: String,
    /// Toggle copy/selection mode (default: "alt+y")
    pub copy_selection_toggle: String,
    /// Toggle the diagram pane position (default: "alt+t")
    pub diagram_pane_toggle: String,
    /// Toggle typing scroll lock (default: "alt+s")
    pub typing_scroll_lock_toggle: String,
    /// Cycle inline diff display mode (default: "alt+g")
    pub diff_mode_cycle: String,
    /// Toggle the info widget (default: "alt+i")
    pub info_widget_toggle: String,
    /// Show/dismiss the session todo list as an inline card in the chat
    /// transcript (default: "alt+x")
    pub todo_card_toggle: String,
    /// Focus/unfocus the inline swarm panel for keyboard navigation (default:
    /// "alt+n"; alt+↑/↓ select, alt+o pops out, alt+shift+p opens the swarm
    /// prompt, esc exits). Active only when `agents.swarm_spawn_mode = "inline"`
    /// and the session manages swarm agents.
    pub swarm_panel_focus: String,
    /// Spawn a fresh jcode session in a new terminal window (default: unbound).
    /// Example: "alt+enter".
    pub new_terminal: String,
    /// Open the `/resume` session picker (default: "cmd+b" on macOS, "alt+r"
    /// elsewhere). Set "" to disable.
    pub open_resume: String,
    /// Session picker Enter action: "current-terminal" (default) or "new-terminal".
    /// Ctrl+Enter performs the alternate action.
    pub session_picker_enter: SessionPickerResumeAction,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        // Pull platform-appropriate defaults from the single source of truth in
        // `keybindings.rs`. This is where the macOS vs Linux split takes
        // effect: each field resolves to its own platform's default binding.
        let p = KeybindingPlatform::current();
        let get = |id: &str, fallback: &'static str| {
            default_binding(id, p).unwrap_or(fallback).to_string()
        };
        Self {
            scroll_up: get("scroll_up", "ctrl+k"),
            scroll_down: get("scroll_down", "ctrl+j"),
            scroll_page_up: get("scroll_page_up", "alt+u"),
            scroll_page_down: get("scroll_page_down", "alt+d"),
            model_switch_next: get("model_switch_next", "ctrl+tab"),
            model_switch_prev: get("model_switch_prev", "ctrl+shift+tab"),
            fallback_switch: get("fallback_switch", "ctrl+y"),
            effort_increase: get("effort_increase", "alt+right"),
            effort_decrease: get("effort_decrease", "alt+left"),
            centered_toggle: get("centered_toggle", "alt+c"),
            scroll_prompt_up: get("scroll_prompt_up", "ctrl+["),
            scroll_prompt_down: get("scroll_prompt_down", "ctrl+]"),
            scroll_bookmark: get("scroll_bookmark", "ctrl+g"),
            auto_poke_toggle: get("auto_poke_toggle", "ctrl+p"),
            scroll_up_fallback: get("scroll_up_fallback", ""),
            scroll_down_fallback: get("scroll_down_fallback", ""),
            workspace_left: get("workspace_left", "alt+h"),
            workspace_down: get("workspace_down", "alt+j"),
            workspace_up: get("workspace_up", "alt+k"),
            workspace_right: get("workspace_right", "alt+l"),
            side_panel_toggle: get("side_panel_toggle", "alt+m"),
            copy_selection_toggle: get("copy_selection_toggle", "alt+y"),
            diagram_pane_toggle: get("diagram_pane_toggle", "alt+t"),
            typing_scroll_lock_toggle: get("typing_scroll_lock_toggle", "alt+s"),
            diff_mode_cycle: get("diff_mode_cycle", "alt+g"),
            info_widget_toggle: get("info_widget_toggle", "alt+i"),
            todo_card_toggle: get("todo_card_toggle", "alt+x"),
            swarm_panel_focus: get("swarm_panel_focus", "alt+n"),
            new_terminal: get("new_terminal", ""),
            open_resume: get(
                "open_resume",
                if cfg!(target_os = "macos") {
                    "cmd+b"
                } else {
                    "alt+r"
                },
            ),
            session_picker_enter: SessionPickerResumeAction::CurrentTerminal,
        }
    }
}
/// Display/UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NativeScrollbarConfig {
    /// Show a native terminal scrollbar in the chat viewport (default: true)
    pub chat: bool,
    /// Show a native terminal scrollbar in the side panel (default: true)
    pub side_panel: bool,
}
impl Default for NativeScrollbarConfig {
    fn default() -> Self {
        Self {
            chat: true,
            side_panel: true,
        }
    }
}
fn default_true() -> bool {
    true
}

/// Runtime feature toggles
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureConfig {
    /// Check for and install jcode updates during startup (default: true).
    /// Set this to false for the persistent equivalent of `--no-update`.
    pub check_updates: bool,
    /// Enable swarm coordination features (default: true)
    pub swarm: bool,
    /// Default state of auto-poke (automatic follow-up when the model stops with
    /// incomplete todos). `/poke on` / `/poke off` still override this per session
    /// (default: true)
    pub auto_poke: bool,
    /// Inject timestamps into user messages and tool results sent to the model (default: true)
    pub message_timestamps: bool,
    /// Surface an in-chat system message whenever a request misses the KV cache
    /// for a harness-caused (avoidable) reason: the system prompt, tool set, or
    /// message prefix changed without the conversation legitimately growing.
    /// These should essentially never happen, so the notice acts as a loud alarm
    /// that something in the harness silently invalidated the prefix cache
    /// (default: true).
    pub kv_cache_miss_notices: bool,
    /// Update channel: "stable" (releases only) or "main" (latest commits)
    pub update_channel: UpdateChannel,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            check_updates: true,
            swarm: true,
            auto_poke: true,
            message_timestamps: true,
            kv_cache_miss_notices: true,
            update_channel: UpdateChannel::default(),
        }
    }
}

/// Search engine used by the websearch tool.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum WebSearchEngine {
    /// DuckDuckGo HTML search, no API key required.
    #[default]
    Duckduckgo,
    /// Bing search. Uses the Bing API when configured, otherwise Bing HTML search.
    Bing,
    /// SearXNG metasearch instance (JSON API). Requires `searxng_url` (or the
    /// `JCODE_SEARXNG_URL` env var) to point at a SearXNG instance. Useful on
    /// hosts where DuckDuckGo/Bing block the request via TLS fingerprinting.
    Searxng,
}

impl WebSearchEngine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Duckduckgo => "duckduckgo",
            Self::Bing => "bing",
            Self::Searxng => "searxng",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "duckduckgo" | "ddg" => Some(Self::Duckduckgo),
            "bing" => Some(Self::Bing),
            "searxng" | "searx" => Some(Self::Searxng),
            _ => None,
        }
    }
}

/// Configuration for the websearch tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSearchConfig {
    /// Preferred engine when the tool input does not specify one.
    pub engine: WebSearchEngine,
    /// Keyless HTML engines to try after the preferred engine fails.
    pub fallback_engines: Vec<WebSearchEngine>,
    /// Optional Bing API key for primary Bing searches. Fallback Bing uses keyless HTML search.
    pub bing_api_key: Option<String>,
    /// Environment variable containing the Bing API key.
    pub bing_api_key_env: String,
    /// Bing market, e.g. "en-US" or "zh-CN".
    pub bing_market: String,
    /// Base URL of a SearXNG instance (e.g. "https://searx.example.org"), used
    /// by the `searxng` engine. When empty, the `searxng_url_env` variable is
    /// consulted instead.
    pub searxng_url: Option<String>,
    /// Environment variable containing the SearXNG base URL.
    pub searxng_url_env: String,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            engine: WebSearchEngine::Duckduckgo,
            fallback_engines: vec![WebSearchEngine::Bing],
            bing_api_key: None,
            bing_api_key_env: "JCODE_BING_API_KEY".to_string(),
            bing_market: "en-US".to_string(),
            searxng_url: None,
            searxng_url_env: "JCODE_SEARXNG_URL".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    /// Default model to use (e.g. "claude-opus-4-8", "copilot:claude-opus-4.6")
    pub default_model: Option<String>,
    /// Default provider to use (claude|openai|copilot|openrouter)
    pub default_provider: Option<String>,
    /// Reasoning effort for OpenAI Responses API (none|minimal|low|medium|high|xhigh|max)
    pub openai_reasoning_effort: Option<String>,
    /// Reasoning effort for Anthropic Messages API output_config (none|low|medium|high|xhigh; max aliases to strongest supported)
    pub anthropic_reasoning_effort: Option<String>,
    /// OpenAI transport mode (auto|websocket|https)
    pub openai_transport: Option<String>,
    /// OpenAI service tier override (priority|flex)
    pub openai_service_tier: Option<String>,
    /// OpenAI native compaction mode: "auto", "explicit", or "off".
    pub openai_native_compaction_mode: String,
    /// Token threshold at which OpenAI auto native compaction should trigger.
    pub openai_native_compaction_threshold_tokens: usize,
    /// Preserve provider-native reasoning/thinking items for future-turn context when supported.
    pub preserve_reasoning_context: bool,
    /// How to handle cross-provider failover when the same input would be resent elsewhere.
    pub cross_provider_failover: CrossProviderFailoverMode,
    /// Whether jcode should automatically try another account on the same provider
    /// before falling back to a different provider.
    pub same_provider_account_failover: bool,
    /// Copilot premium request mode: "normal", "one", or "zero"
    /// "zero" means all requests are free (no premium requests consumed)
    pub copilot_premium: Option<String>,
    /// Pin the `gemini` provider to Code Assist OAuth even when a Gemini
    /// Developer API key (`gemini.env` / `GEMINI_API_KEY`) is present. Without
    /// this an API key silently wins and every turn bills per token on the
    /// key's project. `JCODE_GEMINI_FORCE_OAUTH` overrides this value.
    pub gemini_force_oauth: bool,
    /// Google Cloud project for Gemini Code Assist OAuth. Workspace accounts
    /// require one; without it every turn fails with "requires setting
    /// GOOGLE_CLOUD_PROJECT". `GOOGLE_CLOUD_PROJECT` (or its legacy `_ID`
    /// alias) overrides this value. Config values are never exported to env.
    pub gemini_project: Option<String>,
    /// When set (non-empty), /model only lists routes from these providers.
    /// Entries match provider labels ("openai", "anthropic", "copilot",
    /// "openrouter", ...), api methods ("claude-oauth",
    /// "openai-compatible:myprofile", ...), or openai-compatible profile ids
    /// ("myprofile"). The active model's routes always stay visible.
    pub model_picker_providers: Option<Vec<String>>,
    /// Max seconds to wait for streaming data before timing out a request with
    /// no data received. Base budget only: high reasoning efforts scale it up
    /// automatically (see `jcode_base::provider::stream_idle_timeout_for_effort`).
    /// Default: 180. Overridable via `JCODE_STREAM_IDLE_TIMEOUT_SECS`.
    pub stream_idle_timeout_secs: u64,
    /// Maximum request attempts for transient provider errors, including the
    /// initial attempt. Default: 8. Overridable via `JCODE_MAX_RETRIES`.
    pub max_retries: u32,
    /// Maximum exponential-backoff delay between transient-error retries.
    /// Default: 30 seconds. Overridable via `JCODE_RETRY_BACKOFF_CAP_SECS`.
    pub retry_backoff_cap_secs: u64,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default_model: None,
            default_provider: None,
            openai_reasoning_effort: Some("low".to_string()),
            anthropic_reasoning_effort: None,
            openai_transport: None,
            openai_service_tier: Some("priority".to_string()),
            openai_native_compaction_mode: "auto".to_string(),
            openai_native_compaction_threshold_tokens: 200_000,
            preserve_reasoning_context: true,
            cross_provider_failover: CrossProviderFailoverMode::Countdown,
            same_provider_account_failover: true,
            copilot_premium: None,
            gemini_force_oauth: false,
            gemini_project: None,
            model_picker_providers: None,
            stream_idle_timeout_secs: 180,
            max_retries: 8,
            retry_backoff_cap_secs: 30,
        }
    }
}

/// Desktop notification configuration for interactive sessions.
///
/// Local desktop notifications only, e.g. "agent finished a long turn".
/// Platform integrations (ntfy, email, chat bridges) are not part of core.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationsConfig {
    /// Send a desktop notification when an agent turn completes (default: true).
    /// Notifications fire only for long turns (see thresholds below) and, by
    /// default, only while the terminal window is unfocused.
    pub turn_complete: bool,
    /// Minimum turn duration, in seconds, before a completed turn notifies
    /// (default: 120).
    pub turn_complete_min_secs: u64,
    /// Lower duration threshold, in seconds, used when the session has todos
    /// recorded, since todos indicate longer task-style work (default: 30).
    pub turn_complete_todo_min_secs: u64,
    /// Only notify while the terminal window is unfocused (default: true).
    /// Requires a terminal that reports focus events (most modern terminals).
    pub turn_complete_only_when_unfocused: bool,
    /// macOS Notification Center sound name played on turn completion
    /// (e.g. "Glass", "Ping", "Hero"). Empty string disables the sound.
    /// Ignored on non-macOS platforms. Default: "Glass".
    pub turn_complete_sound: String,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            turn_complete: true,
            turn_complete_min_secs: 120,
            turn_complete_todo_min_secs: 30,
            turn_complete_only_when_unfocused: true,
            turn_complete_sound: "Glass".to_string(),
        }
    }
}

/// Power-management configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerConfig {
    /// Prevent automatic system sleep while any jcode session is actively
    /// streaming/processing, and ask logind to block lid-switch suspend. The
    /// display is still allowed to sleep. Default: true.
    ///
    /// Honored by the shared `jcode serve` daemon. The `JCODE_DISABLE_POWER_INHIBIT`
    /// environment variable forces this off regardless of the config value.
    pub prevent_sleep_while_streaming: bool,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            prevent_sleep_while_streaming: true,
        }
    }
}

#[cfg(test)]
mod reasoning_display_defaults_tests {
    use super::*;

    #[test]
    fn explicit_reasoning_display_is_distinguishable_from_the_legacy_fallback() {
        // Front-ends (the desktop) apply their own default only when the user
        // has not chosen one, so this flag must not be true just because
        // `show_thinking` happens to be set.
        let mut display = DisplayConfig {
            reasoning_display: None,
            show_thinking: true,
            ..DisplayConfig::default()
        };
        assert!(!display.has_explicit_reasoning_display());
        assert_eq!(display.reasoning_display(), ReasoningDisplayMode::Full);

        display.set_reasoning_display(ReasoningDisplayMode::Current);
        assert!(display.has_explicit_reasoning_display());
        assert_eq!(display.reasoning_display(), ReasoningDisplayMode::Current);
        assert!(
            display.show_thinking,
            "any active display mode must keep reasoning requested from the provider"
        );

        display.set_reasoning_display(ReasoningDisplayMode::Off);
        assert!(display.has_explicit_reasoning_display());
        assert!(!display.show_thinking);
    }
}
