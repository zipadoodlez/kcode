# After a change

There is no CI on this fork. The inherited GitHub Actions workflows were removed:
they ran jcode's pipeline (Discord release announcements, Homebrew/AUR
publishing, PR-issue enforcement, Greptile labelling), none of which applies
here. Nothing runs automatically now, so the gate below is it. Run it yourself,
or tell an agent to run it, after a major change and before committing.

## The gate

```sh
scripts/check_guardrails.sh              # non-zero on failure
scripts/check_guardrails.sh --fix        # rustfmt + rebaseline ratchets
scripts/check_guardrails.sh --skip-slow  # skip cargo check/clippy/machete
```

It must pass. It runs the old CI guardrail set locally: module declarations
resolve, `cargo fmt --check`, `cargo check --all-targets --all-features`,
`cargo clippy -- -D warnings`, `Cargo.lock` freshness, the warning budget, the
size/panic/swallowed-error/wildcard ratchets, crate dependency boundaries, the
onboarding state-space invariants, and `cargo machete` when installed.

A compile or clippy failure is a real regression; do not commit past it.

## Tests

`cargo test` runs the workspace; target a crate while iterating
(`cargo test -p jcode-tui --lib`). The suite has pre-existing failures on this
tree, so a red run is not automatically your change: see
[testing.md](testing.md) for the known `jcode-tui` parallel-execution race and
the `--test-threads=1` workaround, and check any suspect in isolation before
blaming it. `scripts/test_ci_suites.py` runs the suites with per-suite timing if
you want a readable local report.

## Budget ratchets

Several ratchets (`scripts/check_*`) carry jcode's numbers and currently trip
because the fork grew past them. When growth is intentional, re-baseline the
specific file with `--update` (or `--fix` above) in the same commit; otherwise
fix it. Do not `--update` to silence a ratchet you did not mean to move.

## By change type

- **UI / behavior change**: build and reload in one step with
  `selfdev build-reload`, then exercise it. `cargo build` alone proves nothing
  about behavior; the running session is served by the installed daemon.
- **Public behavior, commands, or config change**: update `docs/user/` and, if
  the mechanism changed, `docs/internals/`. Those docs describe the code as it
  is today; a doc that disagrees with the code is wrong.
- **Introduced or removed a concept**: record it in `docs/wip.md` and, if it is
  planned work, `docs/plans/`.

## Environment

Set `TMPDIR` to real disk, not tmpfs, for cargo builds: `ring` and `aws-lc-sys`
build scripts fail with "Disk quota exceeded" on a small RAM-backed tmpfs.
