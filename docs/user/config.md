# Configuration

kcode's settings live in `~/.kcode/config.toml` (the `.kcode` directory follows
`KCODE_HOME` when set). In the TUI:

- `/config` shows the current configuration,
- `/config init` (or `/config create`) writes a commented template,
- `/config edit` opens it, creating the file first if needed.

Section-specific options are documented where they are used: `[provider]` and
profiles in [providers.md](providers.md), credentials in [auth.md](auth.md),
`[hooks]` and `[terminal]` in [hooks.md](hooks.md), and `[keybindings]` in
[tui.md](tui.md).

## Environment variables

Every setting that has an environment variable is named `KCODE_*`. There is no
`KCODE_*` spelling.

The three that matter most:

| Variable | Effect |
| --- | --- |
| `KCODE_HOME` | state directory, default `~/.kcode` |
| `KCODE_SOCKET` | server socket path, default `<runtime_dir>/kcode.sock` |
| `KCODE_RUNTIME_DIR` | directory for runtime sockets, default `XDG_RUNTIME_DIR` |

## The system prompt

kcode assembles its system prompt from layers. Two are user-editable files, so
you can tune agent behavior without rebuilding.

Layers, in order:

1. **Base system prompt** - built-in `crates/kcode-base/src/prompt/system_prompt.md`,
   overridable by file (below).
2. Capability modules (for example, mermaid guidance).
3. Product-specific self-dev guidance. A session rooted in a kcode Desktop
   checkout gets the Desktop prompt and `desktop_selfdev` tool, separate from the
   CLI/TUI self-dev flags.
4. `AGENTS.md` - project `./AGENTS.md` and global `~/AGENTS.md`.
5. Prompt overlay - `./.kcode/prompt-overlay.md` and `~/.kcode/prompt-overlay.md`.
6. Preferred tools - `./.kcode/preferred-tools.md` and
   `~/.kcode/preferred-tools.md`.
7. Memory and the active skill prompt (dynamic, not cached).

Note the asymmetry: project files live under a `.kcode/` directory in the
project; global files live under `~/.kcode/`. When the project and global paths
resolve to the same file (working in `$HOME`, or symlink aliases), it is included
once under its project heading; otherwise both are included even if identical.

### Adding guidance

Append without touching the default prompt:

- `~/.kcode/prompt-overlay.md` - applies everywhere.
- `./.kcode/prompt-overlay.md` - applies to one project.

### Replacing the base prompt

To replace layer 1 entirely, create one of:

- `./.kcode/system-prompt.md` (project, highest precedence)
- `~/.kcode/system-prompt.md` (global)

The first non-empty file wins; otherwise the built-in default is used. An empty
or whitespace-only file falls back to the default, so an empty prompt cannot ship
by accident. This replaces only the base prompt: AGENTS.md, overlays, skills, and
memory still apply.

### Timing

These files take effect for **new sessions**; a running session keeps the prompt
it captured at start. Editing the built-in `system_prompt.md` needs a rebuild,
since it is embedded with `include_str!`.

### Swarm prompt

Swarm model-routing guidance has its own file, `.kcode/swarm-prompt.md`, edited
with `/swarm-prompt` (project or global). New agents load the latest contents
immediately; running agents keep the prompt they captured at creation, so their
tool definition and context cache stay stable.
