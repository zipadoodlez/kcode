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

## Phase 2 - user-visible paths and branding (DONE)

Landed in commit `e5dae250`, 223 files.

- `~/.jcode` -> `~/.kcode` and `~/.config/jcode` -> `~/.config/kcode` in
  user-facing strings and docs (104 + 18).
- Home-based `.join(".jcode")` fallbacks that recreated a stray `~/.jcode`
  (copilot machine id, schema quirks, catalog caches, changelog marker) now
  use `.kcode`.
- Logs are written as `kcode-YYYY-MM-DD.log`; rotation still sweeps legacy
  `jcode-*.log` / `jcode-desktop-*.log` (decision 3: keep sweeping).
- Process titles, window titles, `/quit` and `/help` labels, CLI help and
  error strings, and `binary_stem()` (`target/release/kcode`).
- Project-local `./.jcode/` kept for compatibility (decision 1).

Not done, deliberately: the `jcode` SSH remote binary default
(`args.ssh_binary.unwrap_or("jcode")`). Changing it would break SSH into hosts
that have jcode installed; leave it and let `--ssh-binary` decide.

Verification: no new test failures. `jcode-base` 19 vs 19, `jcode-app-core`
24 vs 24, `jcode-tui` 31 vs 34, `jcode-logging` and the CLI binary suite pass.
The residual failures are the pre-existing flaky set.

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

1. **Project-local dir**: kept `./.jcode/` for compatibility with existing
   jcode projects. Revisit if the inconsistency starts hurting.
2. **Env var prefix**: kept `JCODE_*` (Phase 3, unchanged).
3. **Log rotation**: still sweeps legacy `jcode-*.log`.

## Already done in this pass

- Removed the dead `/memory` command surface (see README "Status").
- Dropped the stale `[ambient]` section from the default config template.
- Migrated user-visible paths, log filenames, process titles and branding
  (Phase 2 above).
