# A limited palette for the TUI

Status: proposal
Problem: kcode ships **222 distinct colors and 699 hardcoded `rgb()` sites**
across 31 files. A conventional theme has 16.

## The problem, measured

| | count |
|---|---|
| semantic roles (`Role` in `jcode-tui-style/src/palette.rs`) | 22 |
| raw `rgb(r, g, b)` literal sites | **699** |
| distinct colors those literals describe | **222** |
| files containing them | 31 |
| colors in a conventional theme (Nord, Solarized, base16) | **16** |

The palette module says the quiet part out loud:

> "Hundreds of ad hoc `rgb(r, g, b)` literals scattered across widgets. … Ad hoc
> `rgb(...)` literals carry no role, so they are not configurable."

So `/colors` can retheme 22 roles, while ~90% of what you actually see on screen
is unreachable. Worse, many literals are near-duplicates of each other, which
means there is no single color to change even if you could reach it:

```
rgb(100,180,100)  vs  rgb(100,200,100)      # two greens
rgb(180,180,190)  vs  rgb(140,140,150)      # two greys
rgb(255,193,7)    vs  rgb(255,200,100)      # two ambers
```

Top offenders: `ui_messages.rs` (110), `swarm_gallery.rs` (88), `ui_input.rs`
(62), `session_picker/render.rs` (54), `info_widget_todos.rs` (47).

## The target

Every color the TUI renders belongs to one small, curated set, reached through a
role name. Nothing is computed, blended, or adapted at runtime.

- **Now: 22 roles.** Code names a role; each role has exactly one color.
- **Later: 16 slots** (base16-shaped). Roles share slots; a theme is 16 hex
  values, so any published base16 theme can be pasted in.
- **Removed outright:** light-mode color math, derived and animated colors,
  shades. A shade either becomes the role it belongs to, or it is deleted.

There is no `harmony` module in kcode (the scorer only ever existed on the jcode
branch), so there is nothing to remove on that front. "Harmonious" here is a
property of the chosen palette, not a runtime tool.

## Why the light-mode machinery can go

`jcode-tui-style/src/theme_mode.rs` (615 lines) paints nothing. It exists to
repair a dark-tuned palette for a light background: a hue-preserving luminance
flip, then a 7:1 contrast repair against an assumed `#e0e0e0` surface
(`theme_mode.rs:110-112`), plus OSC-11 background detection at startup and a
prewarm thread.

Its own header states the tradeoff: "Rather than maintaining a second hand-tuned
palette for light terminals, we adapt colors at a single choke point."

Once the palette is 22 curated values, a second palette is cheap. So generate it
**once**: run the existing transform over the 22 role colors, freeze the result,
store it as the light theme, and delete the transform. Light mode becomes data,
not code. The role set already labels surfaces vs text (`UserBg`,
`SelectionBg`, `Border` against `UserText`, `AiText`, …), so the bake is
mechanical.

One consequence to accept: the runtime repair adapts to the *actual* terminal
background, so freezing it assumes a fixed light surface and retires OSC-11
detection. That is the trade.

The same argument retires derived colors: `theme.rs`'s `rainbow_prompt_color`,
`prompt_entry_color`, `prompt_entry_shimmer_color`, `blend_color`,
`animated_tool_color` and the `jcode-tui-anim` machinery exist to vary a color
over time, which is exactly the complexity this plan removes.

## Target model

```
roles     22 semantic     User, Ai, Tool, Dim, Success, Warning, … each has one
                          color; code never names an rgb value
slots     16 (step 4)     a theme is 16 hex values (base16-shaped); roles share
                          slots by default
config    two keys        [display.palette] is the theme;
                          [display.colors] is a role->slot override
literals  0               outside jcode-tui-style
```

The mechanism already exists. Once per frame,
`theme_mode::adapt_buffer_for_display` rewrites any buffer color that equals a
role's default onto that role's configured color. So a literal that is *given* a
role becomes themeable with no widget changes, and `role_color()` deliberately
returns the *default* so nothing is remapped twice.

## Migration

Each phase lowers `BASELINE`; the guard is the acceptance test.

1. **Port the ratchet.** ✅ Done
   (`crates/jcode-tui/tests/no_new_raw_rgb_literals.rs`). kcode's baseline is
   **697 literals across 30 files**, lower than jcode's 802 because the fork
   removed mermaid and the memory widgets.
2. **Collapse literals onto the 22 roles**, largest first: `ui_messages`,
   `swarm_gallery`, `ui_input`, `session_picker`, the `info_widget_*` group.
   Each literal becomes its nearest role's accessor; a literal with no distinct
   purpose is deleted, not recolored. Done when `BASELINE` is empty and every
   rendered color is a role default.
3. **Bake light, delete the machinery.** Freeze the light theme from the current
   transform, then delete the light math, `theme_detect.rs`/OSC-11, the
   `display.theme` config and `JCODE_THEME`, `palette_literals.rs`, and the
   derived-color helpers in `theme.rs`.
4. **Add the 16-slot layer.** `[display.palette]` with the 16 slots, the
   role->slot default table, and `/colors` editing slots (showing which roles
   share one). Done when a base16 theme pasted into config repaints the TUI.
5. **Delete the empty `BASELINE`.** The guard becomes zero-tolerance: no new
   literal can ever land.

### Provisional role -> slot mapping (step 4)

Roles will keep growing (diff line states, tool progress, per-provider accents);
22 is where the role list starts, not where it stops. Adding a *slot* is
expensive - it changes the theme contract and drops base16 portability - so a new
role must name an existing slot.

| slot | base16 | roles that default to it |
|---|---|---|
| `bg` | base00 | - |
| `bg_alt` | base01 | `UserBg` |
| `bg_selection` | base02 | `SelectionBg` |
| `comment` | base03 | `Dim`, `Border` |
| `fg_dim` | base04 | `System`, `Queued` |
| `fg` | base05 | `UserText`, `AiText`, `HeaderName` |
| `fg_bright` | base06 | - |
| `bg_bright` | base07 | - |
| `red` | base08 | `Error` |
| `orange` | base09 | `Asap` |
| `yellow` | base0A | `Warning`, `Pending` |
| `green` | base0B | `Success`, `Ai` |
| `cyan` | base0C | `Info`, `Tool` |
| `blue` | base0D | `User`, `FileLink`, `HeaderSession` |
| `purple` | base0E | `Accent`, `HeaderIcon` |
| `brown` | base0F | - |

`brown`, `fg_bright` and `bg_bright` start unclaimed: slots exist for themes to
fill, not because every role needs its own color.

## Open decisions

- **The 16 values and the role -> slot table** (step 4). This is the design step,
  the only place taste matters. Everything before it is mechanical.
- **Whether light ships as a selectable theme or is dropped.** Baking it costs
  one table; dropping it costs the light-terminal look.

## Success criteria

- `BASELINE` is empty and the guard is zero-tolerance.
- No raw `rgb(...)` outside `jcode-tui-style`.
- No `ThemeMode`, no runtime contrast math, no derived or animated colors.
- Every rendered color is a role default (step 2), then a slot (step 4).
- A published base16 theme pasted into `config.toml` repaints the whole TUI,
  including tool rows, diffs, swarm gallery and info widgets.
- **Default rendering is not required to be unchanged.** Collapsing 222 colors
  onto 22 roles, then 16 slots, necessarily changes the look.

## Non-goals

- **Not** chasing 26 (Catppuccin) or ~30 (Gruvbox) colors. Those numbers come
  from intensity variants and extra neutrals; 16 is enough for this UI and buys
  base16 theme portability.
- **Not** making every widget independently themeable. Roles are the control
  surface; slots are the palette.
- **Not** preserving the current default look. Reversed from the original plan,
  which required byte-identical defaults; that was incompatible with the palette
  collapse and with removing the light-mode repair.
