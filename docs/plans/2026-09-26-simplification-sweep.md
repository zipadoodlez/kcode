# Simplification sweep, 2026-09-26

Status: in progress
Source: the session working list, promoted here so it survives the session.

Everything below is deletion-first except phase 4. Order matters: the decorative
animations go first because they are what demand a fast tick, then the donut
(which retires the second timer and the partial repaint), then the cadence
collapse, then the one addition, then the audit fix, then one verification run at
the end.

## Checklist

- [ ] **s3 - delete the decorative scroll animations.** The overscroll elastic
      reveal (`OVERSCROLL_DWELL`, `OVERSCROLL_GESTURE_GAP`,
      `register_chat_overscroll`, `chat_overscroll_active`,
      `chat_overscroll_remaining`, `update_chat_overscroll`, the
      `(overscroll x.x)` countdown, its redraw reason, and the three-mode
      `overscroll_status` collapses to on/off), the tail catch-up slide
      (`resolve_tail_follow_scroll`, `TAIL_CATCHUP_ACTIVE`,
      `request_tail_follow_snap`, the `tail_catchup` reason), and the side-pane
      resize easing (`animated_side_pane_ratio` and its from/target/start state;
      snap instead).
- [ ] **s4 - drop the autoscroll cadence override.** Advance N lines on the
      normal fast tick instead of returning `REDRAW_COPY_AUTOSCROLL` ahead of the
      live-output branches. Removes a special case and the responsiveness
      regression the #1333 review flagged (streaming drops to 60ms during a drag).
- [ ] **r2 - delete the idle donut** and, with it, the animation-only partial
      repaint, the second timer (`status_spinner_interval` select arm), the
      `jcode-tui-anim` crate, the `opt-level = 3` overrides, `idle_animation`, and
      `disabled_animations`. Idle motion, if wanted, is a two-glyph cycle on the
      existing `SPINNER_FRAMES`.
- [ ] **r3 - collapse cadence to one rule.** `wants_fast_tick()` plus `FAST`/
      `SLOW` replaces `redraw_interval*`, `periodic_redraw_required*`,
      `live_activity_redraw_reason` and the named-reason table. Drop
      `PerformanceTier` and its auto-scoring, keeping only the real compatibility
      fixes (WSL/Windows Terminal focus + keyboard, macOS glyph-safe cap).
- [ ] **p5 - palette phase 4.** `[display.palette]` with the 16 base16 slots, the
      role to slot default table, and `/colors` editing slots. Acceptance: a
      published base16 theme pasted into config repaints the whole TUI.
- [ ] **a1 - kill the ambient/permissions contradiction.** Delete the dangling
      `#[command(subcommand)]` and ambient doc comment on `Permissions`, decide
      whether `Command::Permissions` and `AmbientTranscript` stay, then make
      `README.md` and `what-was-removed.md` agree with the code.
- [ ] **v1 - verification run, last.** `cargo check --all-targets`, the guard,
      and the suites, diffed against the known baseline (31 lib, 15 math, 1
      lock-order failures on `main`). Nothing earlier in this list is verified
      until this passes.

## Skipped

- **s5 (optional): name the wheel ladder thresholds as constants.** The ladder
  itself is a deliberate, wanted feel; it stays, marker and all.

## Notes

- `main` baselines for the v1 diff: `jcode-tui --lib` 31 failing, the math/LaTeX
  suite 15, `test_lock_order` 1. Any other failure is ours.
- The two plans this leans on: `redraw-simplification.md` (the target shape) and
  `limited-palette.md` (phase 4).
