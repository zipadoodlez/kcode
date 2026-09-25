use super::*;
use jcode_tui_style::theme::file_link_color;
mod ui_pinned_table;
use ui_pinned_table::is_rendered_table_line;

#[path = "ui_pinned_utils.rs"]
mod util_support;
#[cfg(test)]
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use util_support::{estimate_side_panel_pane_area, lru_touch, side_panel_content_signature};

const SIDE_PANEL_HEADER_HEIGHT: u16 = 1;

fn side_panel_border_style(focused: bool) -> Style {
    let border_color = if focused { tool_color() } else { dim_color() };
    Style::default().fg(border_color)
}

fn side_panel_inner(area: Rect) -> Rect {
    ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::LEFT)
        .inner(area)
}

fn side_panel_content_area(area: Rect) -> Option<Rect> {
    let inner = side_panel_inner(area);
    if inner.width == 0 || inner.height <= SIDE_PANEL_HEADER_HEIGHT {
        return None;
    }

    Some(Rect {
        x: inner.x,
        y: inner.y + SIDE_PANEL_HEADER_HEIGHT,
        width: inner.width,
        height: inner.height - SIDE_PANEL_HEADER_HEIGHT,
    })
}

#[path = "ui_pinned_selection.rs"]
mod selection_support;
use selection_support::apply_side_selection_highlight;

enum PinnedContentEntry {
    Diff {
        file_path: String,
        lines: Vec<ParsedDiffLine>,
        additions: usize,
        deletions: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PinnedCacheKey {
    messages_version: u64,
}

#[derive(Default)]
struct PinnedCacheState {
    key: Option<PinnedCacheKey>,
    entries: Vec<PinnedContentEntry>,
    rendered_lines: Option<PinnedRenderedCache>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SidePanelMarkdownKey {
    page_id: String,
    content_signature: u64,
    inner_width: u16,
    centered: bool,
}

#[derive(Default)]
struct SidePanelMarkdownCacheState {
    entries: HashMap<SidePanelMarkdownKey, RenderedSidePanelMarkdown>,
    order: VecDeque<SidePanelMarkdownKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SidePanelRenderKey {
    page_id: String,
    content_signature: u64,
    inner_width: u16,
    inner_height: u16,
    centered: bool,
}

#[derive(Default)]
struct SidePanelRenderCacheState {
    entries: HashMap<SidePanelRenderKey, PinnedRenderedCache>,
    order: VecDeque<SidePanelRenderKey>,
}

/// Side-panel render cache statistics, surfaced by the debug socket and the
/// visual-debug capture.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SidePanelDebugStats {
    pub render_cache_hits: u64,
    pub render_cache_misses: u64,
    pub markdown_cache_hits: u64,
    pub markdown_cache_misses: u64,
    pub markdown_cache_entries: usize,
    pub render_cache_entries: usize,
}

#[derive(Default)]
struct SidePanelDebugState {
    stats: SidePanelDebugStats,
}

#[derive(Clone)]
struct RenderedSidePanelMarkdown {
    rendered_markdown: Vec<Line<'static>>,
}

#[derive(Clone)]
struct PinnedRenderedCache {
    inner_width: u16,
    line_wrap: bool,
    lines: Vec<Line<'static>>,
    wrapped_plain_lines: std::sync::Arc<Vec<String>>,
    wrapped_copy_offsets: std::sync::Arc<Vec<usize>>,
    raw_plain_lines: std::sync::Arc<Vec<String>>,
    wrapped_line_map: std::sync::Arc<Vec<WrappedLineMap>>,
    left_margins: Vec<u16>,
}

fn estimate_lines_bytes(lines: &[Line<'static>]) -> usize {
    lines
        .iter()
        .map(|line| {
            std::mem::size_of::<Line<'static>>()
                + line.spans.capacity() * std::mem::size_of::<Span<'static>>()
                + line
                    .spans
                    .iter()
                    .map(|span| span.content.len())
                    .sum::<usize>()
        })
        .sum()
}

fn estimate_arc_string_vec_bytes(values: &std::sync::Arc<Vec<String>>) -> usize {
    std::mem::size_of::<Vec<String>>()
        + values.capacity() * std::mem::size_of::<String>()
        + values.iter().map(|value| value.capacity()).sum::<usize>()
}

fn estimate_arc_usize_vec_bytes(values: &std::sync::Arc<Vec<usize>>) -> usize {
    std::mem::size_of::<Vec<usize>>() + values.capacity() * std::mem::size_of::<usize>()
}

fn estimate_arc_wrapped_line_map_bytes(values: &std::sync::Arc<Vec<WrappedLineMap>>) -> usize {
    std::mem::size_of::<Vec<WrappedLineMap>>()
        + values.capacity() * std::mem::size_of::<WrappedLineMap>()
}

fn estimate_pinned_rendered_cache_bytes(cache: &PinnedRenderedCache) -> usize {
    estimate_lines_bytes(&cache.lines)
        + estimate_arc_string_vec_bytes(&cache.wrapped_plain_lines)
        + estimate_arc_usize_vec_bytes(&cache.wrapped_copy_offsets)
        + estimate_arc_string_vec_bytes(&cache.raw_plain_lines)
        + estimate_arc_wrapped_line_map_bytes(&cache.wrapped_line_map)
        + cache.left_margins.capacity() * std::mem::size_of::<u16>()
}

fn estimate_rendered_side_panel_markdown_bytes(value: &RenderedSidePanelMarkdown) -> usize {
    estimate_lines_bytes(&value.rendered_markdown)
}

fn estimate_pinned_content_entry_bytes(entry: &PinnedContentEntry) -> usize {
    match entry {
        PinnedContentEntry::Diff {
            file_path, lines, ..
        } => {
            file_path.capacity()
                + lines.capacity() * std::mem::size_of::<crate::tui::ui_diff::ParsedDiffLine>()
                + lines
                    .iter()
                    .map(|line| line.prefix.capacity() + line.content.capacity())
                    .sum::<usize>()
        }
    }
}

fn estimate_side_panel_markdown_key_bytes(key: &SidePanelMarkdownKey) -> usize {
    key.page_id.capacity()
}

fn estimate_side_panel_render_key_bytes(key: &SidePanelRenderKey) -> usize {
    key.page_id.capacity()
}

pub(crate) fn debug_memory_profile() -> serde_json::Value {
    let (pinned_entries_count, pinned_entries_bytes, pinned_rendered_lines_bytes) = {
        let cache = pinned_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entries_bytes = cache
            .entries
            .iter()
            .map(estimate_pinned_content_entry_bytes)
            .sum::<usize>()
            + cache.entries.capacity() * std::mem::size_of::<PinnedContentEntry>();
        let rendered_lines_bytes = cache
            .rendered_lines
            .as_ref()
            .map(estimate_pinned_rendered_cache_bytes)
            .unwrap_or(0);
        (cache.entries.len(), entries_bytes, rendered_lines_bytes)
    };

    let (markdown_cache_entries_count, markdown_cache_bytes, markdown_cache_key_bytes) =
        with_side_panel_markdown_cache(|cache| {
            let entry_bytes = cache
                .entries
                .values()
                .map(estimate_rendered_side_panel_markdown_bytes)
                .sum::<usize>();
            let key_bytes = cache
                .entries
                .keys()
                .map(estimate_side_panel_markdown_key_bytes)
                .sum::<usize>()
                + cache
                    .order
                    .iter()
                    .map(estimate_side_panel_markdown_key_bytes)
                    .sum::<usize>();
            (cache.entries.len(), entry_bytes, key_bytes)
        });

    let (render_cache_entries_count, render_cache_bytes, render_cache_key_bytes) =
        with_side_panel_render_cache(|cache| {
            let entry_bytes = cache
                .entries
                .values()
                .map(estimate_pinned_rendered_cache_bytes)
                .sum::<usize>();
            let key_bytes = cache
                .entries
                .keys()
                .map(estimate_side_panel_render_key_bytes)
                .sum::<usize>()
                + cache
                    .order
                    .iter()
                    .map(estimate_side_panel_render_key_bytes)
                    .sum::<usize>();
            (cache.entries.len(), entry_bytes, key_bytes)
        });

    serde_json::json!({
        "pinned_cache": {
            "entries_count": pinned_entries_count,
            "entries_bytes": pinned_entries_bytes,
            "rendered_lines_bytes": pinned_rendered_lines_bytes,
        },
        "side_panel_markdown_cache": {
            "entries_count": markdown_cache_entries_count,
            "entries_bytes": markdown_cache_bytes,
            "key_bytes": markdown_cache_key_bytes,
        },
        "side_panel_render_cache": {
            "entries_count": render_cache_entries_count,
            "entries_bytes": render_cache_bytes,
            "key_bytes": render_cache_key_bytes,
        },
        "total_estimate_bytes": pinned_entries_bytes
            + pinned_rendered_lines_bytes
            + markdown_cache_bytes
            + markdown_cache_key_bytes
            + render_cache_bytes
            + render_cache_key_bytes,
    })
}

type SidePaneSnapshotCache = (
    std::sync::Arc<Vec<String>>,
    std::sync::Arc<Vec<usize>>,
    std::sync::Arc<Vec<String>>,
    std::sync::Arc<Vec<WrappedLineMap>>,
    Vec<u16>,
);

fn build_side_pane_snapshot_cache(
    lines: &[Line<'static>],
    inner_width: u16,
) -> SidePaneSnapshotCache {
    let plain_lines: Vec<String> = lines.iter().map(super::line_plain_text).collect();
    let wrapped_line_map: Vec<WrappedLineMap> = plain_lines
        .iter()
        .enumerate()
        .map(|(raw_line, text)| WrappedLineMap {
            raw_line,
            start_col: 0,
            end_col: unicode_width::UnicodeWidthStr::width(text.as_str()),
        })
        .collect();
    let copy_offsets = vec![0; plain_lines.len()];
    let left_margins = line_left_margins_for_area(lines, inner_width);
    let plain_lines = std::sync::Arc::new(plain_lines);
    (
        plain_lines.clone(),
        std::sync::Arc::new(copy_offsets),
        plain_lines,
        std::sync::Arc::new(wrapped_line_map),
        left_margins,
    )
}
static PINNED_CACHE: OnceLock<Mutex<PinnedCacheState>> = OnceLock::new();
#[cfg(not(test))]
static SIDE_PANEL_MARKDOWN_CACHE: OnceLock<Mutex<SidePanelMarkdownCacheState>> = OnceLock::new();
#[cfg(not(test))]
static SIDE_PANEL_RENDER_CACHE: OnceLock<Mutex<SidePanelRenderCacheState>> = OnceLock::new();
#[cfg(not(test))]
static SIDE_PANEL_DEBUG: OnceLock<Mutex<SidePanelDebugState>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static TEST_SIDE_PANEL_MARKDOWN_CACHE: RefCell<SidePanelMarkdownCacheState> = RefCell::new(SidePanelMarkdownCacheState::default());
    static TEST_SIDE_PANEL_RENDER_CACHE: RefCell<SidePanelRenderCacheState> = RefCell::new(SidePanelRenderCacheState::default());
    static TEST_SIDE_PANEL_DEBUG: RefCell<SidePanelDebugState> = RefCell::new(SidePanelDebugState::default());
}

const SIDE_PANEL_MARKDOWN_CACHE_LIMIT: usize = 12;
const SIDE_PANEL_RENDER_CACHE_LIMIT: usize = 12;

fn pinned_cache() -> &'static Mutex<PinnedCacheState> {
    PINNED_CACHE.get_or_init(|| Mutex::new(PinnedCacheState::default()))
}

#[cfg(not(test))]
fn side_panel_markdown_cache() -> &'static Mutex<SidePanelMarkdownCacheState> {
    SIDE_PANEL_MARKDOWN_CACHE.get_or_init(|| Mutex::new(SidePanelMarkdownCacheState::default()))
}

#[cfg(not(test))]
fn side_panel_render_cache() -> &'static Mutex<SidePanelRenderCacheState> {
    SIDE_PANEL_RENDER_CACHE.get_or_init(|| Mutex::new(SidePanelRenderCacheState::default()))
}

#[cfg(not(test))]
fn side_panel_debug() -> &'static Mutex<SidePanelDebugState> {
    SIDE_PANEL_DEBUG.get_or_init(|| Mutex::new(SidePanelDebugState::default()))
}

fn with_side_panel_markdown_cache<R>(f: impl FnOnce(&SidePanelMarkdownCacheState) -> R) -> R {
    #[cfg(test)]
    {
        return TEST_SIDE_PANEL_MARKDOWN_CACHE.with(|state| f(&state.borrow()));
    }
    #[cfg(not(test))]
    {
        let state = side_panel_markdown_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&state)
    }
}

fn with_side_panel_markdown_cache_mut<R>(
    f: impl FnOnce(&mut SidePanelMarkdownCacheState) -> R,
) -> R {
    #[cfg(test)]
    {
        return TEST_SIDE_PANEL_MARKDOWN_CACHE.with(|state| f(&mut state.borrow_mut()));
    }
    #[cfg(not(test))]
    {
        let mut state = side_panel_markdown_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut state)
    }
}

fn with_side_panel_render_cache<R>(f: impl FnOnce(&SidePanelRenderCacheState) -> R) -> R {
    #[cfg(test)]
    {
        return TEST_SIDE_PANEL_RENDER_CACHE.with(|state| f(&state.borrow()));
    }
    #[cfg(not(test))]
    {
        let state = side_panel_render_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&state)
    }
}

fn with_side_panel_render_cache_mut<R>(f: impl FnOnce(&mut SidePanelRenderCacheState) -> R) -> R {
    #[cfg(test)]
    {
        return TEST_SIDE_PANEL_RENDER_CACHE.with(|state| f(&mut state.borrow_mut()));
    }
    #[cfg(not(test))]
    {
        let mut state = side_panel_render_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut state)
    }
}

fn with_side_panel_debug<R>(f: impl FnOnce(&SidePanelDebugState) -> R) -> R {
    #[cfg(test)]
    {
        return TEST_SIDE_PANEL_DEBUG.with(|state| f(&state.borrow()));
    }
    #[cfg(not(test))]
    {
        let state = side_panel_debug()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&state)
    }
}

fn with_side_panel_debug_mut<R>(f: impl FnOnce(&mut SidePanelDebugState) -> R) -> R {
    #[cfg(test)]
    {
        return TEST_SIDE_PANEL_DEBUG.with(|state| f(&mut state.borrow_mut()));
    }
    #[cfg(not(test))]
    {
        let mut state = side_panel_debug()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut state)
    }
}

pub(crate) fn side_panel_debug_stats() -> SidePanelDebugStats {
    let mut stats = with_side_panel_debug(|state| state.stats.clone());
    stats.markdown_cache_entries = with_side_panel_markdown_cache(|cache| cache.entries.len());
    stats.render_cache_entries = with_side_panel_render_cache(|cache| cache.entries.len());
    stats
}

pub(crate) fn side_panel_debug_json() -> Option<serde_json::Value> {
    let stats = side_panel_debug_stats();
    serde_json::to_value(serde_json::json!({ "stats": stats })).ok()
}

pub(crate) fn reset_side_panel_debug_stats() {
    with_side_panel_debug_mut(|debug| {
        debug.stats = SidePanelDebugStats::default();
    });
}

pub(crate) fn clear_side_panel_render_caches() {
    with_side_panel_markdown_cache_mut(|cache| {
        *cache = SidePanelMarkdownCacheState::default();
    });
    with_side_panel_render_cache_mut(|cache| {
        *cache = SidePanelRenderCacheState::default();
    });
}

pub(crate) fn prewarm_focused_side_panel(
    snapshot: &crate::side_panel::SidePanelSnapshot,
    terminal_width: u16,
    terminal_height: u16,
    ratio_percent: u8,
    centered: bool,
) -> bool {
    let Some(page) = snapshot.focused_page() else {
        return false;
    };
    let Some(area) = estimate_side_panel_pane_area(terminal_width, terminal_height, ratio_percent)
    else {
        return false;
    };
    let Some(inner) = side_panel_content_area(area) else {
        return false;
    };
    if inner.width == 0 || inner.height == 0 {
        return false;
    }
    let _ = render_side_panel_markdown_cached(page, inner, centered);
    true
}

/// Collect the pinned file-diff entries used by the right-hand pane.
///
/// Inline images render in the transcript now. Keeping image payloads out of
/// this frame-level probe is important because `TuiState::side_pane_images()`
/// may materialize and clone multi-megabyte base64 strings.
pub(super) fn collect_pinned_diffs_cached(
    messages: &[DisplayMessage],
    messages_version: u64,
) -> bool {
    let key = PinnedCacheKey { messages_version };

    let mut cache = match pinned_cache().lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };

    if cache.key.as_ref() == Some(&key) {
        return !cache.entries.is_empty();
    }

    let entries = collect_pinned_content(messages);
    let has_entries = !entries.is_empty();
    cache.key = Some(key);
    cache.entries = entries;
    cache.rendered_lines = None;
    has_entries
}

fn collect_pinned_content(messages: &[DisplayMessage]) -> Vec<PinnedContentEntry> {
    let mut entries = Vec::new();

    for msg in messages {
        if msg.role != "tool" {
            continue;
        }
        let Some(ref tc) = msg.tool_data else {
            continue;
        };

        if !tools_ui::is_edit_tool_name(&tc.name) {
            continue;
        }

        let file_path = tc
            .input
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                tc.input
                    .get("patch_text")
                    .and_then(|v| v.as_str())
                    .and_then(|patch_text| match tools_ui::canonical_tool_name(&tc.name) {
                        "apply_patch" => tools_ui::extract_apply_patch_primary_file(patch_text),
                        "patch" => tools_ui::extract_unified_patch_primary_file(patch_text),
                        _ => None,
                    })
            })
            .unwrap_or_else(|| "unknown".to_string());

        let change_lines = {
            let from_content = collect_diff_lines(&msg.content);
            if !from_content.is_empty() {
                from_content
            } else {
                generate_diff_lines_from_tool_input(tc)
            }
        };
        if change_lines.is_empty() {
            continue;
        }

        let additions = change_lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Add)
            .count();
        let deletions = change_lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Del)
            .count();

        entries.push(PinnedContentEntry::Diff {
            file_path,
            lines: change_lines,
            additions,
            deletions,
        });
    }
    entries
}

pub(super) fn draw_pinned_content_cached(
    frame: &mut Frame,
    area: Rect,
    app: &dyn TuiState,
    scroll: usize,
    line_wrap: bool,
    focused: bool,
) {
    use ratatui::widgets::{Paragraph, Wrap};

    if area.width < 10 || area.height < 3 {
        return;
    }

    let mut cache = match pinned_cache().lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };

    if cache.entries.is_empty() {
        return;
    }

    let entries = &cache.entries;
    let total_diffs = entries
        .iter()
        .filter(|e| matches!(e, PinnedContentEntry::Diff { .. }))
        .count();
    let total_additions: usize = entries
        .iter()
        .map(|e| match e {
            PinnedContentEntry::Diff { additions, .. } => *additions,
        })
        .sum();
    let total_deletions: usize = entries
        .iter()
        .map(|e| match e {
            PinnedContentEntry::Diff { deletions, .. } => *deletions,
        })
        .sum();

    let mut title_parts = vec![Span::styled(" side ", Style::default().fg(tool_color()))];
    title_parts.push(Span::styled(
        "Pinned",
        Style::default()
            .fg(file_link_color())
            .add_modifier(ratatui::style::Modifier::BOLD),
    ));
    title_parts.push(Span::styled(" ", Style::default().fg(dim_color())));
    if total_diffs > 0 {
        title_parts.push(Span::styled(
            format!("+{}", total_additions),
            Style::default().fg(diff_add_color()),
        ));
        title_parts.push(Span::styled(" ", Style::default().fg(dim_color())));
        title_parts.push(Span::styled(
            format!("-{}", total_deletions),
            Style::default().fg(diff_del_color()),
        ));
        title_parts.push(Span::styled(
            format!(" {}f", total_diffs),
            Style::default().fg(dim_color()),
        ));
    }
    title_parts.push(Span::styled(
        " ⇧Tab hide ".to_string(),
        Style::default().fg(dim_color()),
    ));
    let border_style = side_panel_border_style(focused);
    let Some(inner) =
        super::draw_right_rail_chrome(frame, area, Line::from(title_parts), border_style)
    else {
        return;
    };

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let needs_rebuild = match &cache.rendered_lines {
        Some(rendered) => rendered.inner_width != inner.width || rendered.line_wrap != line_wrap,
        None => true,
    };

    if needs_rebuild {
        let mut text_lines: Vec<Line<'static>> = Vec::new();

        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                text_lines.push(Line::from(""));
            }

            match entry {
                PinnedContentEntry::Diff {
                    file_path,
                    lines: diff_lines,
                    additions,
                    deletions,
                } => {
                    let short_path = file_path
                        .rsplit('/')
                        .take(2)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("/");

                    let file_ext = std::path::Path::new(file_path)
                        .extension()
                        .and_then(|e| e.to_str());

                    text_lines.push(Line::from(vec![
                        Span::styled("── ", Style::default().fg(dim_color())),
                        Span::styled(
                            short_path,
                            Style::default()
                                .fg(file_link_color())
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        ),
                        Span::styled(" (", Style::default().fg(dim_color())),
                        Span::styled(
                            format!("+{}", additions),
                            Style::default().fg(diff_add_color()),
                        ),
                        Span::styled(" ", Style::default().fg(dim_color())),
                        Span::styled(
                            format!("-{}", deletions),
                            Style::default().fg(diff_del_color()),
                        ),
                        Span::styled(")", Style::default().fg(dim_color())),
                    ]));

                    for line in diff_lines {
                        let base_color = if line.kind == DiffLineKind::Add {
                            diff_add_color()
                        } else {
                            diff_del_color()
                        };

                        let mut spans: Vec<Span<'static>> = vec![Span::styled(
                            line.prefix.clone(),
                            Style::default().fg(base_color),
                        )];

                        if !line.content.is_empty() {
                            let highlighted =
                                markdown::highlight_line(line.content.as_str(), file_ext);
                            for span in highlighted {
                                let tinted = tint_span_with_diff_color(span, base_color);
                                spans.push(tinted);
                            }
                        }

                        text_lines.push(Line::from(spans));
                    }
                }
            }
        }

        if text_lines.is_empty() {
            text_lines.push(Line::from(Span::styled(
                "No content yet",
                Style::default().fg(dim_color()),
            )));
        }

        let (
            wrapped_plain_lines,
            wrapped_copy_offsets,
            raw_plain_lines,
            wrapped_line_map,
            left_margins,
        ) = build_side_pane_snapshot_cache(&text_lines, inner.width);

        cache.rendered_lines = Some(PinnedRenderedCache {
            inner_width: inner.width,
            line_wrap,
            lines: text_lines,
            wrapped_plain_lines,
            wrapped_copy_offsets,
            raw_plain_lines,
            wrapped_line_map,
            left_margins,
        });
    }

    let Some(rendered) = cache.rendered_lines.as_ref() else {
        return;
    };
    let total_lines = rendered.lines.len();
    super::set_pinned_pane_total_lines(total_lines);

    let max_scroll = total_lines.saturating_sub(inner.height as usize);
    super::set_last_diff_pane_max_scroll(max_scroll);
    let clamped_scroll = scroll.min(max_scroll);
    super::set_last_diff_pane_effective_scroll(clamped_scroll);

    let mut visible_lines: Vec<Line<'static>> = rendered
        .lines
        .iter()
        .skip(clamped_scroll)
        .take(inner.height as usize)
        .cloned()
        .collect();
    let visible_end = clamped_scroll + visible_lines.len();
    let visible_left_margins = rendered
        .left_margins
        .get(clamped_scroll..visible_end.min(rendered.left_margins.len()))
        .unwrap_or(&[]);
    record_side_pane_snapshot_precomputed(
        rendered.wrapped_plain_lines.clone(),
        rendered.wrapped_copy_offsets.clone(),
        rendered.raw_plain_lines.clone(),
        rendered.wrapped_line_map.clone(),
        clamped_scroll,
        visible_end,
        inner,
        visible_left_margins,
    );
    apply_side_selection_highlight(app, &mut visible_lines, clamped_scroll);
    super::clear_area(frame, inner);

    let paragraph = if line_wrap {
        Paragraph::new(visible_lines).wrap(Wrap { trim: false })
    } else {
        Paragraph::new(visible_lines)
    };
    frame.render_widget(paragraph, inner);
}

pub(super) fn draw_side_panel_markdown(
    frame: &mut Frame,
    area: Rect,
    app: &dyn TuiState,
    snapshot: &crate::side_panel::SidePanelSnapshot,
    scroll: usize,
    focused: bool,
    centered: bool,
) {
    if area.width < 10 || area.height < 3 {
        return;
    }

    let Some(page) = snapshot.focused_page() else {
        return;
    };

    let page_index = snapshot
        .pages
        .iter()
        .position(|candidate| candidate.id == page.id)
        .map(|idx| idx + 1)
        .unwrap_or(1);
    let page_count = snapshot.pages.len();

    let border_style = side_panel_border_style(focused);
    let Some(content_shell_area) = side_panel_content_area(area) else {
        return;
    };
    // The first render measures whether a native scrollbar is needed. When one
    // is enabled, the content area loses a column.
    let rendered_full_width = render_side_panel_markdown_cached(page, content_shell_area, centered);

    let mut title_parts = vec![Span::styled(" side ", Style::default().fg(tool_color()))];
    title_parts.push(Span::styled(
        page.title.clone(),
        Style::default()
            .fg(file_link_color())
            .add_modifier(ratatui::style::Modifier::BOLD),
    ));
    title_parts.push(Span::styled(
        format!(" {}/{} ", page_index, page_count),
        Style::default().fg(dim_color()),
    ));
    title_parts.push(Span::styled(
        format!(
            " {} hide ",
            crate::tui::keybind::side_panel_toggle_key_label()
        ),
        Style::default().fg(dim_color()),
    ));
    if focused {
        title_parts.push(Span::styled(
            " j/k scroll ",
            Style::default().fg(dim_color()),
        ));
        if page_count > 1 {
            title_parts.push(Span::styled(
                " Tab/Shift-Tab pages ",
                Style::default().fg(dim_color()),
            ));
        }
        title_parts.push(Span::styled(
            " Esc focus chat ",
            Style::default().fg(dim_color()),
        ));
    }

    let Some(content_shell_area) =
        super::draw_right_rail_chrome(frame, area, Line::from(title_parts), border_style)
    else {
        return;
    };
    let show_native_scrollbar = super::native_scrollbar_visible(
        app.side_panel_native_scrollbar() && content_shell_area.width > 1,
        rendered_full_width.lines.len(),
        content_shell_area.height as usize,
    );
    let (content_inner, scrollbar_area) =
        super::split_native_scrollbar_area(content_shell_area, show_native_scrollbar);
    if content_inner.width == 0 || content_inner.height == 0 {
        return;
    }
    let rendered = if show_native_scrollbar {
        render_side_panel_markdown_cached(page, content_inner, centered)
    } else {
        rendered_full_width
    };

    super::set_pinned_pane_total_lines(rendered.lines.len());
    let max_scroll = rendered
        .lines
        .len()
        .saturating_sub(content_inner.height as usize);
    super::set_last_diff_pane_max_scroll(max_scroll);
    let clamped_scroll = scroll.min(max_scroll);
    super::set_last_diff_pane_effective_scroll(clamped_scroll);

    let mut visible_lines: Vec<Line<'static>> = rendered
        .lines
        .iter()
        .skip(clamped_scroll)
        .take(content_inner.height as usize)
        .cloned()
        .collect();
    let visible_end = clamped_scroll + visible_lines.len();
    let visible_left_margins = rendered
        .left_margins
        .get(clamped_scroll..visible_end.min(rendered.left_margins.len()))
        .unwrap_or(&[]);
    record_side_pane_snapshot_precomputed(
        rendered.wrapped_plain_lines.clone(),
        rendered.wrapped_copy_offsets.clone(),
        rendered.raw_plain_lines.clone(),
        rendered.wrapped_line_map.clone(),
        clamped_scroll,
        visible_end,
        content_inner,
        visible_left_margins,
    );
    apply_side_selection_highlight(app, &mut visible_lines, clamped_scroll);
    super::clear_area(frame, content_inner);
    frame.render_widget(Paragraph::new(visible_lines), content_inner);

    if let Some(scrollbar_area) = scrollbar_area {
        super::clear_area(frame, scrollbar_area);
        super::render_native_scrollbar(
            frame,
            scrollbar_area,
            clamped_scroll,
            rendered.lines.len(),
            content_inner.height as usize,
            focused,
        );
    }
}

fn render_side_panel_markdown_cached(
    page: &crate::side_panel::SidePanelPage,
    inner: Rect,
    centered: bool,
) -> PinnedRenderedCache {
    let content_signature = side_panel_content_signature(page);
    let key = SidePanelRenderKey {
        page_id: page.id.clone(),
        content_signature,
        inner_width: inner.width,
        inner_height: inner.height,
        centered,
    };

    if let Some(rendered) = with_side_panel_render_cache_mut(|cache| {
        let rendered = cache.entries.get(&key).cloned();
        if rendered.is_some() {
            lru_touch(&mut cache.order, &key);
            cache.order.push_back(key.clone());
        }
        rendered
    }) {
        with_side_panel_debug_mut(|debug| {
            debug.stats.render_cache_hits += 1;
        });
        return rendered;
    }
    with_side_panel_debug_mut(|debug| {
        debug.stats.render_cache_misses += 1;
    });

    let rendered_markdown =
        render_side_panel_markdown_lines_cached(page, content_signature, inner.width, centered);

    let align = if centered {
        Alignment::Center
    } else {
        Alignment::Left
    };
    let mut text_lines: Vec<Line<'static>> = Vec::new();

    for line in rendered_markdown.rendered_markdown.iter() {
        text_lines.push(align_if_unset(line.clone(), align));
    }

    if centered {
        crate::tui::markdown::recenter_structured_blocks_for_display(
            &mut text_lines,
            inner.width as usize,
        );
    }

    if text_lines.is_empty() {
        text_lines.push(Line::from(Span::styled(
            "No side panel content yet",
            Style::default().fg(dim_color()),
        )));
    }

    let (
        wrapped_plain_lines,
        wrapped_copy_offsets,
        raw_plain_lines,
        wrapped_line_map,
        left_margins,
    ) = build_side_pane_snapshot_cache(&text_lines, inner.width);

    let rendered = PinnedRenderedCache {
        inner_width: inner.width,
        line_wrap: false,
        lines: text_lines,
        wrapped_plain_lines,
        wrapped_copy_offsets,
        raw_plain_lines,
        wrapped_line_map,
        left_margins,
    };

    with_side_panel_render_cache_mut(|cache| {
        lru_touch(&mut cache.order, &key);
        cache.entries.insert(key.clone(), rendered.clone());
        cache.order.push_back(key);
        while cache.order.len() > SIDE_PANEL_RENDER_CACHE_LIMIT {
            if let Some(oldest) = cache.order.pop_front() {
                cache.entries.remove(&oldest);
            }
        }
    });

    rendered
}

fn render_side_panel_markdown_lines_cached(
    page: &crate::side_panel::SidePanelPage,
    content_signature: u64,
    inner_width: u16,
    centered: bool,
) -> RenderedSidePanelMarkdown {
    let key = SidePanelMarkdownKey {
        page_id: page.id.clone(),
        content_signature,
        inner_width,
        centered,
    };

    if let Some(rendered) = with_side_panel_markdown_cache_mut(|cache| {
        let rendered = cache.entries.get(&key).cloned();
        if rendered.is_some() {
            lru_touch(&mut cache.order, &key);
            cache.order.push_back(key.clone());
        }
        rendered
    }) {
        with_side_panel_debug_mut(|debug| {
            debug.stats.markdown_cache_hits += 1;
        });
        return rendered;
    }
    with_side_panel_debug_mut(|debug| {
        debug.stats.markdown_cache_misses += 1;
    });

    let saved_centered = markdown::center_code_blocks();
    markdown::set_center_code_blocks(centered);
    let rendered_lines =
        markdown::render_markdown_with_width(&page.content, Some(inner_width as usize));
    let lines = wrap_side_panel_markdown_lines(rendered_lines, inner_width as usize);
    markdown::set_center_code_blocks(saved_centered);

    let rendered = RenderedSidePanelMarkdown {
        rendered_markdown: lines,
    };

    with_side_panel_markdown_cache_mut(|cache| {
        lru_touch(&mut cache.order, &key);
        cache.entries.insert(key.clone(), rendered.clone());
        cache.order.push_back(key);
        while cache.order.len() > SIDE_PANEL_MARKDOWN_CACHE_LIMIT {
            if let Some(oldest) = cache.order.pop_front() {
                cache.entries.remove(&oldest);
            }
        }
    });

    rendered
}

fn wrap_side_panel_markdown_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .flat_map(|line| {
            if is_rendered_table_line(&line) {
                vec![line]
            } else {
                markdown::wrap_line(line, width)
            }
        })
        .collect()
}
