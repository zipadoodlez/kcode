# Rendering

How the TUI turns state into terminal output: the color pipeline, terminal
compatibility, and the markdown parity contract.

## Colors

The built-in palette is hand-tuned and fixed. `default_palette_is_frozen` in
`crates/jcode-tui-style/src/palette.rs` holds a redundant copy of every value and
fails if one changes. Changing a default changes what every user sees on launch,
so it must be a deliberate edit.

Every color the TUI paints is one of ~22 named semantic roles (`ALL_ROLES`);
widgets name a role, never an rgb value, and a CI guard
(`crates/jcode-tui/tests/no_new_raw_rgb_literals.rs`) keeps it that way. Editing
every call site to retheme would be fragile, so substitution happens at the one
point every color passes through, the rendered frame buffer.
`adapt_buffer_for_display` (`crates/jcode-tui-style/src/display.rs`) has one job:
replace any buffer color equal to a role's default with that role's configured
color. An unconfigured palette is a byte-identical no-op, guarded by tests.

The theme is configuration, not detection. `display.theme` (or the `JCODE_THEME`
env override) selects `dark` (the default) or `light`; `light` installs the baked
light palette (`Role::light_rgb`), which was generated once from the removed
light terminal transform. There is no terminal background query. `light`
entries count as configured, so the display pass repaints the whole UI.

Two consequences:

- **Role accessors return the default.** `theme::user_color()` returns the role's
  *default* color, not the configured one; returning the configured color would
  remap a cell twice and compound the offsets.
- **Only role-tagged colors are configurable.** A buffer color equal to a role's
  default is replaced by that role's override, and ratatui named colors map to the
  role they conventionally stand for. `Color::Reset` is never substituted.

### Configuring colors

```toml
# ~/.kcode/config.toml
[display.colors]
user = "#8ab4f8"
ai = "#81c784"
accent = "#ba8bff"
error = "#ff6464"
```

| command | effect |
|---|---|
| `/colors` | list every configurable role |
| `/colors <role> <#rrggbb>` | set one role (saved to config) |
| `/colors export` | print the palette as config TOML |
| `/colors reset [role]` | reset one role, or all |

Changes apply immediately; no restart.

### Adding a role

1. Add the variant to `Role` in `crates/jcode-tui-style/src/palette.rs`, list it
   in `ALL_ROLES`, and give it a `key()` and a `default_rgb()` equal to today's
   hard-coded value.
2. If it is a background, say so in `is_background()`.
3. Add an accessor in `theme.rs` and use it at the call sites.

`ALL_ROLES` drives the `/colors` listing, completions, and export.

## Terminal compatibility

Color capability is detected once
(`crates/jcode-tui-workspace/src/color_support.rs`): `COLORTERM=truecolor|24bit`,
then `TERM_PROGRAM` (Ghostty, iTerm2, WezTerm, Warp, Alacritty, Hyper), then
`TERM` (kitty/Ghostty/Alacritty, else 256-color). Outside truecolor, `rgb()`
quantizes to the xterm-256 cube plus grayscale ramp, choosing the perceptually
nearest entry.

On macOS, the VS Code integrated terminal and Apple Terminal are capped to 256
colors even when they advertise truecolor: their GPU glyph atlas corrupts under
heavy per-cell RGB churn (#330). Override with `JCODE_GLYPH_SAFE_MODE=on|off`.

Keyboard protocols and tmux are in [tui.md](../user/tui.md). On exit the TUI
restores what it changed (kitty keyboard pop, tmux `modifyOtherKeys` reset).

## Markdown parity

The markdown renderer must match the reference renderer at four levels:

- **L1 content** - the visible text.
- **L2 line structure** - line breaks and block boundaries.
- **L3 wrapped layout** - output wrapped at widths 20, 40, and 80.
- **L4 style invariants** - emphasis, code background, math styling.

Zero tolerance: any mismatch at a level is a failure, not "close enough". Fuzzed
inputs use statistical bounds by the rule of three (no observed failure in n runs
bounds the rate at roughly 3/n). Harness:
`crates/jcode-tui-markdown/src/render_core_adapter_tests.rs`.
