# Work in progress

The index of what is not finished, so it does not get lost between passes. This
file is expected to shrink.

When an item lands, delete its row. When a plan completes, delete the plan doc -
git has the history.

## Plans in flight

| plan | state | what's left |
|---|---|---|
| [plans/tuistate-decomposition.md](plans/tuistate-decomposition.md) | analysis only, nothing extracted | refresh the 114-method categorization to the current 156, then extract leaf sub-traits one per commit starting with a single-file consumer, keeping `ui.rs` and `ui_viewport.rs` on the supertrait |
| [plans/browser-provider-protocol.md](plans/browser-provider-protocol.md) | draft spec, no implementation | tighten the core method set and the normalized `page.snapshot` format before building any adapter |

## Committed ideas, no plan doc yet

| idea | the problem |
|---|---|
| compile-time isolation | build and link time; the workspace recompiles far more than it should. The old `COMPILE_TIME_ISOLATION_REFACTOR.md` was a historical record naming removed machinery, so it was deleted - the idea stays here |
| the axis kcode owns | as a subtraction fork on MIT code, kcode has no moat on anything inherited; only what it *adds* is ownable, and today that is nothing. Leading candidate: verifiability plus package-manager ownership - reproducible builds, no self-modification, no telemetry, permission-gated by default, every performance claim shipped with a runnable script and raw artifacts in-repo |

## Open items in the code, no plan

| item | state |
|---|---|
| `JCODE_*` env vars | the state dir is `~/.kcode` but the env prefix was never renamed. Decide: rename with a `JCODE_*` fallback, or document as-is |
| dead SSH-block commands | `/theme`, `/stats`, `/file`, `/open`, `/permission`, `/permissions`, `/new-terminal`, `/debug-fixture` are blocked over SSH but have no handler anywhere, so they do nothing locally either |
| `client:mermaid:*` debug help | `server/debug_help.rs` still lists ~9 mermaid/image debug commands after the mermaid removal. Some (`mermaid:ui-bench`) have a tester handler, so this needs verifying, not a blind delete |
| `-p` vs `provider list` | `-p` accepts 52 provider choices; `provider list` prints 26. The extras are aliases and gateways with no catalog entry |
| `/help <item>` coverage | written detail for 70 of 114 registered commands; the other 44 answer `Unknown command`, and there is no `/help list` |
| packaging | `packaging/arch/PKGBUILD` does not exist, and the README install section points at it |
| unknown config sections | the loader silently ignores unknown top-level sections, so older config files keep dead keys with no warning |
| pre-existing test failures | on `main`, `jcode-tui --lib` has 31 failing tests, the math/LaTeX suite 15 (`fuzz_*`, `test_*_math_*`), and `test_lock_order` 1. They are environmental/order-flaky, not regressions. Treat them as the baseline so a change is not diffed against a clean tree every time |
| irrelevant tests | the suite still covers removed features and carries many brittle pixel/color assertions. Collapse or delete rather than keep maintaining them |
| `[dictation]` in README | `README.md:297` lists the section; the feature is gone, leaving four dead env names in `config.rs:59-62` |

## Repo hygiene, no plan

| item | state |
|---|---|
| CI is upstream's | `discord-release.yml`, `update-star-history.yml`, `require-issue.yml`, `freebsd-smoke.yml` describe jcode, not this fork. Prune to kcode-shaped workflows |
| dead config | `Cargo.lock` is committed (correct for a binary) but still listed in `.gitignore:3`; `.gitignore` also names paths absent here (`/.jcode/generated-images/`, `/telemetry-worker/backups/`, `graphify-out/`, `captures/`, `ios_simulator_screenshot.png`) |
| `scripts/` | ~90 inherited files, no README; several are explicitly jcode-specific. Classify keep/delete/broken or delete |
| budget baselines | the six budget files (`panic_budget.json`, `swallowed_error_budget.json`, `code_size_budget.json`, `warning_budget.txt`, `wildcard_reexport_budget.json`, `test_size_budget.json`) carry jcode's numbers; re-baseline or they are meaningless or block work |
| fork policy | rebase lane vs hard divergence is undecided, and it blocks crate names, the env prefix and the provider cut. `README.md` states "does not track upstream", but nothing follows from it |
| licensing | no `license` field on the root `Cargo.toml` or any of the 63 members; add `license = "MIT"`, ship the LICENSE inside the package (the PKGBUILD), and generate a `THIRD_PARTY_NOTICES` from `Cargo.lock` |

## Provider layer

| item | state |
|---|---|
| provider cut | 20 `jcode-provider-*` crates, 61k lines, for roughly three wire formats (OpenAI-compatible, Anthropic, Gemini). ~19k cut candidates (`cursor-runtime`, `copilot*`, `antigravity*`, `grok-build-runtime`, `claude-cli-runtime`, `bedrock`, `provider-doctor`, `provider-metadata`/catalog); realistic target ~12-18k. Caution: the 46.8k non-test lines are accumulated production bugfixes - delete as they bite, do not rewrite blind |

## Hook surface gaps

Found while writing `user/hooks.md`.

| gap | detail |
|---|---|
| `turn_start` `SOURCE` | only `chat` is ever emitted; the schema advertises `chat`/`resume`/`ambient`, and `ambient` belongs to a removed mode. Narrow the contract or wire the resume path |
| `session_start` schema comment | stale: says `create`/`resume`; the code emits `create`, `attach`, `resume` |
| hooks are unobservable | no `/hooks` command, no listing of configured hooks, no dry-run. A typo in a command string looks identical to a hook that runs and does nothing |
| blocked calls are invisible | `pre_tool` stderr goes to the model; nothing tells the user their policy blocked a call |
| hook failures | logged and dropped, with no user-visible signal |

## This doc set

The rebuild is complete: `user/` (7), `internals/` (9), `dev/` (4), `plans/` (3),
plus `README.md`, `what-was-removed.md`, and this file at the root. Notes that
outlive it:

- **Install** stays in the root `README.md`; there is no `user/install.md`.
- The repo-root `OAUTH.md` is folded into `user/providers.md` + `user/auth.md`.
- **Dropped as unshipped/speculative** (git has it): the multi-session
  protocol-multiplexing phases and open questions, and Herdr's upstream TODO list.
