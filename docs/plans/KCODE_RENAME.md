# Renaming jcode to kcode

Status: **Phase 1 (identity) is done.** This doc is the plan for everything
after it, re-measured against the current tree.

The point of the rename was never cosmetics. It was that a binary called
`jcode` claims `~/.jcode` and the shared runtime dir, so it refuses to start
alongside an installed jcode. Giving it its own name, dirs and sockets makes
`kcode` coexist instead of fight. **That goal is already met:** package and
binary are `kcode`, home is `~/.kcode`, config is `~/.config/kcode`, sockets
are `kcode.sock` / `kcode-debug.sock` / `kcode-daemon.lock`, and the two
daemons run side by side.

Everything below is what remains. It splits cleanly into "the name is wrong
where a user can see it" and "the name is wrong where nobody can see it".

## Measured today

| surface | count | user-visible | verdict |
|---|---|---|---|
| user-facing paths still written `~/.jcode/...` | 119 occurrences, ~57 files | **yes, and wrong** | fix |
| user-facing `jcode <cmd>` help/error strings | 307 | yes | fix |
| log filenames (`jcode-YYYY-MM-DD.log`) | 3 sites + tests | yes | fix |
| process titles (`jcode:s:`, `jcode:d:`) | ~8 | yes (`ps`) | fix |
| `JCODE_*` env vars | 602 names / 4952 occurrences | partly | decision |
| docs / scripts / CI | 1090 / 1752 / 147 | mostly dev-facing | fix where they name the binary |
| `jcode-*` crate names | 62 | no | **skip** |

## Do not rename: identity that must stay `jcode`

A blind `jcode` → `kcode` rename would break these. They refer to the *service*
or to *upstream*, not to this binary.

| must stay | count | why |
|---|---|---|
| `github.com/1jehuang/jcode` | 25 | upstream repo; `origin` remote, cherry-pick source |
| `jcode.sh` | 37 | the Jcode account/subscription service domain |
| `ModelRouteApiMethod::JcodeSubscription`, provider id `"jcode"` | 25 | the `jcode` provider is a real subscription service, not this binary |
| "Jcode Subscription" / "Jcode account" strings | 148 | service branding |
| `[lib] name = "jcode"` | 1 | internal; renaming forces `use jcode::` churn everywhere for no visible gain |
| imported-session paths (claude/codex import) | — | they encode upstream paths on purpose |

## Phase 2 - user-visible paths and branding (the real work)

This is not cosmetic. The state dir moved to `~/.kcode`, but code and messages
still say `~/.jcode`, so instructions point at paths that **do not exist**.
Concrete example: logs are written to `~/.kcode/logs/jcode-2026-09-24.log`.

Order by user impact:

1. **Log filenames** - `crates/jcode-logging/src/lib.rs` (write site, the
   `find_latest` path, rotation/`cleanup` prefix match, 3 test fixtures).
   Change `jcode-` prefix to `kcode-`. Rotation must keep matching old
   `jcode-*.log` files for one deprecation window, or users never see their
   old logs cleaned up.
2. **User-facing path strings** - ~57 files, 119 occurrences:
   `~/.jcode/skills`, `~/.jcode/mcp.json`, `~/.jcode/swarm-prompt.md`,
   `~/.jcode/plans`, `~/.jcode/todos`, `~/.jcode/state`, `~/.jcode/logs`,
   `~/.jcode/config.toml`, `~/.config/jcode`. Highest-value files:
   `tool/skill.rs`, `tool/mcp.rs`, `tool/communicate.rs`,
   `tool/config_edit_notice.rs`, `server/socket.rs`,
   `server/swarm_persistence.rs`, `config/default_file.rs`, `cli/debug.rs`.
3. **CLI help and error strings** - 307 occurrences of the binary being named
   `jcode` in `--help`, usage errors and banners (`src/cli/*.rs`,
   `crates/jcode-tui/src/tui/app/input_help.rs`).
4. **Process titles** - `crates/jcode-base/src/process_title.rs` and
   `src/cli/proctitle.rs`: `jcode`, `jcode:s:`, `jcode:d:`, `jcode:selfdev`,
   `jcode:client`.
5. **TUI strings** - "Spawn new jcode session", `/log` help claiming
   `~/.jcode/logs/jcode-*.log`, swarm-prompt help naming `./.jcode/`.

Gate: no user-visible string says `jcode` where it means this binary; a user
can follow the `/log`, `/skills` and MCP messages and land on real paths.

## Phase 3 - env var prefix (decision, not yet worth it)

602 unique `JCODE_*` names, 4952 occurrences. Options:

- **Keep `JCODE_*`.** Zero risk. Document it. Inconsistent with the dirs.
- **Rename to `KCODE_*` with a `JCODE_*` fallback.** ~5000-line mechanical
  diff; the fallback is permanent surface; test sandboxes isolate through
  `JCODE_HOME`/`JCODE_RUNTIME_DIR`, so a botched pass silently breaks test
  isolation.

Recommendation: keep until kcode has users who will actually type these.
This is the one place the earlier plan's "optional" call still holds.

## Phase 4 - docs, scripts, CI

`docs/*.md` (1090), `scripts/` (1752), `.github/` (147). Update where they name
the binary or its artifacts (`jcode-linux-x86_64` etc.). Leave prose that
explains the *fork of jcode*.

## Phase 5 - crate names (skip)

62 `crates/jcode-*` -> `crates/kcode-*` plus every `use`. Invisible to users.
Pure churn with high conflict cost. Skip unless reading `kcode` in the source
tree is itself the goal.

## Open decisions

1. **Project-local dir**: `./.jcode/skills`, `./.jcode/mcp.json`,
   `./.jcode/swarm-prompt.md`. Rename to `./.kcode/` (consistent, breaks
   reading existing jcode project config) or keep (compatible with jcode
   projects, slightly inconsistent)?
2. **Env var prefix**: keep `JCODE_*` or rename with fallback?
3. **Log rotation**: match old `jcode-*.log` forever, or only for a window?

## Already done in this pass

- Removed the dead `/memory` command surface (see README "Status").
- Dropped the stale `[ambient]` section from the default config template.
