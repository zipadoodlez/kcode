# A limited palette for the TUI

Status: proposal
Problem: kcode has **222 distinct colors and 699 hardcoded `rgb()` sites**
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

## Prior art

This is the continuation of **upstream issue #1397** ("colors without a role
stop being configurable"). Step 5 of that plan is already written and lives on
the `guard/no-raw-rgb-literals` branch: a CI ratchet
(`crates/jcode-tui/tests/no_new_raw_rgb_literals.rs`) with a `BASELINE` of
per-file literal counts. It fails when a literal is *added* and does not care
about the existing ones, so migration can be incremental and the ratchet
tightens as families move to roles. That guard does not exist in kcode yet.

## Target model

Three layers, each with one job:

```
palette   16 slots        a theme is 16 hex values (base16-shaped, so any
                          published base16 theme can be pasted in)
roles     22 semantic     User, Ai, Tool, Dim, Success, Warning, … each
                          defaults to a slot; code never names a color
config    two keys        [display.palette] is the theme;
                          [display.colors] is a role->slot override
literals  0               outside jcode-tui-style
```

22 roles over 16 slots is not a squeeze - roles share slots by default, exactly
as Dracula maps ~12 token classes onto 7 hues. Proposed default mapping:

### Slots are fixed, roles are open-ended

The asymmetry is the whole design:

- **Adding a role is cheap and expected.** Name it, assign it a slot, done.
  Roles will keep growing (diff line states, tool progress, per-provider
  accents) and that is fine - 22 is where we start, not where we stop.
- **Adding a slot is expensive and deliberate.** It changes the theme contract,
  invalidates every existing theme's mapping, and drops base16 portability.
  It should need a written reason, not a code review.

The rule for a new role: **it must name an existing slot.** If it seems to need
its own color, that usually means an old literal was a near-duplicate - assign
it the nearest slot and delete the shade.

Initial role -> slot mapping:

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

The mechanism already exists: `adapt_buffer_for_display` rewrites any buffer
color that equals a role's default onto that role's configured color once per
frame. So a literal that is *given* a role becomes themeable with no widget
changes, and an unconfigured palette stays byte-identical to today.

## Migration

Each phase lowers `BASELINE`; the guard is the acceptance test.

1. **Port the ratchet.** ✅ Done
   (`crates/jcode-tui/tests/no_new_raw_rgb_literals.rs`). kcode's baseline is
   **697 literals across 30 files** - lower than jcode's 802 because the fork
   removed mermaid and the memory widgets. Verified: lowering one entry fails
   with `(+1)`; adding a literal in a file absent from `BASELINE` fails too.
2. **Add the slot layer.** `[display.palette]` with the 16 slots, the role->slot
   default table, and `/colors` editing slots (showing which roles share one).
   Done when a base16 theme pasted into config repaints every role.
3. **Collapse duplicate families**, largest first - `ui_messages`,
   `swarm_gallery`, `ui_input`, `session_picker`, the `info_widget_*` group.
   Done when those files reach zero and their colors are ≤ the slots they use.
4. **Sweep the remainder**, then delete the empty `BASELINE`. The guard becomes
   zero-tolerance: no new literal can ever land.

Phase 3 is the real work and it is mechanical: most sites are a near-duplicate
of a slot, so the change is "pick the slot, delete the shade".

## Success criteria

- `BASELINE` is empty and the guard is zero-tolerance.
- 222 distinct colors collapse to 16 defaults.
- A published base16 theme pasted into `config.toml` repaints the whole TUI,
  including tool rows, diffs, swarm gallery and info widgets.
- Default rendering is unchanged (existing golden/render tests still pass).

## Non-goals

- **Not** chasing 26 (Catppuccin) or ~30 (Gruvbox) colors. Those numbers come
  from intensity variants and extra neutrals; 16 is enough for this UI and buys
  base16 theme portability.
- **Not** making every widget independently themeable. Roles are the control
  surface; slots are the palette.
- **Not** changing the default look. The migration is a refactor of how colors
  are *named*, not of what they are.
