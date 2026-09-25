# TUI

The terminal UI. This covers input and the session picker; color roles live in
`internals/rendering.md` and desktop panels in `internals/panels.md`.

## Multi-line input

Terminals send one byte for Enter (`0x0d`), so `Enter`, `Shift+Enter`, and
`Ctrl+Enter` arrive identically and an app cannot tell them apart. kcode's fix is
the **kitty keyboard protocol**: it asks the terminal to disambiguate at startup,
and the terminal then sends `ESC[13;2u` for Shift+Enter. On a terminal that
implements the protocol, **Shift+Enter inserts a newline with no setup**.

Inside **tmux**, the app must also opt in with xterm `modifyOtherKeys` mode 2
(`ESC[>4;2m`), which kcode requests; tmux needs `extended-keys on` and
`extended-keys-format csi-u`. `extended-keys always` is not required. kcode
re-asserts the request when it reapplies terminal modes and resets it with
`ESC[>4;0m` on exit.

### `/terminal-setup`

Run it when Shift+Enter submits instead of inserting a newline. It queries the
terminal for real support (rather than assuming, since the activation sequence
"succeeds" almost everywhere), then confirms the chord works, applies the needed
config (tmux settings, WezTerm's `enable_kitty_keyboard`), or explains why config
cannot help (Terminal.app).

### Fallbacks

These work on any terminal because they do not depend on modified-key reporting:

- **Trailing backslash then Enter** inserts a newline, like shell line
  continuation. An escaped `\\` is literal text and submits.
- **Option/Alt+Enter** works wherever the terminal sends `ESC` + `CR`, including
  Terminal.app with "Use Option as Meta Key".

## Session picker

`/resume`, `/session`, and `/sessions` open the interactive picker. They are
local UI commands and are never sent as chat prompts.

| key | action |
|---|---|
| `Enter` | resume the highlighted session in the current terminal |
| `Ctrl+Enter` | open it in a new terminal |
| `Esc`, `q`, `Ctrl+C` | close the picker without changing sessions |

The default is configured:

```toml
# ~/.kcode/config.toml
[keybindings]
session_picker_enter = "current-terminal"   # or "new-terminal" to swap
```

Resuming **in the current terminal** switches the current workspace/client to
that session. Importable external sessions are converted to kcode sessions
first. If several are selected, only the first is resumed in place and the UI
says so; the picker closes once the resume is queued.

Opening **in a new terminal** gives each selected target its own terminal when
possible, or prints `kcode --resume <id>` commands when terminal launch is
unavailable. The picker stays open and clears the selection so more sessions can
be opened.

`/save [label]` bookmarks the current session into the picker's saved section;
`/unsave` removes the bookmark.
