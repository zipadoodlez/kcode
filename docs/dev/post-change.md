# After a change

There is no CI on this fork. The inherited GitHub Actions workflows were removed:
they ran kcode's pipeline (Discord release announcements, Homebrew/AUR
publishing, PR-issue enforcement, Greptile labelling), none of which applies
here. Nothing runs automatically now, so the gate below is it. Run it yourself,
or tell an agent to run it, after a major change and before committing.

## The gate

```sh
scripts/check_guardrails.sh              # non-zero on failure
scripts/check_guardrails.sh --fix        # rustfmt + rebaseline ratchets
scripts/check_guardrails.sh --skip-slow  # skip cargo clippy
```

It must pass. It runs the old CI guardrail set locally: `cargo fmt --check`,
`cargo clippy -- -D warnings` (which also compiles every target), `Cargo.lock`
freshness, the size, wildcard, and `App`-shape ratchets, crate dependency
boundaries, and the onboarding state-space invariants.

A compile or clippy failure is a real regression; do not commit past it.

## Knowledge graph

`graphify-out/` is a local cache and is not tracked. Build it once per clone
with `graphify update .` (about a minute on this tree, AST-only, no API key).
After that the committed `.githooks/post-commit` and `post-checkout` rebuild it
for free: incremental over the files the commit touched, and detached, so it
never blocks a commit. `core.hooksPath` is local git config and does not travel
with a clone, so run `scripts/setup_git_hooks.sh` once per clone to point git at
the hooks.

Query the graph instead of grepping for architecture questions:

```sh
graphify query "how does the transcript store state"
graphify god-nodes
graphify path "ServerEvent" "TuiState"
```

graph.json records the `built_at_commit` of the last commit that changed the
code topology, so a graph that lags a structural change is visible. Only
`graph.json` and `GRAPH_REPORT.md` are tracked; the cache, the
interpreter/root sidecars, and the HTML view are gitignored. The 44 MB JSON is
excluded from text diffs and union-merged on conflict (see `.gitattributes`).

## Tests

`cargo test` runs the workspace; target a crate while iterating
(`cargo test -p kcode-tui --lib`). The suite has pre-existing failures on this
tree, so a red run is not automatically your change: see
[testing.md](testing.md) for the known `kcode-tui` parallel-execution race and
the `--test-threads=1` workaround, and check any suspect in isolation before
blaming it. `scripts/test_ci_suites.py` runs the suites with per-suite timing if
you want a readable local report.

## Budget ratchets

Several ratchets (`scripts/check_*`) are baselined to this fork's tree, so a
red one is this fork's own drift rather than inherited kcode numbers. When
growth is intentional, re-baseline the specific file with `--update` (or
`--fix` above) in the same commit; otherwise fix it. Do not `--update` to
silence a ratchet you did not mean to move.

The two size ratchets (production and test files) also fail when a tracked file
*shrinks*, so an improvement has to be recorded in the same commit. The cap then
only ever tightens: an unrecorded shrink would leave the old, looser cap in
force and let the file regrow to it unnoticed.

Two ratchets were deleted because a count cannot make the distinction the rule
needs. The swallowed-error ratchet counted `let _ =`, `.ok()` and
`.unwrap_or_default()`, which are idiomatic Rust, so its signal was dominated by
code that is not a defect. The panic ratchet counted `.unwrap()`, `.expect()`
and `panic!` in production paths: it could not tell a justified
`.expect("invariant")` from a careless `.unwrap()`, so it blocked correct new
code, and the one place it pointed at (the SSH-login flow's guarded unwraps) is
tracked as work in `docs/todo.md` §2 instead. If the policy is ever wanted back,
clippy's `unwrap_used`/`expect_used`/`panic` lints express it with a per-site
`#[allow(..., reason = "...")]`.

Three more steps were removed for being redundant rather than wrong: a
`mod`-without-file pre-check (rustfmt reports the missing file itself), a
separate `cargo check` (clippy compiles every target, so it covers the same
compile errors and the pair built the tree twice), and the warning budget
(clippy's `-D warnings` denies the same warnings over a wider target and feature
set). `cargo machete` was removed because it never ran here.

## By change type

- **UI / behavior change**: build with
  `cargo build --profile selfdev -p kcode --bin kcode`, then exercise it. There
  is no `selfdev build-reload` in this fork: that tool targets upstream's `jcode`
  package, which does not exist here. `cargo build` alone proves nothing about
  behavior; the running session is served by the installed daemon, so run the
  fresh binary against its own socket when you need to observe it.
- **Public behavior, commands, or config change**: update `docs/user/` and, if
  the mechanism changed, `docs/internals/`. Those docs describe the code as it
  is today; a doc that disagrees with the code is wrong.
- **Introduced or removed a concept**: record it in `docs/todo.md`.

## Names

This tree uses two names on purpose. Before you "fix" one of them, know which
kind it is:

- **`kcode`** is this product: the binary, the crates, `KCODE_*` variables, the
  `.kcode/` state and project directories, the tools.
- **`jcode`** names the upstream project this fork came from, or the service it
  still speaks to. These are correct and must stay:

  | where | why |
  | --- | --- |
  | the `jcode` provider id, the `Jcode subscription` labels | existing config and session files carry the id |
  | `jcode.sh` URLs, the `1jehuang/jcode` repo, the `1jehuang/homebrew-jcode` tap | upstream's service and repository, not ours |
  | ACP `_jcode` capability, `_meta.jcode` keys | clients already speak these strings |
  | `changelog/` entries | they record what shipped, and history is not rewritten |

Everything else is `kcode`, and there is no compatibility alias for anything
that was renamed: `JCODE_*` is not read, `.jcode/` is not consulted. Do not add
a shim to soften that; the point of the rename is that one name means one thing.

## Environment

Set `TMPDIR` to real disk, not tmpfs, for cargo builds: `ring` and `aws-lc-sys`
build scripts fail with "Disk quota exceeded" on a small RAM-backed tmpfs.
