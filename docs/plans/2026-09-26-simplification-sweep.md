# Simplification sweep, 2026-09-26

Status: in progress
Source: the session working list, promoted here so it survives the session.

## Handoff

Where things stand, so a fresh session can continue without the transcript.

- **Branch:** `main`. The working branch was fast-forwarded into it and deleted, so
  there is nothing to remember. Tree clean.
- **Landed and verified:** palette phase 2 (all 697 literals through roles; guard
  `BASELINE` empty and zero-tolerance), the follow-up pass that imports role
  accessors instead of qualifying every call, 8 test expectations updated, docs.
- **Landed but UNBUILT:** phase 3 (light mode baked into `Role::light_rgb`, the
  runtime transform deleted) and the prompt-entry animation deletion. Both were
  committed at the user's request without a build. Assume at most trivial
  fixups; the edits were small and reference-checked by grep.
- **Not started:** every task in the checklist below.
- **Order:** s3 -> s4 -> r2 -> r3 -> p5 -> a1 -> v1. Installments land as one
  commit each.
- **Build/verify (v1), and the only build commands used:**
  - `cargo check --profile selfdev -p kcode -p jcode-tui-style -p jcode-tui
    -p jcode-base -p jcode-config-types --all-targets`
  - `cargo test --profile selfdev -p jcode-tui --test no_new_raw_rgb_literals`
  - `cargo test --profile selfdev -p jcode-tui-style -p jcode-tui
    -p jcode-config-types --no-fail-fast`
  - Rust builds go through `scripts/dev_cargo.sh` via the shell wrapper; a check
    is ~2-5 min on this box. `rustfmt --edition 2024 <files>` is a syntax parse,
    not a build, and was used in place of compiling.
- **Failure baseline for the v1 diff (all pre-existing on `main`):**
  `jcode-tui --lib` 31 failing, the markdown math/LaTeX suite 15,
  `test_lock_order` 1. Anything else is ours. Two more tests
  (`render_system_message_uses_scheduled_task_card`,
  `test_agent_model_picker_openrouter_...`) fail only under parallel execution and
  pass in isolation; treat as flaky.
- **Findings that live only here:**
  - Issue #1332 / PR #1333: kcode already carries the core (no wheel queue,
    discrete notches, proximity autoscroll, overlay scroll dedup). Only the
    velocity ladder was in question and it is **kept by decision**.
  - `what-was-removed.md` and `README.md` disagree with the code about ambient
    permissions (task a1).

### Warnings for whoever picks this up

- **Deleting by marker range swallowed adjacent code once.** Cutting from
  `let now_ms = ...` to the next `if let` in `ui_viewport.rs` also took the
  viewport padding and `clear_area` with it. Diff-check every ranged deletion.
- **Removing an `rgb` import can break child modules** that re-import it via
  `use super::rgb`, and test modules that got it from `use super::*`. The same
  applies to any glob-imported helper.
- The guard test is zero-tolerance now: any new raw `rgb(...)` outside
  `jcode-tui-style` fails. Use a role.

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
