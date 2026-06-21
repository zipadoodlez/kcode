//! First-run onboarding welcome screen.
//!
//! Rendered in place of the normal empty-state transcript when
//! `TuiState::onboarding_welcome_active()` is true (brand-new install /
//! unauthenticated / new user, or `/onboarding-preview`).
//!
//! Layout, top to bottom, vertically centered in the chat area:
//!   1. Grayed telemetry notice header.
//!   2. The animated donut (attention grab).
//!   3. "Welcome to jcode onboarding" title.
//!   4. The login / getting-started prompt with suggestions.
//!
//! The donut is drawn as a live widget (not part of the cached transcript) so
//! it animates every frame, matching the idle-donut behavior elsewhere.

use super::animations;
use super::{dim_color, header_name_color};
use crate::tui::TuiState;
use crate::tui::color_support::rgb;
use ratatui::{prelude::*, widgets::Paragraph};

const DONUT_HEIGHT: u16 = 12;
const TELEMETRY_LINES: u16 = 4;
const GAP: u16 = 1;

/// Accent color for the welcome title.
fn welcome_accent() -> Color {
    rgb(138, 180, 248)
}

/// Append the universal "Esc to skip" hint shown on every guided onboarding
/// phase. This advertises the escape hatch that guarantees the user can always
/// leave onboarding (see `handle_onboarding_continue_prompt_key`), so a first-
/// run user is never visibly trapped. Kept dim and on its own line so it never
/// competes with the primary action.
fn push_esc_skip_hint(lines: &mut Vec<Line<'static>>, align: Alignment) {
    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            "Esc to skip onboarding (log in later with /login).",
            Style::default().fg(dim_color()),
        ))
        .alignment(align),
    );
}

/// Build the Yes/No selector as a pair of rounded "pills" with the selection
/// indicated *visually* rather than with a sentence of instructions.
///
/// Design goals (per onboarding UX review):
///   * Rounded/pill look instead of a hard rectangle: parentheses `( Yes )`
///     read as a soft capsule in a terminal.
///   * The selected pill is filled (REVERSED) + BOLD; the unselected one is a
///     dim hollow outline. The fill is a NON-color attribute so the selection
///     survives on monochrome terminals (Tier 10 color-independence).
///   * Dim ASCII chevrons `<` ... `>` flank the row to imply "this slides
///     left/right" without the user having to read a hint line. They are pure
///     ASCII so they never depend on Unicode glyph support.
fn yes_no_pill_line(yes_highlighted: bool, align: Alignment) -> Line<'static> {
    let selected = Style::default()
        .fg(welcome_accent())
        .add_modifier(Modifier::BOLD | Modifier::REVERSED);
    let unselected = Style::default().fg(dim_color());
    let chevron = Style::default().fg(dim_color());

    let (yes_style, no_style) = if yes_highlighted {
        (selected, unselected)
    } else {
        (unselected, selected)
    };

    Line::from(vec![
        // Left chevron hints "press left to move here".
        Span::styled("< ", chevron),
        Span::styled("( Yes )", yes_style),
        Span::styled("   ", unselected),
        Span::styled("( No )", no_style),
        // Right chevron hints "press right to move here".
        Span::styled(" >", chevron),
    ])
    .alignment(align)
}

/// Render the import screen body: a "Yes / No" header row, then one row per
/// detected login. Each login has a circle under the Yes column and a circle
/// under the No column; the *filled* circle is the current choice. Every login
/// defaults to "Yes" (import), so the pre-selected state is obvious: the Yes
/// column is already filled. The cursor row is marked with a `>` gutter, and
/// Left/Right move the choice between Yes and No.
///
/// The header, circles, and gutter are interactive-widget glyphs, not
/// load-bearing prose: the surrounding ASCII copy ("We found N existing
/// logins", "Imports all checked in Ns") already conveys state in plain text.
fn import_two_column_lines(prompt: &crate::tui::LoginImportPrompt) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();

    // Left column width: the widest "<cursor>Provider (source)" entry, so the
    // Yes/No circle columns line up cleanly. The 2-cell cursor gutter is
    // included so the columns do not shift when the cursor moves.
    let left_width = prompt
        .rows
        .iter()
        .map(|r| 2 + r.provider_summary.chars().count() + 2 + r.source_name.chars().count() + 1)
        .max()
        .unwrap_or(0)
        .max(12);

    // Fixed-width Yes/No columns so the header labels sit directly above the
    // circles. Each column is `COL_W` cells with the glyph centered; a small gap
    // separates the two columns.
    const COL_W: usize = 5;
    const GAP: &str = "  ";
    let center_cell = |s: &str| {
        let len = s.chars().count();
        let total = COL_W.saturating_sub(len);
        let left = total / 2;
        let right = total - left;
        format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
    };

    let yes_color = rgb(126, 211, 159);
    let filled = Style::default().fg(yes_color).add_modifier(Modifier::BOLD);
    let empty = Style::default().fg(dim_color());
    let header_style = Style::default().fg(dim_color()).add_modifier(Modifier::BOLD);

    // Header row: blank under the provider column, then "Yes" and "No" labels
    // centered over their circle columns.
    out.push(
        Line::from(vec![
            Span::raw(" ".repeat(left_width)),
            Span::styled(center_cell("Yes"), header_style),
            Span::raw(GAP),
            Span::styled(center_cell("No"), header_style),
        ])
        .alignment(Alignment::Center),
    );

    for (i, row) in prompt.rows.iter().enumerate() {
        let is_cursor = i == prompt.cursor;
        // A `> ` gutter marks the row the arrow keys act on.
        let cursor_marker = if is_cursor { "> " } else { "  " };
        let cursor_style = Style::default().fg(welcome_accent());
        let label_style = if row.checked {
            Style::default().fg(rgb(210, 210, 210))
        } else {
            Style::default().fg(dim_color())
        };

        // Compose the left cell, then pad it out to left_width so the circle
        // columns align regardless of provider/source length.
        let left_text = format!(
            "{}{} ({})",
            cursor_marker, row.provider_summary, row.source_name
        );
        let pad = left_width.saturating_sub(left_text.chars().count());

        // Filled circle marks the chosen column; the other is a hollow outline.
        let (yes_glyph, yes_style, no_glyph, no_style) = if row.checked {
            ("●", filled, "○", empty)
        } else {
            ("○", empty, "●", filled)
        };

        let spans: Vec<Span<'static>> = vec![
            Span::styled(cursor_marker, cursor_style),
            Span::styled(row.provider_summary.clone(), label_style),
            Span::styled(format!(" ({})", row.source_name), Style::default().fg(dim_color())),
            Span::raw(" ".repeat(pad)),
            Span::styled(center_cell(yes_glyph), yes_style),
            Span::raw(GAP),
            Span::styled(center_cell(no_glyph), no_style),
        ];
        out.push(Line::from(spans).alignment(Alignment::Center));
    }

    out
}

/// Grayed telemetry notice shown at the very top of the onboarding screen.
fn telemetry_header_lines(width: u16) -> Vec<Line<'static>> {
    let align = Alignment::Center;
    let dim = Style::default().fg(dim_color());
    let lines = vec![
        "jcode collects anonymous usage statistics (version, OS, session",
        "activity, and crash reasons). No code, prompts, or personal data.",
        "Opt out anytime: export JCODE_NO_TELEMETRY=1",
    ];
    lines
        .into_iter()
        .map(|text| {
            // Truncate defensively on very narrow terminals.
            let text = if (text.chars().count() as u16) > width.saturating_sub(2) {
                text.chars()
                    .take(width.saturating_sub(3) as usize)
                    .collect::<String>()
                    + "…"
            } else {
                text.to_string()
            };
            Line::from(Span::styled(text, dim)).alignment(align)
        })
        .collect()
}

/// Welcome title + the getting-started prompt/suggestions.
fn welcome_body_lines(app: &dyn TuiState) -> Vec<Line<'static>> {
    let align = Alignment::Center;
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(
        Line::from(Span::styled(
            "Welcome to jcode onboarding",
            Style::default()
                .fg(welcome_accent())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(align),
    );
    lines.push(
        Line::from(Span::styled(
            "Let's get you set up.",
            Style::default().fg(header_name_color()),
        ))
        .alignment(align),
    );

    use crate::tui::OnboardingWelcomeKind;
    match app.onboarding_welcome_kind() {
        OnboardingWelcomeKind::Login {
            import,
            importing,
            error,
            repair_agent_label,
        } => {
            lines.push(Line::from(""));
            match import {
                None if importing => {
                    // The user committed the import; it's running. Show progress
                    // instead of the manual-login recovery copy so we never tell
                    // the user to "log in again" right after they chose to import.
                    lines.push(
                        Line::from(Span::styled(
                            "Importing your logins…",
                            Style::default()
                                .fg(welcome_accent())
                                .add_modifier(Modifier::BOLD),
                        ))
                        .alignment(align),
                    );
                    lines.push(
                        Line::from(Span::styled(
                            "Hang tight, this only takes a moment.",
                            Style::default().fg(dim_color()),
                        ))
                        .alignment(align),
                    );
                }
                None if error.is_some() => {
                    // A prior import failed. Explain what happened and give a
                    // concrete, guaranteed next step (Enter opens the picker).
                    let reason = error.unwrap_or_default();
                    lines.push(
                        Line::from(Span::styled(
                            "We couldn't import those logins.",
                            Style::default()
                                .fg(rgb(240, 180, 120))
                                .add_modifier(Modifier::BOLD),
                        ))
                        .alignment(align),
                    );
                    if !reason.is_empty() {
                        lines.push(
                            Line::from(Span::styled(
                                reason,
                                Style::default().fg(dim_color()),
                            ))
                            .alignment(align),
                        );
                    }
                    lines.push(Line::from(""));
                    lines.push(
                        Line::from(Span::styled(
                            "No problem - you can log in directly.",
                            Style::default()
                                .fg(welcome_accent())
                                .add_modifier(Modifier::BOLD),
                        ))
                        .alignment(align),
                    );
                    lines.push(
                        Line::from(Span::styled(
                            "Press Enter to choose a provider (OpenAI, Anthropic, and more).",
                            Style::default().fg(dim_color()),
                        ))
                        .alignment(align),
                    );
                    // If we detected a coding agent the user recently used, offer
                    // to hand the fix to it (it can run jcode's auth-test and add
                    // the key non-interactively).
                    if let Some(agent) = repair_agent_label {
                        lines.push(
                            Line::from(Span::styled(
                                format!("Press H to have {agent} help fix this for you."),
                                Style::default().fg(welcome_accent()),
                            ))
                            .alignment(align),
                        );
                    }
                }
                None => {
                    lines.push(
                        Line::from(Span::styled(
                            "First, log in to get started.",
                            Style::default()
                                .fg(welcome_accent())
                                .add_modifier(Modifier::BOLD),
                        ))
                        .alignment(align),
                    );
                    lines.push(
                        Line::from(Span::styled(
                            "Press Enter to pick who to log in with (OpenAI, Anthropic, and more).",
                            Style::default().fg(dim_color()),
                        ))
                        .alignment(align),
                    );
                }
                Some(prompt) => {
                    let total = prompt.rows.len();
                    lines.push(
                        Line::from(Span::styled(
                            format!(
                                "We found {} existing login{}.",
                                total,
                                if total == 1 { "" } else { "s" },
                            ),
                            Style::default()
                                .fg(welcome_accent())
                                .add_modifier(Modifier::BOLD),
                        ))
                        .alignment(align),
                    );
                    lines.push(
                        Line::from(Span::styled(
                            "All set to import. Switch any to No to skip it:",
                            Style::default().fg(dim_color()),
                        ))
                        .alignment(align),
                    );
                    lines.push(Line::from(""));

                    // Per-login rows: each provider is followed by a Yes/No
                    // pair with "Yes" (import) lit by default. The user moves the
                    // cursor up/down and flips a login to "No" to skip it.
                    lines.extend(import_two_column_lines(&prompt));
                    lines.push(Line::from(""));

                    lines.push(
                        Line::from(Span::styled(
                            "Up/down to move, Left/Right for Yes/No.",
                            Style::default().fg(dim_color()),
                        ))
                        .alignment(align),
                    );
                    lines.push(
                        Line::from(Span::styled(
                            "Press Enter to continue.",
                            Style::default()
                                .fg(welcome_accent())
                                .add_modifier(Modifier::BOLD),
                        ))
                        .alignment(align),
                    );
                    lines.push(
                        Line::from(Span::styled(
                            format!("Imports all checked in {}s.", prompt.seconds_left),
                            Style::default().fg(dim_color()),
                        ))
                        .alignment(align),
                    );
                }
            }
            push_esc_skip_hint(&mut lines, align);
            return lines;
        }
        OnboardingWelcomeKind::LoginOpenAi { yes_highlighted } => {
            lines.push(Line::from(""));
            lines.push(
                Line::from(Span::styled(
                    "First, log in to get started.",
                    Style::default()
                        .fg(welcome_accent())
                        .add_modifier(Modifier::BOLD),
                ))
                .alignment(align),
            );
            lines.push(Line::from(""));
            lines.push(
                Line::from(Span::styled(
                    "Log in to OpenAI?",
                    Style::default()
                        .fg(welcome_accent())
                        .add_modifier(Modifier::BOLD),
                ))
                .alignment(align),
            );
            lines.push(
                Line::from(Span::styled(
                    "Choose \"No\" to skip for now (run /login anytime).",
                    Style::default().fg(dim_color()),
                ))
                .alignment(align),
            );
            lines.push(Line::from(""));

            // Rounded Yes/No pills; the selection is shown visually (filled
            // pill + flanking chevrons), so the hint can stay short.
            lines.push(yes_no_pill_line(yes_highlighted, align));
            lines.push(Line::from(""));
            lines.push(
                Line::from(Span::styled(
                    "Enter to confirm.",
                    Style::default().fg(dim_color()),
                ))
                .alignment(align),
            );
            push_esc_skip_hint(&mut lines, align);
            return lines;
        }
        OnboardingWelcomeKind::ContinuePrompt {
            cli_label,
            yes_highlighted,
            seconds_left,
        } => {
            lines.push(Line::from(""));
            lines.push(
                Line::from(Span::styled(
                    format!("Continue where you left off in {cli_label}?"),
                    Style::default()
                        .fg(welcome_accent())
                        .add_modifier(Modifier::BOLD),
                ))
                .alignment(align),
            );
            lines.push(Line::from(""));

            // Rounded Yes/No pills; selection shown visually so the hint stays
            // short. The countdown line below already explains the default.
            lines.push(yes_no_pill_line(yes_highlighted, align));
            lines.push(Line::from(""));
            lines.push(
                Line::from(Span::styled(
                    format!("Opens the resume menu automatically in {seconds_left}s…"),
                    Style::default().fg(dim_color()),
                ))
                .alignment(align),
            );
            push_esc_skip_hint(&mut lines, align);
            return lines;
        }
        OnboardingWelcomeKind::Suggestions => {}
    }

    let suggestions = app.suggestion_prompts();
    if !suggestions.is_empty() {
        lines.push(Line::from(""));
        for (i, (label, prompt)) in suggestions.iter().enumerate() {
            let is_login = prompt.starts_with('/');
            let spans = if is_login {
                vec![
                    Span::styled(
                        format!("{} ", label),
                        Style::default()
                            .fg(welcome_accent())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("(type {})", prompt),
                        Style::default().fg(dim_color()),
                    ),
                ]
            } else {
                vec![
                    Span::styled(
                        format!("[{}] ", i + 1),
                        Style::default().fg(welcome_accent()),
                    ),
                    Span::styled(label.clone(), Style::default().fg(rgb(200, 200, 200))),
                ]
            };
            lines.push(Line::from(spans).alignment(align));
        }
        if suggestions.len() > 1 {
            lines.push(Line::from(""));
            lines.push(
                Line::from(Span::styled(
                    format!("Press 1-{} or type anything to start", suggestions.len()),
                    Style::default().fg(dim_color()),
                ))
                .alignment(align),
            );
        }
    }

    lines
}

/// Draw the full onboarding welcome screen into `area`.
pub(super) fn draw_onboarding_welcome(frame: &mut Frame, app: &dyn TuiState, area: Rect) {
    if area.width < 4 || area.height < 6 {
        // Too small for the full treatment: fall back to a minimal welcome.
        let lines = welcome_body_lines(app);
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    let telemetry = telemetry_header_lines(area.width);
    let body = welcome_body_lines(app);
    let telemetry_h = (telemetry.len() as u16).min(TELEMETRY_LINES);
    let body_h = body.len() as u16;

    // Donut shrinks if the area is short so the welcome text always fits.
    let donut_h = DONUT_HEIGHT.min(
        area.height
            .saturating_sub(telemetry_h + body_h + GAP * 2 + 1),
    );

    let used = telemetry_h + GAP + donut_h + GAP + body_h;
    let pad_top = area.height.saturating_sub(used) / 2;

    let mut constraints = vec![Constraint::Length(pad_top), Constraint::Length(telemetry_h)];
    if donut_h > 0 {
        constraints.push(Constraint::Length(GAP));
        constraints.push(Constraint::Length(donut_h));
    }
    constraints.push(Constraint::Length(GAP));
    constraints.push(Constraint::Length(body_h));
    constraints.push(Constraint::Min(0));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // chunks[0] = top pad, [1] = telemetry, then optional gap+donut, gap, body.
    frame.render_widget(
        Paragraph::new(telemetry).alignment(Alignment::Center),
        chunks[1],
    );

    let mut idx = 2;
    if donut_h > 0 {
        // skip gap chunk
        idx += 1;
        animations::draw_idle_animation(frame, app, chunks[idx]);
        idx += 1;
    }
    // skip gap chunk
    idx += 1;
    frame.render_widget(
        Paragraph::new(body).alignment(Alignment::Center),
        chunks[idx],
    );
}
