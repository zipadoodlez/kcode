//! Quantifying info-widget movement during scrolling.
//!
//! Info widgets are placed into the "negative space" (free width to the right/left
//! of the rendered text) of the *currently visible* viewport, anchored to absolute
//! screen rows. As the user scrolls, the per-row free-width profile changes line by
//! line, so a widget that fit at one screen position may reflow or jump elsewhere.
//! That reflow is the "distracting movement" users report.
//!
//! This module turns that distraction into deterministic numbers so it can be
//! measured and A/B-tested. It exposes:
//!
//! * [`analyze_frames`] - given the widget placements observed across a sequence of
//!   scroll frames, compute movement/flicker metrics. This is the shared analyzer
//!   used by both the synthetic simulation here and the live debug bench.
//! * [`simulate_scroll`] - drive the real layout algorithm over a synthetic content
//!   width profile, one scroll line at a time, producing the frame sequence.

use super::info_widget::{InfoWidgetData, WidgetPlacement};
use super::info_widget_layout::{Margins, WidgetAnchor, calculate_placements_anchored};
use ratatui::layout::Rect;
use serde::Serialize;
use std::collections::BTreeMap;

/// A single widget rectangle observed in one frame.
#[derive(Debug, Clone, Copy)]
pub struct PlacedRect {
    pub kind: &'static str,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl PlacedRect {
    pub fn from_placement(p: &WidgetPlacement) -> Self {
        Self {
            kind: p.kind.as_str(),
            x: p.rect.x,
            y: p.rect.y,
            width: p.rect.width,
            height: p.rect.height,
        }
    }
}

/// Per-widget motion statistics accumulated across the scroll sequence.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WidgetMotion {
    pub kind: String,
    /// Number of frames in which this widget was present.
    pub frames_present: usize,
    /// Transitions from absent -> present after the first frame (flicker in).
    pub appearances: usize,
    /// Transitions from present -> absent (flicker out).
    pub disappearances: usize,
    /// Frames where the widget moved (x or y changed) vs the previous present frame.
    pub move_events: usize,
    /// Frames where the widget changed width or height.
    pub resize_events: usize,
    /// Total absolute vertical travel (sum of |dy|) across consecutive present frames.
    pub y_travel: u32,
    /// Total absolute horizontal travel (sum of |dx|).
    pub x_travel: u32,
    /// Total absolute width change.
    pub width_churn: u32,
    /// Total absolute height change.
    pub height_churn: u32,
    /// Largest single-frame top-left jump (Chebyshev distance).
    pub max_jump: u16,
}

impl WidgetMotion {
    /// Composite "distraction" contribution from this widget: positional travel
    /// plus flicker (each appear/disappear weighted like a full-widget jump).
    fn distraction(&self) -> u32 {
        self.x_travel
            + self.y_travel
            + (self.appearances as u32 + self.disappearances as u32) * FLICKER_WEIGHT
    }
}

/// A flicker (appear/disappear) is at least this visually disruptive, in
/// equivalent cells of positional travel.
const FLICKER_WEIGHT: u32 = 8;

/// Aggregate stability report over a scroll sequence.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StabilityReport {
    /// Number of frames (scroll positions) analyzed.
    pub frames: usize,
    /// Number of scroll steps (frames - 1).
    pub steps: usize,
    /// Per-widget breakdown, sorted by descending distraction.
    pub widgets: Vec<WidgetMotion>,
    /// Total move events across all widgets.
    pub total_move_events: usize,
    /// Total flicker transitions (appearances + disappearances).
    pub total_flicker: usize,
    /// Total positional travel (x + y) across all widgets.
    pub total_travel: u32,
    /// Total size churn (width + height) across all widgets.
    pub total_size_churn: u32,
    /// Positional travel per 100 scroll lines (the headline distraction metric).
    pub travel_per_100_lines: f64,
    /// Flicker transitions per 100 scroll lines.
    pub flicker_per_100_lines: f64,
    /// Composite distraction score per 100 scroll lines (travel + weighted flicker).
    pub distraction_per_100_lines: f64,
    /// Fraction of scroll steps in which at least one widget moved or flickered.
    pub unstable_step_fraction: f64,
    /// Widget kind contributing the most distraction.
    pub worst_widget: Option<String>,
}

/// Analyze a sequence of frames (each a list of placed widget rects) and compute
/// movement/flicker metrics. Frames are assumed to be consecutive scroll positions
/// differing by one content line.
pub fn analyze_frames(frames: &[Vec<PlacedRect>]) -> StabilityReport {
    let mut report = StabilityReport {
        frames: frames.len(),
        steps: frames.len().saturating_sub(1),
        ..Default::default()
    };
    if frames.len() < 2 {
        // Still record presence so single-frame callers see widget set.
        if let Some(first) = frames.first() {
            let mut by_kind: BTreeMap<&'static str, WidgetMotion> = BTreeMap::new();
            for r in first {
                by_kind.entry(r.kind).or_default().frames_present += 1;
            }
            report.widgets = by_kind
                .into_iter()
                .map(|(k, mut m)| {
                    m.kind = k.to_string();
                    m
                })
                .collect();
        }
        return report;
    }

    let mut by_kind: BTreeMap<&'static str, WidgetMotion> = BTreeMap::new();
    let mut unstable_steps = 0usize;

    // Count presence in the very first frame.
    for r in &frames[0] {
        let m = by_kind.entry(r.kind).or_default();
        m.frames_present += 1;
    }

    for step in 0..frames.len() - 1 {
        let prev = &frames[step];
        let cur = &frames[step + 1];
        let mut step_unstable = false;

        // Index current frame by kind for lookup.
        let cur_index = |kind: &str| cur.iter().find(|r| r.kind == kind).copied();
        let prev_index = |kind: &str| prev.iter().find(|r| r.kind == kind).copied();

        // Gather the union of kinds present in either frame.
        let mut kinds: Vec<&'static str> = Vec::new();
        for r in prev.iter().chain(cur.iter()) {
            if !kinds.contains(&r.kind) {
                kinds.push(r.kind);
            }
        }

        for kind in kinds {
            let m = by_kind.entry(kind).or_default();
            match (prev_index(kind), cur_index(kind)) {
                (Some(p), Some(c)) => {
                    m.frames_present += 1;
                    let dx = abs_diff(p.x, c.x);
                    let dy = abs_diff(p.y, c.y);
                    let dw = abs_diff(p.width, c.width);
                    let dh = abs_diff(p.height, c.height);
                    if dx != 0 || dy != 0 {
                        m.move_events += 1;
                        step_unstable = true;
                    }
                    if dw != 0 || dh != 0 {
                        m.resize_events += 1;
                        step_unstable = true;
                    }
                    m.x_travel += dx as u32;
                    m.y_travel += dy as u32;
                    m.width_churn += dw as u32;
                    m.height_churn += dh as u32;
                    m.max_jump = m.max_jump.max(dx.max(dy));
                }
                (None, Some(_)) => {
                    m.frames_present += 1;
                    m.appearances += 1;
                    step_unstable = true;
                }
                (Some(_), None) => {
                    m.disappearances += 1;
                    step_unstable = true;
                }
                (None, None) => {}
            }
        }

        if step_unstable {
            unstable_steps += 1;
        }
    }

    let mut widgets: Vec<WidgetMotion> = by_kind
        .into_iter()
        .map(|(k, mut m)| {
            m.kind = k.to_string();
            m
        })
        .collect();
    widgets.sort_by(|a, b| b.distraction().cmp(&a.distraction()).then(a.kind.cmp(&b.kind)));

    report.total_move_events = widgets.iter().map(|w| w.move_events).sum();
    report.total_flicker = widgets
        .iter()
        .map(|w| w.appearances + w.disappearances)
        .sum();
    report.total_travel = widgets.iter().map(|w| w.x_travel + w.y_travel).sum();
    report.total_size_churn = widgets.iter().map(|w| w.width_churn + w.height_churn).sum();
    report.worst_widget = widgets
        .first()
        .filter(|w| w.distraction() > 0)
        .map(|w| w.kind.clone());

    let steps = report.steps.max(1) as f64;
    report.travel_per_100_lines = report.total_travel as f64 / steps * 100.0;
    report.flicker_per_100_lines = report.total_flicker as f64 / steps * 100.0;
    let distraction: u32 = widgets.iter().map(|w| w.distraction()).sum();
    report.distraction_per_100_lines = distraction as f64 / steps * 100.0;
    report.unstable_step_fraction = unstable_steps as f64 / steps;
    report.widgets = widgets;

    report
}

/// Drive the real layout algorithm over a synthetic content-width profile, scrolling
/// one content line at a time, and return the placements observed at each scroll
/// position. `content_widths[i]` is the rendered text width of content line `i`.
///
/// This faithfully exercises the production [`calculate_placements`] including the
/// sticky carry-over pass, so the resulting metrics reflect the real algorithm.
pub fn simulate_scroll(
    content_widths: &[u16],
    area_width: u16,
    viewport_height: u16,
    data: &InfoWidgetData,
) -> Vec<Vec<PlacedRect>> {
    let mut frames: Vec<Vec<PlacedRect>> = Vec::new();
    if area_width == 0 || viewport_height == 0 || content_widths.is_empty() {
        return frames;
    }

    let total_lines = content_widths.len();
    let view = viewport_height as usize;
    let max_scroll = total_lines.saturating_sub(view);
    let area = Rect::new(0, 0, area_width, viewport_height);

    // Carry anchors across frames exactly like the live renderer does, so the
    // HUD pinning / hide-in-place behaviour is exercised identically.
    let mut anchors: Vec<WidgetAnchor> = Vec::new();

    for scroll in 0..=max_scroll {
        let mut right_widths: Vec<u16> = Vec::with_capacity(view);
        for row in 0..view {
            let line = scroll + row;
            let used = content_widths
                .get(line)
                .copied()
                .unwrap_or(0)
                .min(area_width);
            right_widths.push(area_width.saturating_sub(used));
        }
        let margins = Margins {
            right_widths,
            left_widths: Vec::new(),
            centered: false,
        };
        let outcome = calculate_placements_anchored(area, &margins, data, true, &anchors);
        frames.push(
            outcome
                .visible
                .iter()
                .map(PlacedRect::from_placement)
                .collect(),
        );
        anchors = outcome.anchors;
    }

    frames
}

/// Convenience: simulate a scroll and return the aggregate report.
pub fn measure_scroll(
    content_widths: &[u16],
    area_width: u16,
    viewport_height: u16,
    data: &InfoWidgetData,
) -> StabilityReport {
    let frames = simulate_scroll(content_widths, area_width, viewport_height, data);
    analyze_frames(&frames)
}

fn abs_diff(a: u16, b: u16) -> u16 {
    if a >= b { a - b } else { b - a }
}

/// Map a captured widget-kind string back to a stable `&'static str` so live
/// frame captures (which carry owned `String` kinds) can flow through
/// [`analyze_frames`]. Unknown kinds collapse to `"other"`.
pub fn intern_kind(kind: &str) -> &'static str {
    use super::info_widget::WidgetKind;
    for k in WidgetKind::all_by_priority() {
        if k.as_str() == kind {
            return k.as_str();
        }
    }
    "other"
}

#[cfg(test)]
#[path = "info_widget_stability_tests.rs"]
mod tests;
