# Renaming jcode to kcode

Status: in progress. Phase 1 is the product; phases 2 and 3 are optional and
probably YAGNI. See "What we deliberately skip" at the bottom.

The point of the name change is not cosmetics. It is that a binary called
`jcode` claims `~/.jcode` and the shared runtime dir by default, so it refuses
to start alongside an installed jcode. Giving it its own name, dirs and sockets
makes `kcode` a thing that coexists instead of a fork that fights.

## Decisions (fixed)

- Package and main binary become `kcode`. Version resets to `0.1.0`; we no
  longer inherit upstream's `0.85.x`.
- Home dir becomes `~/.kcode`, app config `~/.config/kcode`, sockets
  `kcode.sock` / `kcode-debug.sock` / `kcode-daemon.lock`.
- No dual identity. We do not alias old names. One-time copy of `~/.jcode`
  carries auth and sessions over.
- Branch stays a branch. Repo split happens after Phase 1, not before.

## What a rename has to touch, measured

| surface | count | user-visible | phase |
|---|---|---|---|
| central dir/socket resolution | 2 files | yes | 1 |
| package + bin name | 1 file | yes | 1 |
| display/branding strings | ~8 files | yes | 1 |
| `JCODE_*` env vars | 482 names | yes, but later | 2 |
| `jcode-*` crates | 62 | no | 3 |
| `.jcode` literals | 252 (98 in tests, 64 non-test files) | partly | 1 (home) / skip (rest) |
| docs, scripts, CI | 58 md, 49 py, 33 sh, 6 yml | yes | 4 |

## Phase 1 - core identity (the product)

Files:

- `Cargo.toml` - `[package] name`, `[[bin]] name` for the main binary,
  `version = "0.1.0"`, description. `[lib] name` stays `jcode`: internal, and
  keeping it avoids rewriting `use jcode::` for no visible gain.
- `crates/jcode-storage/src/lib.rs` - `jcode_dir()` returns `~/.kcode`;
  `app_config_dir()` and `hardening` use `kcode`; fallback runtime dir becomes
  `kcode-{uid}`.
- `crates/jcode-app-core/src/server/socket.rs` - `kcode.sock`,
  `kcode-daemon.lock`. `debug_socket_path()` derives from `socket_path()`, so
  one edit covers both.
- `src/cli/args.rs` - clap `name` and `about`.
- `src/cli/proctitle.rs` - process titles.
- Branding strings: `tui_launch.rs`, `turn_execution.rs`, `session_launch.rs`,
  `ui_header.rs`, `app/helpers.rs`.

Gate:

1. `cargo check --workspace` clean.
2. Build, run, confirm `~/.kcode` and `kcode.sock` are created.
3. Confirm it starts with **no env vars and no `--no-selfdev` hack** while the
   real jcode daemon is still running.

## Phase 2 - env var prefix (optional)

482 `JCODE_*` -> `KCODE_*` across `.rs`, `.py`, `.sh`, `.json`, CI, docs.

Care: test sandboxes isolate via `JCODE_HOME`/`JCODE_RUNTIME_DIR`
(`scripts/find_unlocked_env_tests.py` helps); vars leak into agent child
processes; do not touch non-`JCODE_` prefixes (provider SDK names).

Only worth doing once kcode has users who will type `KCODE_...`. Until then it
is a large mechanical diff with real breakage risk and no payoff.

## Phase 3 - crate rename (do not do)

62 `crates/jcode-*` -> `crates/kcode-*` plus every `use`. Invisible to users.
Pure churn, high conflict cost. Skip unless the source tree reading as `kcode`
is itself the goal.

## Phase 4 - docs, scripts, packaging

README and `docs/*.md`, 49 python and 33 shell scripts, CI artifact names
(`jcode-linux-x86_64` etc.), release workflow env.

Unrelated cleanup found while surveying: `scripts/mermaid_fit_probe.py` is dead
residue from the feature cut and should be deleted regardless of the rename.

## Phase 5 - repo split

New repo, push, keep `origin` as an upstream remote for selective cherry-picks.
Cheap once Phase 1 has landed, because identity already exists.

## What we deliberately skip

- Crate renames (Phase 3): no user value.
- Env var alias shims: permanent complexity, no benefit.
- Renaming the `[lib]` crate: internal.
- The ~200 remaining `.jcode` literals outside the central resolver (copilot
  machine id, schema dialect quirks, project-local dirs, tests): they would
  create a stray `~/.jcode` in a couple of edge paths. Harmless, cosmetic,
  cheap to fix later if it ever shows. Not now.
