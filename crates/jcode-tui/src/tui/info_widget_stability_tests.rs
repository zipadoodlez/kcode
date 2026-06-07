use super::*;
use crate::tui::info_widget::InfoWidgetData;

/// Build widget data that yields a stable overview widget (model + queue line etc).
fn sample_data() -> InfoWidgetData {
    InfoWidgetData {
        model: Some("gpt-test".to_string()),
        queue_mode: Some(true),
        ..Default::default()
    }
}

#[test]
fn flat_content_is_perfectly_stable() {
    // Uniform narrow content: the negative-space shape never changes while scrolling,
    // so a well-behaved widget should never move.
    let content: Vec<u16> = vec![20; 200];
    let report = measure_scroll(&content, 100, 20, &sample_data());
    assert!(report.frames > 10, "expected many scroll frames");
    assert_eq!(
        report.total_travel, 0,
        "uniform content should produce zero widget travel, got {:#?}",
        report
    );
    assert_eq!(report.total_flicker, 0, "uniform content should not flicker");
    assert!(report.distraction_per_100_lines.abs() < f64::EPSILON);
}

#[test]
fn ragged_content_makes_widgets_move() {
    // Chat-like content: mostly narrow lines with a long line every few rows. This
    // leaves fitting regions in the right margin, but their top/height shift under
    // the fixed screen rows as you scroll, which is exactly the reported distraction.
    let content: Vec<u16> = (0..200)
        .map(|i| if i % 7 == 0 { 95 } else { 28 })
        .collect();
    let report = measure_scroll(&content, 100, 24, &sample_data());
    assert!(
        report.widgets.iter().any(|w| w.frames_present > 0),
        "expected at least one widget to be placed, got {:#?}",
        report
    );
    assert!(
        report.distraction_per_100_lines > 0.0,
        "ragged content should register movement, got {:#?}",
        report
    );
}

#[test]
fn analyze_frames_counts_travel_and_flicker() {
    // Hand-built frames to lock the metric math independent of the layout engine.
    let a = PlacedRect {
        kind: "overview",
        x: 60,
        y: 4,
        width: 30,
        height: 8,
    };
    let moved = PlacedRect {
        kind: "overview",
        y: 7,
        ..a
    };
    let frames = vec![vec![a], vec![moved], vec![]];
    let report = analyze_frames(&frames);
    assert_eq!(report.steps, 2);
    let w = &report.widgets[0];
    assert_eq!(w.y_travel, 3, "expected |7-4| vertical travel");
    assert_eq!(w.move_events, 1);
    assert_eq!(w.disappearances, 1);
    assert_eq!(report.total_flicker, 1);
    // unstable in both steps (move then disappear)
    assert!((report.unstable_step_fraction - 1.0).abs() < f64::EPSILON);
}

#[test]
fn empty_input_is_safe() {
    let report = analyze_frames(&[]);
    assert_eq!(report.frames, 0);
    assert_eq!(report.steps, 0);
    assert!(report.widgets.is_empty());
}
