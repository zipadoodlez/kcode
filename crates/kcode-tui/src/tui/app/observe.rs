use super::App;
use crate::message::ToolCall;
use crate::side_panel::SidePanelPage;

pub(super) const OBSERVE_PAGE_ID: &str = "observe";
const OBSERVE_PAGE_TITLE: &str = "Observe";

/// State behind the observe side-panel page: a live log of the agent's own tool
/// activity. One home, so the refresh and page logic reads one struct.
#[derive(Default)]
pub(super) struct Observe {
    pub(super) enabled: bool,
    pub(super) page_markdown: String,
    pub(super) page_updated_at_ms: u64,
}

impl Observe {
    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn page(&self) -> SidePanelPage {
        SidePanelPage::ephemeral_markdown(
            OBSERVE_PAGE_ID,
            OBSERVE_PAGE_TITLE,
            "observe://latest-context",
            if self.page_markdown.trim().is_empty() {
                observe_placeholder_markdown()
            } else {
                self.page_markdown.clone()
            },
            self.page_updated_at_ms.max(1),
        )
    }
}

impl App {
    fn should_observe_tool(&self, tool_call: &ToolCall) -> bool {
        self.observe.enabled && !is_noise_tool(&tool_call.name)
    }

    pub(super) fn set_observe_mode_enabled(&mut self, enabled: bool, focus: bool) {
        self.observe.enabled = enabled;
        let page = if enabled {
            if self.observe.page_markdown.trim().is_empty() {
                self.observe.page_markdown = observe_placeholder_markdown();
                self.observe.page_updated_at_ms = now_ms();
            }
            Some(self.observe.page())
        } else {
            None
        };
        self.apply_mirror_page(OBSERVE_PAGE_ID, page, focus);
    }

    pub(super) fn observe_tool_call(&mut self, tool_call: &ToolCall) {
        if !self.should_observe_tool(tool_call) {
            return;
        }
        self.observe.page_markdown = build_observe_tool_call_markdown(tool_call);
        self.observe.page_updated_at_ms = now_ms();
        self.refresh_observe_page();
    }

    pub(super) fn observe_tool_result(
        &mut self,
        tool_call: &ToolCall,
        output: &str,
        is_error: bool,
        title: Option<&str>,
    ) {
        if !self.should_observe_tool(tool_call) {
            return;
        }
        self.observe.page_markdown =
            build_observe_tool_result_markdown(tool_call, output, is_error, title);
        self.observe.page_updated_at_ms = now_ms();
        self.refresh_observe_page();
    }

    /// React to a completed tool by forcing the info-widget caches it may have
    /// dirtied to refetch on the next frame, instead of waiting out their TTLs.
    ///
    /// The info widget's git-status and todos panels are stale-while-revalidate
    /// caches (5s / 1s TTL). The agent's own tools are exactly what mutate those
    /// underlying sources, so without an explicit nudge the widget would show the
    /// repo/todo state from *before* the tool ran for a full TTL. This keeps the
    /// SWR perf benefit (no synchronous git/disk work on the render path) while
    /// making self-inflicted staleness self-correct immediately: the next read
    /// returns the last value and kicks a background refresh.
    ///
    /// Called unconditionally on every tool completion (local and remote paths),
    /// independent of observe mode. Tool names are matched leniently because the
    /// same logical tool surfaces under several aliases (e.g. `bash`/`shell`,
    /// `write`/`write_file`).
    pub(super) fn note_tool_completed(&mut self, tool_call: &ToolCall, is_error: bool) {
        if is_error {
            // A failed tool did not change the working tree or todos.
            return;
        }

        let name = tool_call.name.to_ascii_lowercase();

        // Any filesystem- or repo-mutating tool can change git status (branch,
        // staged/modified/untracked counts). `batch` can wrap any of these, so
        // treat it as potentially mutating too.
        let mutates_repo = matches!(
            name.as_str(),
            "bash"
                | "shell"
                | "shell_exec"
                | "write"
                | "write_file"
                | "edit"
                | "edit_file"
                | "multiedit"
                | "patch"
                | "apply_patch"
                | "batch"
                | "run_shell"
        );
        if mutates_repo {
            super::helpers::invalidate_git_info_cache();
        }

        // The todo tool rewrites the work list, which the session title reads.
        if name == "todo" || name == "todowrite" || name == "todo_write" {
            self.update_terminal_title();
        }
    }

    fn refresh_observe_page(&mut self) {
        if !self.observe.enabled {
            return;
        }

        let focus_observe = self.side_panel.focused_page_id.as_deref() == Some(OBSERVE_PAGE_ID);
        let snapshot = self.decorate_side_panel_with_page(
            self.snapshot_without_page(OBSERVE_PAGE_ID),
            self.observe.page(),
            focus_observe,
        );
        self.apply_side_panel_snapshot(snapshot);
    }
}

fn observe_placeholder_markdown() -> String {
    "# Observe\n\nWaiting for the next tool call or tool result.\n\nThis page is transient and only shows the **latest** useful context-bearing tool activity. UI/bookkeeping tools like `side_panel`, `goal`, and todo reads/writes are skipped. It is not persisted to disk.\n".to_string()
}

fn build_observe_tool_call_markdown(tool_call: &ToolCall) -> String {
    format!(
        "# Observe\n\nLatest tool call emitted by the model.\n\n- Tool: `{}`\n- Status: running\n\n## Tool input\n{}\n",
        tool_call.name,
        crate::side_panel::fenced_block("json", &pretty_json(&tool_call.input))
    )
}

fn build_observe_tool_result_markdown(
    tool_call: &ToolCall,
    output: &str,
    is_error: bool,
    title: Option<&str>,
) -> String {
    let token_count = crate::util::estimate_tokens(output);
    let token_label = crate::util::format_approx_token_count(token_count);
    let output_chars = crate::util::format_number(output.len());
    // Keep these severity badges ASCII-only. Emoji/variation-selector glyphs
    // like ⚠️ and 🔴 are prone to width mismatches in terminal emulators and can
    // leave stale cells behind when the observe pane repaints.
    let size_note = match crate::util::approx_tool_output_token_severity(token_count) {
        crate::util::ApproxTokenSeverity::Normal => None,
        crate::util::ApproxTokenSeverity::Warning => Some(" [large]"),
        crate::util::ApproxTokenSeverity::Danger => Some(" [very large]"),
    };
    let mut markdown = format!(
        "# Observe\n\nLatest tool result added to context.\n\n- Tool: `{}`\n- Status: {}\n- Returned to context: `{}` · `{} chars`{}\n",
        tool_call.name,
        if is_error { "error" } else { "completed" },
        token_label,
        output_chars,
        size_note.unwrap_or("")
    );
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        markdown.push_str(&format!("- Title: `{}`\n", title.trim()));
    }
    markdown.push_str(&format!(
        "\n## Tool input\n{}\n\n## Tool output\n{}\n",
        crate::side_panel::fenced_block("json", &pretty_json(&tool_call.input)),
        crate::side_panel::fenced_block("text", if output.is_empty() { "(empty)" } else { output })
    ));
    markdown
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn is_noise_tool(name: &str) -> bool {
    matches!(
        name,
        "side_panel" | "panel" | "goal" | "todo" | "todoread" | "todowrite"
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dur| dur.as_millis() as u64)
        .unwrap_or(0)
}
