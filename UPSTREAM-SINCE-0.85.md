# jcode changes since v0.85.0

Untracked working note. kcode forked from jcode **v0.85.0** (release commit
`9d66a96d`, 2026-09-18). This file tracks what upstream jcode did after that, so
that once the refactor settles we can port back only the parts that still apply.

Not committed on purpose: it is scratch planning, not product docs. Delete or
move it whenever it stops being useful.

## Provenance

- Upstream: <https://github.com/1jehuang/jcode> (`master`).
- Local mirror used to build this file:
  `$KCODE_SCRATCH_DIR/jcode-upstream.git` (`~/.kcode/scratch/jcode-upstream.git`).
- Range covered: `v0.85.0 .. master` at `5f1c091cf` (2026-09-30).
- Releases since fork: v0.86.0, v0.87.0, v0.87.1, v0.88.0, v0.89.0, v0.89.1,
  v0.89.2, v0.89.3, plus 11 unreleased commits on `master`.
- Scale: ~398 non-merge commits, 643 files, +74k/-16k lines.
  Heaviest areas: `jcode-tui` (140 files), `jcode-base` (125),
  `jcode-app-core` (112), providers, `jcode-sdk` (16, excluded here).

Refresh the mirror before relying on this:

```bash
git -C "$KCODE_SCRATCH_DIR/jcode-upstream.git" fetch --tags --prune origin
```

## How to port a feature (smooth path)

Merge-in-upstream is a trap: the fork deleted whole subsystems (mermaid, replay,
memory/ambient, dictation, gmail/Google login, dashboard, computer-use, menubar,
installer, iOS, telemetry worker, TS SDK, jcode.sh account/subscription). A
merge would drag them back and collide with the removal commits and the
2026-09-28 `jcode -> kcode` rename (`6df93fd6`).

Port per feature instead:

1. **Pick commits.** Find the upstream change for a feature:
   ```bash
   git -C "$KCODE_SCRATCH_DIR/jcode-upstream.git" log --no-merges \
     v0.85.0..master -- crates/jcode-tui/src/...
   ```
   The release notes below tell you which release contains it; the appendix has
   every commit subject since v0.85.0.
2. **Work off the shared branch.** Another agent may be active on this branch,
   so do porting in a worktree, never in the main checkout:
   ```bash
   git worktree add .kcode/port-<feat> -b port/<feat> main
   ```
3. **Apply.** `git cherry-pick -x <sha>` first. Git rename detection usually
   maps `crates/jcode-*` to `crates/kcode-*` because the rename is a real commit
   in this tree, but expect conflicts. Fallback:
   ```bash
   git -C mirror diff <sha>^..<sha> > /tmp/p.patch
   git apply --3way /tmp/p.patch
   ```
4. **Do not blanket-rename `jcode` -> `kcode`.** The fork deliberately keeps the
   `JCODE_*` env-var prefix, the `jcode` provider id, and `jcode.sh` URLs for
   wire/config compatibility. See `docs/dev/post-change.md` under Names. Review
   every string hunk by hand.
5. **Skip patches you cannot use.** If a change touches only SDK, desktop,
   voice/dictation, memory/ambient, mermaid/diagram/image, account/subscription,
   telemetry or cloud, drop it (see the `N/A` tags below).

## Triage legend

- `PORT` - applies to the fork, worth doing.
- `MAYBE` - applies only partly, or needs a decision / new surface.
- `HAVE` - the fork already has an equivalent (the forker fixed some of these
  independently, e.g. the wrapped-row drag fix).
- `N/A` - touches a surface the fork removed.

## Ranked shortlist (do these first)

Ordered by value / effort for this TUI+CLI fork:

1. **Always-on adaptive status line** (v0.87.1, extended in v0.89.0). Directory,
   branch, git counts, context, provider, model pinned below the input; info
   widgets behind it. Highest-visibility quality-of-life change upstream made.
   - v0.87.1: `changelog/v0.87.1.json`; status-line commits in `944f747e9^..v0.87.1`.
   - v0.89.0: pinned line + info widgets + Changes/Commits widgets.
2. **Opus 5.5 + independent Anthropic model discovery** (v0.87.0, default in
   v0.88.0). Provider catalog work, low conflict.
3. **Provider-native server-side web search** (`engine = "native"`, unreleased).
   Rework of the existing `websearch` tool; fork keeps websearch.
   `c427cc191`, `11aa88461`, `3942ceee3`, `c5481cf61`.
4. **`/merge` and `--merge-remote-release`** (v0.86.0 / v0.88.0). Note the
   remote-release half assumes upstream CI; only `/merge` transfers cleanly.
5. **MCP deferred loading / dynamic args / lifecycle fixes** (v0.86.0, v0.89.0).
   No more prompt-cache busting when MCP servers are used.
6. **Cursor provider correctness** (v0.89.1, v0.89.2). Native MCP bridge, model
   id resolution, tool results returned.
7. **`edit` becomes a multi-replace tool; `multiedit` removed** (v0.88.0).
   The fork still has both `edit` and `multiedit`.
8. **Token-usage accounting fixes** (v0.86.0 cache-write tokens, v0.89.3
   counting failed/crashed/compacted turns).
9. **Session list / save bookmarks / untitled-session naming** (v0.88.0,
   v0.89.0).
10. **Swarm polish**: agents keep model/provider/effort on attach (v0.88.0);
    cross-swarm messaging and status dock (v0.89.0/0.89.1).
11. **Update-check fix for upstream-less checkouts** (v0.86.0). Directly
    relevant: this fork has no upstream.
12. **Ctrl+C behavior** (first clears input, second quits) and **embedded/multiple
    slash commands per message** (v0.89.0).

## Release-by-release notes

### v0.86.0 - "Smarter task handling and clearer session feedback" (2026-09-20)

- `PORT` Jev browser handoff handles whole multi-step tasks, returning to the
  parent only when done or blocked. `crates/kcode-*` browser/jev path.
- `N/A` Memory recall via Jev instead of a local embedding model (memory removed).
- `PORT` `/cache extend`; clearer cache usage, retention estimates, limits.
- `PORT` `/merge` integrates the current branch into the repo primary branch
  with validation of the combined result.
- `PORT` Terminal tab titles show session edit-line counts and elapsed time.
- `N/A` Jev prefers the subscription route (jcode.sh account removed).
- `MAYBE` Gated remote-compilation client (needs a provisioned service).
- `N/A` Native voice APIs / Nari streaming (dictation removed).
- `N/A` Rust/TypeScript client message-kind distinction; SDK SSH sharing.
- `HAVE` Pinned diff side panel removed (upstream removed it; fork has no panel
  dependency on it).
- `PORT` OpenAI WebSocket recovery when earlier tool outputs change / missing
  tool result.
- `PORT` Cache-write tokens retained in usage reporting; no OpenAI double-count.
- `PORT` Cache-retention estimates stop warning on inactive routes.
- `HAVE` Drag-to-copy follows visible text on wrapped lines (fork independently
  fixed this in `72192529`).
- `PORT` Todo quality checks stop pushing ownership/confidence follow-ups for
  completed work.
- `PORT` Edit results preserve file-position diffs; distinguish unchanged file
  from unavailable snapshot.
- `PORT` Fresh-session history available during active turns; socket messages
  survive interrupted reads.
- `MAYBE` Scheduled tasks reach attached sessions; `MAYBE` local client skills
  load before initial history (skills half applies).
- `PORT` MCP preserves dynamic args; `MAYBE` non-image file-drop normalization.
- `PORT` Source checkouts with no upstream skip misleading update checks. Directly
  relevant to this fork.

### v0.87.0 - "Claude Opus 5.5 and independent Anthropic model discovery" (2026-09-22)

- `PORT` Claude Opus 5.5 through Anthropic, explicit API-key and Claude-login
  routes in the model picker.
- `PORT` API-key and OAuth catalogs refresh independently.
- `PORT` Update Claude OAuth client compatibility to Claude Code 2.1.280.
- `PORT` Opus 5.5 adaptive thinking, progress summaries, thinking controls.
- `PORT` Keep last usable catalog on refresh failure; cache keyed by credential
  fingerprint.
- `PORT` Distinct "missing API key" vs "missing Claude login" labels.
- `PORT` Opus 5.5 context/output/reasoning/pricing metadata.

### v0.87.1 - "Always-on adaptive status line" (2026-09-22)

- `PORT` Status line always visible, adapts to width; right-aligned model.
- `PORT` Git status counts next to the branch.
- `PORT` Facts compact step by step on narrow terminals.
- `PORT` `display.overscroll_status = "overscroll"` restores old behavior.

### v0.88.0 - "Claude Opus 5.5 default, multi-browser, banked resets" (2026-09-23)

- `PORT` Opus 5.5 medium effort is the default; `anthropic-api:` uses the native
  Anthropic runtime.
- `PORT` Browser tool detects the default browser; Chrome/Edge/Brave/Chromium/
  Safari in addition to Firefox.
- `PORT` Reset exhausted OpenAI usage with banked resets (OpenAI account, not the
  removed jcode.sh account).
- `PORT` `edit` applies multiple replacements atomically, replacing `multiedit`.
- `PORT` Saved sessions always visible; `/save` bookmarks persist through writes.
- `N/A` Voice input / Nari.
- `PORT` Credential import never overrides existing logins.
- `PORT` OrcaRouter provider for `login` and `--provider`.
- `PORT` Parallel tool calls stream names/inputs as they arrive.
- `N/A` SDK per-session system prompts / tool policies / stop reasons.
- `PORT` Auth badge updates when switching OAuth <-> API-key.
- `N/A` `/merge-remote-release` (assumes upstream CI).
- `N/A` Desktop settings preservation (no desktop crate in fork).
- `PORT` Swarm agents keep model/provider/effort when their window attaches.
- `PORT` Proxy tool arguments preserved until stream completion.
- `PORT` Interleaved OpenAI streaming tool calls stay matched.
- `PORT` Todo quality checks stay quiet after the final response.
- `PORT` Long TLDR swarm summaries accepted; null bash notification flags fall
  back.

### v0.89.0 - "Built-in voice, agent applets, pinned status line" (2026-09-27)

- `N/A` Built-in voice input.
- `PORT` Pinned session status line + info widgets (continuation of v0.87.1).
- `MAYBE` Agents show interactive applets (new `applet-types` surface; port only
  if we want it).
- `PORT` Cross-swarm messaging with unique labels.
- `PORT` Changes widget: dirty files newest first, line counts, marks files the
  agent edited.
- `PORT` MCP tools load via provider-native deferred loading (no cache bust).
- `PORT` Account auto-switch pool with ordered failover (multi-account picker).
- `N/A` Upgrade card / plan feature allowances (jcode.sh account).
- `PORT` Daemon detects and reports KV cache misses to clients.
- `PORT` Untitled sessions named after first prompt; save labels name session.
- `PORT` Keep reading position across terminal resizes.
- `PORT` Embedded and multiple slash commands in one message.
- `PORT` First Ctrl+C clears input, second quits.
- `PORT` Pre-tool input transformers.
- `PORT` Yolo-Auto provider; grok-4.7/4.6 on Cursor. `N/A` OpenRouter image
  support (images removed).
- `MAYBE` Block lid-close sleep on macOS/Windows (Linux box here; skip unless
  porting for others).
- `N/A` SDK per-connection SSH identities / pinned host keys.
- `PORT` Fixes: MCP startup stalls/dupes, model-switch validation, effort chip,
  Ctrl+Up prompt-history draft, DeepSeek/MiMo context windows, Antigravity 429s,
  shared Claude usage limits across processes.

### v0.89.1 - "Cursor fixes, Commits widget, diagram pane key" (2026-09-29)

- `PORT` Cursor provider via native MCP bridge; `/fast`; reasoning models no
  longer stop mid-answer.
- `PORT` Commits info widget (recent commits, line counts, unpushed marker).
- `PORT` Swarm dock: per-agent status, activity, todo progress.
- `N/A` Diagram pane key, foot sixel for Mermaid (mermaid removed).
- `PORT` Info-widget headers/overflow/legends on borders.
- `MAYBE` Harness API can move a running tool to the background (fork keeps a
  harness binary).
- `PORT` agentgrep 0.1.7 keeps partial results on unreadable files.

### v0.89.2 - "Cursor MCP tool calls complete on every model" (2026-09-29)

- `PORT` Cursor MCP tool calls complete on Composer, Grok 4.7/4.6, Claude.
- `PORT` Switching to Cursor with `/model` no longer leaves MCP calls stuck.
- `PORT` Cursor model names resolve against Cursor's live model list.

### v0.89.3 - "Herdr support and /cloud session moves" (2026-09-29)

- `MAYBE` First-class agent in herdr (sidebar/agent list, `herdr agent wait`,
  pane reopens same session). Optional integration.
- `N/A`/`MAYBE` `/cloud` and `/local` session moves (needs the hosted service).
- `PORT` Usage telemetry counts tokens from failed-mid-stream, crashed,
  compacted and background responses.

### Unreleased on `master` (after v0.89.3)

- `PORT` Provider-native server-side web search (`engine = "native"`);
  prefer it by default, respect session tool policy, never replace an SDK custom
  tool with provider search, label downgraded native summaries.
- `PORT` Anthropic: only skip prefill repair for turns ending on a server tool
  block.
- `PORT` TUI test: retry rollback discards aborted native search rows.
- `PORT` OpenAI: end fresh WebSocket streams on terminal error frames.
- `PORT` Skill installs no longer invalidate the prompt cache.

## Appendix: every upstream commit since v0.85.0

### v0.85.0..v0.86.0
b9babdd9f 2026-09-07 fix: merge duplicate workflow environment maps (fixes #1191)
266ee1716 2026-09-07 fix: format TUI code exposed by restored CI (#1191)
17f57b08e 2026-09-10 fix: preserve explicit sponsor opt-outs across save/load (fixes #1188)
56ac1a170 2026-09-10 fix: create promoted task output before returning (fixes #1189)
38f7967a1 2026-09-10 fix: avoid rearming focus reports from focus events (fixes #1184)
ccf73ab8a 2026-09-10 fix: reap notification children without blocking callers (fixes #1187)
eb95f44c2 2026-09-10 fix: preserve dynamic MCP call arguments in provider schemas (fixes #1214)
d49923c4b 2026-09-10 fix: preserve cargo working directory outside jcode (fixes #1204)
9bedd88a2 2026-09-10 fix: acquire env before render locks in TUI tests (fixes #1201)
4e71f8724 2026-09-10 fix: normalize completed non-image file drops (fixes #1206)
47092fad4 2026-09-10 fix: deliver scheduled tasks to attached sessions (fixes #1194)
0e84b661a 2026-09-10 fix: load local client skills before initial history (fixes #1186)
fc9ef8348 2026-09-12 fix: handle empty API session directories (fixes #1234)
7f8447a3e 2026-09-12 fix: forward OAuth Bash timeout and execution schema (fixes #1223)
e2b9d7a1a 2026-09-12 fix: retain image input for DeepSeek Flash models (fixes #1221)
37a02790b 2026-09-13 test: initialize optional usage in CLI route fixtures (#912)
dd717ce25 2026-09-13 fix: report indeterminate source update checks (fixes #912)
2200140f5 2026-09-15 fix: honor ambient enable config edits (fixes #1269)
180e95db0 2026-09-15 fix: make copy badge fixtures platform-aware (fixes #1275)
c515991f2 2026-09-15 test: verify running ambient loop observes config edits (fixes #1269)
22320bc5c 2026-09-18 fix(api): expose side panel snapshots through harness and SDK
10c339193 2026-09-18 test(sdk): use EventStream timeout API for side panel routing
89d54e635 2026-09-18 ci: run Jev labels after completed Greptile reviews (#1308)
3824bf931 2026-09-18 ci: fix stale-base evidence in semantic PR labeling (#1311)
38d95c406 2026-09-18 feat(memory): add bounded Jev relevance recall engine
da092b112 2026-09-18 Add isolated Jev Decisions client and subscription capability gate
f89e4387a 2026-09-18 test(memory): add opt-in synthetic Jev live acceptance gate
e10c0eeb1 2026-09-18 fix(memory): minimize Jev candidate metadata disclosure
e63af4a1d 2026-09-18 Bind pending memory to project and revalidate selected facts before injection
c32f136a5 2026-09-18 Prefer included subscriber Jev over automatic BYOK routing
2e6e42325 2026-09-18 Replace automatic memory recall with embedding-free Jev decisions
3e9d12d0d 2026-09-18 Use persisted scoped memory in agent deferral test
0ef42d648 2026-09-18 Route memory interfaces through Jev and remove embedding startup work
37eaf331c 2026-09-18 Bind pending recall to exact Jev-selected metadata snapshots
937e01396 2026-09-18 Document observed Jev memory acceptance and rollout boundaries
7a00fe852 2026-09-18 Record successful graceful Jev daemon activation
a874649a5 2026-09-18 Record final whole-result Jev gateway live acceptance
5afd4655e 2026-09-18 fix: skip source update checks without an upstream
28821bde2 2026-09-18 docs: clarify desktop lives in a separate repository
84805d6b7 2026-09-18 feat(voice): add native Nari streaming and bounded PCM capture
8d5953314 2026-09-18 fix(voice): clamp final upsample frame to recording limit
781ed2a7c 2026-09-18 Add subscription-aware remote compilation with shared compute credits
0728476d4 2026-09-18 Document remote compute validation and rollout boundaries
0b4eb4253 2026-09-18 Record batch regressions and bounded Worker runtime validation
58e319b71 2026-09-18 fix: persist authoritative file-position diffs in edit results
5541e4584 2026-09-18 fix: distinguish known no-op file diffs from unknown snapshots
eebc4996f 2026-09-19 docs: update weekly stars chart
c923f72eb 2026-09-18 fix(sdk): expose framed assistant messages and final answers
51884da05 2026-09-18 test(sdk): enforce harness protocol minor version parity
271df5c77 2026-09-18 fix: retain history snapshot guard across nonblocking fast path (#1284)
9a7be7bed 2026-09-18 fix(protocol): frame assistant text messages and preserve retry corrections
0b8179f20 2026-09-18 test: verify assistant text framing through daemon and harness sockets
b487e4222 2026-09-18 fix(api): retain fragmented daemon frames across request cancellation
6dcf591aa 2026-09-18 test: verify Guy issue fixes through real provider SDK acceptance
5424d8785 2026-09-18 feat: add bounded Jev voice session intent classification
6fa9c60b0 2026-09-18 fix: resolve explicit newest voice conversation requests
df34e9273 2026-09-18 fix: serve busy fresh-session history without persisted snapshot (#1284)
465610a58 2026-09-18 test: repeat fresh-session SDK acceptance after confidence audit
053c9d6c9 2026-09-18 docs: map final whole-result rerun to Guy issue requirements
5101ded76 2026-09-18 feat(sdk): add owned shared SSH transport for independent clients
c62973a44 2026-09-18 Reuse authenticated SSH transports across independent SDK clients
0c96a0815 2026-09-18 feat(tui): add merge-to-main slash command
b23f61316 2026-09-18 fix(tui): validate combined merge result before reporting success
451fb7225 2026-09-18 style: apply workspace Rust formatting
50c4533fb 2026-09-18 test(browser): add isolated paired handoff benchmark harness
831171a47 2026-09-18 feat(browser): default to subscription-first Jev handoff
600b8b51f 2026-09-18 fix(browser): enable isolated benchmark debug readiness
725f4c51c 2026-09-18 test(browser): require productive handoff for timing eligibility
48da481ec 2026-09-18 docs(browser): record fresh-session handoff timing and rollout blockers
acde90a25 2026-09-18 Persist Anthropic cache duration and support /cache extend
db75c8013 2026-09-18 fix(openai): align cache retention policy with auth routes and current API
3d18adf90 2026-09-18 docs(browser): verify handoff on real public documentation sites
7d0a7743c 2026-09-18 Report cache baseline retention separately from saved preference
b26624625 2026-09-18 docs(browser): audit test counts and pin measured build artifact
f9185a14e 2026-09-18 fix(tui): snapshot cache retention and label estimates honestly
b3a005e41 2026-09-18 Avoid cache expiry warnings for estimates and inactive provider routes
8343408df 2026-09-18 fix(openai): preserve cache write token usage
c0be37ce1 2026-09-18 Price OpenAI cache-write tokens as an input subset
8ef938219 2026-09-18 Track per-request cache prompt totals across providers and history
be248c641 2026-09-18 Add GitHub Sponsors funding link (#1317)
4973d3920 2026-09-18 Suppress proactive cache countdowns for estimated retention
4d91642f0 2026-09-18 Fix cache notification regression fixture type inference
73558a0d8 2026-09-18 Expose actual cache expiry notification policy in /cache diagnostics
8225cae95 2026-09-19 fix(tui): keep drag-to-copy aligned on wrapped rows
a62d04dd2 2026-09-20 docs: update weekly stars chart
e5e428544 2026-09-19 Remove pinned diff side panel mode
91c0bdda3 2026-09-19 fix(tui): keep drag-to-copy aligned on wrapped rows
f28e346a7 2026-09-20 Make Jev browser handoff own multi-step task execution
c5882da59 2026-09-20 Keep uncertain browser exploration inside Jev handoff
a776881ac 2026-09-20 Trace Jev handoff requirements to final live acceptance evidence
939fb1aa0 2026-09-20 Record post-commit rerun of all mapped handoff checks
97e6a5110 2026-09-20 Refine Jcode base system prompt
135ca5b9e 2026-09-20 fix: reduce false-positive todo ownership and confidence checks
3d400e8db 2026-09-20 fix(openai): replay changed persistent input prefixes
1a591fffc 2026-09-20 feat(tui): show session edit counts and work time in terminal titles
d2dcb6151 2026-09-20 test(tui): cover wrapped copy content at narrow widths
24838926e 2026-09-20 docs: refine base agent guidance
2173c7eb1 2026-09-20 test(todo): align schema checks with concise follow-up guidance
b18d0ed80 2026-09-20 test(browser): expand paired benchmark suite with enforced direct-only arm
e589cbe5a 2026-09-20 chore(release): prepare v0.86.0

### v0.86.0..v0.87.0
2a4edaa02 2026-09-21 docs: update weekly stars chart
ef4c2bd69 2026-09-22 docs: update weekly stars chart
b20c02a72 2026-09-22 fix(anthropic): bump Claude CLI user agent to 2.1.280
24b704586 2026-09-22 docs: explain direct Anthropic API login and model routing
750fb60f5 2026-09-22 feat(anthropic): support Opus 5.5 mandatory adaptive thinking
6582741f0 2026-09-22 fix(anthropic): isolate API and OAuth model catalogs and route availability
de9b5feaa 2026-09-22 fix(anthropic): refresh auth routes independently and shape Opus 5.5 requests
578ea23ef 2026-09-22 test(anthropic): assert independent availability for both picker routes
3aff81fd5 2026-09-22 fix(anthropic): reshape model fallback requests and preserve bound thinking
f4420947b 2026-09-22 chore(release): prepare v0.87.0
774d93ba6 2026-09-22 fix(anthropic): honor scoped OAuth catalog over legacy plan gate

### v0.87.0..v0.87.1
80eeb5666 2026-09-23 docs: update weekly stars chart
be68f8d90 2026-09-22 tui: adaptive overscroll status line, pinned on by default
944f747e9 2026-09-22 chore(release): prepare v0.87.1

### v0.87.1..v0.88.0
54df8d159 2026-09-20 feat(sdk): add TypeScript session tool controls and callbacks
6f65f5ff9 2026-09-20 fix: preserve proxy tool arguments until stream completion (fixes #1326)
d8b39ab58 2026-09-20 feat(sdk): enforce session tool policies and owner-scoped callbacks
3b2272ab8 2026-09-20 Clear account-pinned daemon cooldown after OpenAI usage reset
52315ce13 2026-09-20 Show account-scoped banked reset hint at OpenAI usage exhaustion
baee4a175 2026-09-20 Add safe OpenAI banked usage reset client and quota refresh
0448a8d27 2026-09-20 Suppress local reset availability hint for SSH sessions
1ac3e6a24 2026-09-20 Fix reset hint test buffer extraction scope
4a2e9478e 2026-09-20 feat(tui): add confirmed banked OpenAI usage reset command
f1b4363c4 2026-09-20 feat(sdk): support persisted per-session system prompt overrides
9158ef0c3 2026-09-20 test: import transfer helper for prompt inheritance coverage
adbf48bd3 2026-09-20 feat(sdk): expose structured abnormal turn stop reasons
8962a203f 2026-09-20 feat(streaming): expose parallel tool names and keyed input eagerly
46af73120 2026-09-20 fix(openai): stream interleaved tool calls with stable keyed events
b6375bb14 2026-09-20 fix(tui): keep interleaved streaming tool inputs keyed by call
f7e17eeab 2026-09-20 feat(tui): add merge and remote release command
1d8388635 2026-09-20 fix(cli): accept tool input event metadata in consumers
5c9c33ee2 2026-09-20 fix(auth): preserve scriptable login JSON through validation
ada84e8fc 2026-09-20 fix(runtime): retain busy turn owner for attached successor
2ede271a9 2026-09-20 Document audited browser handoff timings and current credit blocker
3815e1c16 2026-09-21 feat(voice): expose real microphone RMS on NariRecording
74f9b75be 2026-09-21 fix(swarm): accept TLDR summaries above recommended length
4d36cfaee 2026-09-21 fix(api): invalidate idle snapshots on pre-stream turn activity
10032bae9 2026-09-21 fix(bash): accept null notification flags as defaults
7684b6105 2026-09-21 fix(serve): allow credential-free daemon onboarding
9c5aded89 2026-09-21 Expose read-only usage counts by authentication method
b040ce9bb 2026-09-21 fix(sdk): release OAuth callbacks on retry and surface safe login failures
697038bd9 2026-09-21 feat(voice): classify coding-agent intent and bounded quick actions
856d961d9 2026-09-21 fix(voice): batch Jev questions and expose validated reports
0ba3bb90e 2026-09-22 docs: explain direct Anthropic API login and model routing
3b61d8373 2026-09-22 feat(anthropic): support Opus 5.5 mandatory adaptive thinking
ded299fe3 2026-09-22 fix(tui): align model suggestion provider and method columns
43d1779eb 2026-09-22 fix(anthropic): isolate API and OAuth model catalogs and route availability
117925809 2026-09-22 fix(anthropic): refresh auth routes independently and shape Opus 5.5 requests
48a947eef 2026-09-22 test(anthropic): assert independent availability for both picker routes
ce944ab5d 2026-09-22 fix(tui): repair reset review flow and show credit expirations
2ac26c252 2026-09-22 fix(tui): keep todo quality gates quiet after final response
670e0922c 2026-09-22 fix: route voice Jev through Typesafe-backed providers
0c2f188fa 2026-09-22 fix(voice): select highest-scoring concrete intent without thresholds
49b8d4a54 2026-09-22 Propagate resolved credential on ModelChanged so auth badge updates on OAuth/API switch
ec291ff67 2026-09-22 fix(voice): deduplicate classification policy within bounded requests
88e4c1f47 2026-09-22 feat(voice): send product vocabulary as Nari recognition context
b20758361 2026-09-22 feat(voice): report Jev token usage and published Jev/Nari prices
29552405d 2026-09-22 Expose /save labels in SessionInfo so API clients can search them
e2a088e05 2026-09-22 Share pretty model names from jcode-provider-core for Desktop
8c0fc615d 2026-09-22 fix(voice): open mic during Nari handshake so first words are kept
d2a1f3010 2026-09-22 Persist /save through the daemon so later session writes keep the bookmark
1533e8cd1 2026-09-22 Always list saved sessions even past the recency limit
e1576e9e3 2026-09-22 feat(browser): detect the user's browser and support Chrome, Edge, Brave, Chromium and Safari
edcdb1087 2026-09-22 Route anthropic-api: model specs through the native Anthropic runtime
8c03ce410 2026-09-22 Make Claude Opus 5.5 at medium effort the default Anthropic model
a88c36d72 2026-09-22 Add shared metered_usage_cost_usd helper for per-token spend
b52e5d64b 2026-09-22 Add streaming_session_ids for cheap daemon-wide running-session polling
4690cff90 2026-09-22 tui: 2-row minimum composer, quieter right-aligned reversed overscroll line
c64d90da7 2026-09-22 tui: revert composer to 1-row minimum
ac9398ad8 2026-09-22 tui: compact overscroll context bar, keep dir visible, add git status
e8606152c 2026-09-22 tui: adaptive overscroll status line, pinned on by default
2690526a7 2026-09-22 base: move external auth review into jcode-base for desktop onboarding
34b54a466 2026-09-22 base: read-only recent transcript sampler for Desktop onboarding
1b37d5735 2026-09-22 app-core: restore multiedit.rs until its mod.rs removal lands
42285059b 2026-09-22 tools: merge multiedit into atomic edit, add replace, nudge bash edits
ac0b5780b 2026-09-22 app-core: drop orphaned multiedit.rs (merged into edit)
eaa95ac1f 2026-09-23 macos: hide notification helper, drop CLI Spotlight launcher; rustfmt fixes
2fda4f518 2026-09-23 external_auth: expose canonical provider ids on review candidates
567c3dadd 2026-09-23 external_auth: import that never shadows logins Jcode already has
65bd862c4 2026-09-23 Jev voice routing: remove the uncertain outcome, unclear input goes to the coding agent
029151721 2026-09-23 config: round-trip [desktop.*] tables so CLI saves never wipe Desktop settings
71af92dab 2026-09-23 voice: keep Nari handshake off the mic-readiness path
29127c2b6 2026-09-23 voice: NariRecording starts without blocking on mic or network
ec4319d4b 2026-09-23 style: cargo fmt --all
780e3901b 2026-09-23 voice: correct Jeff to Jev and J code to Jcode in transcripts
5056ebac7 2026-09-23 fix: persist prepared sessions and repair test/clippy drift
ee4cd3db3 2026-09-23 chore(release): prepare v0.88.0

### v0.88.0..v0.89.0
66948f696 2026-09-23 voice: describe Jev in the Nari recognition prompt, doubling exact recognition on hard audio
6666b9b02 2026-09-23 fix: make clippy -D warnings clean across the workspace
700a493ea 2026-09-23 Add native email code account sign-in to the SDK
0a6e96e45 2026-09-23 fix: budget MiMo v2.6 models at their 1M context window (fixes #1401) (#1417)
35874182b 2026-09-23 fix: skip skills already imported by an earlier source on first run (fixes #1404) (#1416)
1eaa5a9b3 2026-09-23 fix(macos): make Jcode Desktop the only Jcode app launcher (#1408)
53098c4d8 2026-09-23 fix: correlate Responses tool events by stable output identity (fixes #1336) (#1338)
8c33c4990 2026-09-23 Add Gmail search link for account sign-in emails, including spam
35976823b 2026-09-23 feat(tui): clear input on first Ctrl+C, quit on second (#1418)
4913d40b9 2026-09-23 cursor: add grok-4.7 and grok-4.6 to fallback model catalog (#1377)
4d1e19527 2026-09-23 deps: bump tar, cmov, rand and fast-uri past their published advisories (#1375)
7031acf6d 2026-09-23 ci(release): publish and announce only from the canonical repository (#1372)
acc14b66d 2026-09-23 ci(freebsd-smoke): declare least-privilege workflow permissions (#1371)
baa2bcfb8 2026-09-23 fix(test): read the jev mock socket in blocking mode (#1355)
c161349be 2026-09-23 Fix typos in README (#1334)
1e2079193 2026-09-23 fix: keep 1M context for renamed DeepSeek Flash id (#1396)
63bc9d930 2026-09-23 fix(auto): detect configured OpenAI-compatible profiles (#1385)
25ead746b 2026-09-23 style: cargo fmt after #1385
bc89f485f 2026-09-23 fix(server): validate model switch against target, not current provider (#1384)
2ca8a59f9 2026-09-23 fix(test): drop unused test-module imports in jcode-app-core (#1364)
df07e7e68 2026-09-23 fix(tui): snap tail-follow viewport on resize instead of animating (#1424)
1dcca5741 2026-09-23 tui: retain the prepared chat frame as the published geometry (#1425)
29398b166 2026-09-23 fix: strip provider prefix before matching configured default_model (#1283)
d9f5f8394 2026-09-23 fix(tui): consume focus events and log stream errors in turn loops (#1285)
75dd137b8 2026-09-23 feat: add pre-tool input transformers (#1279)
4cff1fb6b 2026-09-23 fix(tui): keep the draft when Ctrl+Up jumps into prompt history (#1378)
7ddd98e91 2026-09-24 Preserve enabled=false when importing Codex MCP servers (#1328)
ed8d80408 2026-09-23 fix(tui): stop the live git probe leaking real repo state into tests (#1366)
938e74af6 2026-09-23 Onboarding replay samples the longest recent session from Jcode, Claude Code, Codex, Cursor or Pi
1bc6fdd6a 2026-09-23 feat(tui): support embedded and multiple slash commands (#1278)
c4c3b6272 2026-09-24 fix(tui): give drag-edge autoscroll its own step and cadence (#1400)
842477f10 2026-09-24 style: attribute palette overrides by role identity, not color proximity (#1399)
7680d8b54 2026-09-23 fix(tui): stop two account tests asserting labels the store never assigns (#1368) (#1368)
15a46be63 2026-09-24 fix(session): prune stale active-pid markers left by the exec reload (#1419) (#1419)
f57ccf806 2026-09-23 style: cargo fmt after #1419
07ced4bab 2026-09-23 Add Claude session-limit reset client and banked reset reporting for GUI clients
95c276bd2 2026-09-23 fix(macos): gate test-only and unused setup-hints imports
96053d00a 2026-09-23 test: split synthetic AWS key so the secret-scan preflight passes
6af993e83 2026-09-23 fix(macos): gate paused_jcode_shell_command to tests
268abfa84 2026-09-23 deps: bump h2 to 0.4.19 and rustls to 0.23.45 past RUSTSEC-2026-0258/0285
080500f8d 2026-09-23 style: satisfy clippy manual_contains in slash command parser
e181af502 2026-09-23 feat: add Yolo-Auto provider (#1432)
b772d6f8f 2026-09-23 perf: skip full session scan when resuming an unknown generated session id
953c25e29 2026-09-23 fix(jev): retry transient 429/5xx/529 overload on the same route with bounded backoff
4834a881a 2026-09-23 style(jev): rustfmt
3e4f0933e 2026-09-23 refactor(providers): remove the Claude Code CLI subprocess transport
c81ed221c 2026-09-23 refactor(bedrock): resolve aws login profiles via the SDK instead of the aws CLI
e5122e2bd 2026-09-23 voice_intent: evaluate Jev batches concurrently
7a3479cd7 2026-09-23 harness bridge: stop per-event thread churn and return retained heap
a33bb7e65 2026-09-23 refactor(grok-build): talk to the Grok CLI chat proxy over HTTPS instead of ACP
27244d337 2026-09-23 style(grok-build): collapse nested if in auth retry stream (clippy)
69800d750 2026-09-23 perf(jev): hedge voice routing across fresh connections, single Typesafe batch
5bedd9334 2026-09-23 jev: prefer Typesafe direct over OpenRouter in auto routing
b65931032 2026-09-23 sdk: support per-connection SSH identities and pinned host keys
c1424ea74 2026-09-23 selfdev: add reload-bridge to safely restart the harness API bridge
b60d6663b 2026-09-24 harness-api: report requested working_dir for unpersisted new sessions
8870f1993 2026-09-24 bridge: handle --help; bound reload-bridge preflight
691c6f526 2026-09-25 docs: update weekly stars chart
8fd8148ba 2026-09-24 fix(hooks): never let a hook flash a console window on Windows (#1490)
71663532f 2026-09-24 Add jcode-applet-types: versioned, validated wire types for Desktop applets
d0aad5517 2026-09-25 applet-types: stacks default to no padding (containers own insets)
74577fe83 2026-09-25 protocol: report reasoning effort on model_changed so clients never show a stale level
b3947e186 2026-09-25 voice: cut push-to-talk release latency and profile the stop path
2066c6548 2026-09-25 Voice: bias and correct Jcode vs Jev (Jev Desktop -> Jcode Desktop)
0dca548f3 2026-09-25 Sessions: save labels name the session and set_session_saved reaches the SDK
caaf0945a 2026-09-25 fix(telemetry): raise D1 soft limit to 8 GB now that paid storage is metered
8ff1e1dfc 2026-09-25 dictation: resolve the focused Jcode CLI session for global voice holds
e019bbd3f 2026-09-25 docs(telemetry): disclose confidential de-identified usage summaries for investor due diligence
e0828f21e 2026-09-25 jev: label browser decisions so the gateway budgets them apart from memory
accda144e 2026-09-25 Prompt an upgrade when a Jcode plan limit is reached
434a8f9c4 2026-09-25 applets: agent applets across SDK, protocol, server, applet tool and MCP-UI bridge
4291419c7 2026-09-25 usage: show Jcode daily feature allowances and an upgrade hint
60f552176 2026-09-25 Accounts: persistent auto-switch pool; same-provider failover rotates only pool members in user order
00c1d655b 2026-09-25 memory: prefilter Jev recall to 72 lexical candidates
ca4a13eb2 2026-09-25 mcp: load MCP tools through provider-native deferred loading so adding them never busts the prompt cache
8f1dd86b7 2026-09-25 mcp: rebuild the tool snapshot when deferred-loading support changes mid-session
73ba84af5 2026-09-25 usage: name the Jcode row 'Jcode subscription' so Desktop attaches it
b30236373 2026-09-25 sessions: name untitled sessions after their first prompt
a62ff65a3 2026-09-25 voice: drop silent prompt-echo transcripts (repeated 'Hey Jev / Okay Jev' examples)
b7bb4041a 2026-09-25 applet-types: order capabilities so hosts can keep grant sets
25a974bf8 2026-09-25 power: block lid-close sleep on macOS and Windows while sessions work
1cde4f059 2026-09-25 ci: repair master clippy, fmt and test drift
824394a91 2026-09-23 feat(tui-messages): content-coordinate viewport anchors
24741e31d 2026-09-23 fix(tui): keep the reader's message under them across a resize
3a0159e84 2026-09-23 fix(tui): scope the resize anchor to a paused viewport
50f11e47e 2026-09-23 style: rustfmt the anchor module and the resize accessor
634b48615 2026-09-23 fix(tui): never hold a resize anchor across a compacted-history prepend
243b128d6 2026-09-23 fix(tui): anchor rows that belong to no message (live streaming output)
c98c2dd97 2026-09-23 fix(tui): let a resize during a pending prepend win the frame
c17969051 2026-09-25 fix(tui-messages): drop useless usize conversion now that wrapped_len is usize
b5a4cde7a 2026-09-23 refactor(tui): prompt-jump reads prompt rows from the retained frame
3dcdc80eb 2026-09-26 fix(mcp): fail pending requests when a stdio server exits
efbfd2f7d 2026-09-26 openrouter: honour input modalities declared in the model catalog
7c1e22f83 2026-09-26 transport: probe Windows named pipes without a Tokio reactor
7f6ca8d1a 2026-09-26 openrouter: consult the persisted catalog for image support
6c83e653b 2026-09-26 transport: probe remove_socket on its own bound pipe
e372fcb8a 2026-09-26 applet tool: teach select options as {value,label} and host.send_prompt{prompt}; test documented shapes validate
40bf41971 2026-09-25 voice: built-in voice input for the CLI, like Jcode Desktop
28536e51b 2026-09-26 voice: native mic capture for the CLI on macOS and Windows
2173da405 2026-09-26 swarm: cross-swarm DMs via to_swarm, unique swarm labels, list_swarms directory
acd5fed1e 2026-09-26 Login validation: memory-less agents skip plan-limit notices so auth smoke replies stay exact
aba49c024 2026-09-26 voice: drop transcripts that recite the recognition prompt on silence
98d2e5711 2026-09-26 mcp: auto mode never busts the prompt cache on any provider
40e9634e6 2026-09-26 Show an Upgrade card in the chat when a Jcode plan limit is reached
6a3f652e0 2026-09-26 accounts: make the first auto-switch account the default route for new sessions
cfc2d831b 2026-09-26 todo tool: tell models to update the list after nearly every step
6eed478d4 2026-09-26 test(tui): drop duplicated applets field in History fixtures so the lib tests compile
6a3ce79ab 2026-09-26 fix: TUI effort chip adopts reasoning_effort from model_changed (fixes #1504)
85b7105d6 2026-09-26 fix: detect alternate-enter keymap conflicts and make its hint follow queue_mode (fixes #1500)
6fdab0b77 2026-09-26 test: isolate openrouter_like_status test from named-profile env (refs #1479)
05d390097 2026-09-26 feat: daemon-side KV cache miss detection with kv_cache_miss event for all clients
97dedb75e 2026-09-26 accounts: sync auto-switch order with every default-provider change; Jcode subscription defaults to last
a6ff2ba88 2026-09-26 mcp: sanitize MCP tool names for every provider
468cea294 2026-09-26 tui: info widgets become a detail layer behind the overscroll line
31915cb84 2026-09-26 tui: end-to-end draw tests for detail-layer widgets and pink overscroll model
36bb476ff 2026-09-26 dev_cargo: run agent cargo builds at nice 10 so they cannot starve interactive UIs
c52d0cb7c 2026-09-27 docs: update weekly stars chart
c3d6b7bb4 2026-09-27 fix(reload): exit non-zero when the new server never becomes ready
0d17f4753 2026-09-26 tests: real-App overscroll reveal/rebound with pink model; legacy overscroll_status config still loads
4116b0082 2026-09-26 tests: full-frame Context mix renders breakdown without line totals
436d9e761 2026-09-26 tui: remove the Context mix info widget
b8b1b25aa 2026-09-26 harness-api: report the daemon-resolved credential as auth_method
1f29d734f 2026-09-26 sdk: run a stdin script over an SSH connection's pinned options
2353f4b09 2026-09-27 tui: Changes widget shows line counts, agent edits, newest first
f5dc18ad2 2026-09-27 tests: Changes widget end to end on a real git repo
8fff043ca 2026-09-27 voice: coding agent terms in built-in dictation vocabulary, document personal vocabulary
691e1bb04 2026-09-27 usage: share Anthropic OAuth quota and 429 backoff across processes, keep last good limits when throttled
232e49238 2026-09-27 provider-core: add pretty_picker_model_name for readable model picker titles
60a9a1a5c 2026-09-11 fix(test): drop poisoned session-list cache before resume benchmark loads
08f6c9ba4 2026-09-24 test(tui): isolate onboarding telemetry tests from machine opt-out env
48ac65683 2026-09-24 test(tui): assert platform-rendered alt chord in alignment hint
cda802e4f 2026-09-24 test(tui): hermetic ambient refresh and panic-safe env restore in cost test
0cb301e0f 2026-09-27 test(tui): keep onboarding telemetry opt-out safe
acf25f8a8 2026-09-27 test(tui): guard session cache against stale in-flight loads
4c156fe58 2026-09-27 test(tui): assert onboarding telemetry marker persistence
8556d6fcc 2026-09-27 fix(usage): inspect Claude resets before limits and preserve confirmed zero
8343cda96 2026-09-27 tui: label the agent-edit dot in the Changes widget
6821cc949 2026-09-27 test(tui): scope session cache publication hook to loader
cc2171473 2026-09-27 Derive swarm message previews instead of requiring tldr
c13ef1f61 2026-09-27 test(tui): force cache race loader to scan
782ea5216 2026-09-27 test(reload): cover server reload exit statuses
447551c55 2026-09-27 fix(mcp): avoid startup lock stalls and duplicate spawns
39f8e7404 2026-09-27 test(reload): restore test home and assert JSON output
f8650baca 2026-09-27 fix(mcp): clear cancelled owned connection attempts
f2d5f66da 2026-09-27 test(reload): isolate no-listener socket
d945b30da 2026-09-27 fix(antigravity): use daily-cloudcode-pa endpoint to fix consumer-account 429s
3f8c856b8 2026-09-27 fix(antigravity): address greptile review — override-aware project lookup, endpoint validation, catalog invalidation
eb49d7950 2026-09-27 fix(antigravity): don't abort project lookup on a non-JSON loadCodeAssist response
7ee491c8e 2026-09-27 test(mcp): serialize concurrent owned connect test with provenance tests
29d2f11f1 2026-09-27 style: rustfmt antigravity provider after merge
0d3625740 2026-09-27 tui: pin the session status line permanently below the input
9929ee0ea 2026-09-27 chore(release): prepare v0.89.0

### v0.89.0..v0.89.1
92d868767 2026-09-24 test(browser): keep the hanging CLI fixture a single process
645b1b62a 2026-09-24 test(jev): allow loaded clients to reach the mock listener
7acc781b4 2026-09-24 fix(auth): recognize test binaries in custom Cargo target directories
8f8062293 2026-09-24 test(panel): search the global bus for the first panel event
3ae25f228 2026-09-24 test(panel): inject unrelated traffic before the expected snapshot
b8aa0bcda 2026-09-24 test(bash): hold the environment lock during the scratch-directory probe
c9b6e3c01 2026-09-24 test(e2e): opt scenario homes out of telemetry
f5215c06d 2026-09-24 fix(mermaid): detect foot as a sixel terminal
9b65e775f 2026-09-24 fix(agentgrep): bump agentgrep to v0.1.7 so rg exit-2 partial results are not discarded
f74f6751e 2026-09-28 docs: update weekly stars chart
4c4d9651c 2026-09-27 anthropic: keep tool_results leading merged user turns with tool references
09559f26f 2026-09-28 fix(swarm): bring three swarm parameter descriptions under the 25-token cap
38dde5aa1 2026-09-28 fix(openrouter): collapse the nested if in catalog_declares_image_input
7c3a50559 2026-09-28 test(tui): scope two whole-screen scans to what they assert
2efdffad1 2026-09-28 test(tui): pin truecolor in colour tests instead of inheriting COLORTERM
7d93787bd 2026-09-28 cursor: native MCP tool bridge, base model ids with fast param, working /fast
8eb6c66cb 2026-09-28 tui: avoid blank Kitty images inside Zellij
c9c278195 2026-09-28 desktop_selfdev: reach the shared single-panel Desktop host
2ede76066 2026-09-29 docs: update weekly stars chart
c556f2db6 2026-09-28 tui: Alt+M cycles side panel split -> fullscreen -> hidden
6ca159354 2026-09-28 tui: Alt+M also cycles pinned diagram pane split -> fullscreen -> off
aece97322 2026-09-28 transcript_sample: return several largest transcripts across harnesses for onboarding showcase
14df6c064 2026-09-28 harness api: expose background_tool (move running tool to background)
a7ed4c131 2026-09-28 tui: fullscreen diagram pane shows the whole diagram (contain fit)
fe8b9b433 2026-09-28 Revert "tui: fullscreen diagram pane shows the whole diagram (contain fit)"
e34070ae7 2026-09-28 Revert "tui: Alt+M also cycles pinned diagram pane split -> fullscreen -> off"
a526dd5e1 2026-09-28 tui: drop dot separators from overscroll status line
4f6bf8e04 2026-09-28 Replay reasoning before the text it precedes in transcript samples
e205d0f65 2026-09-28 tui: info widgets put headers, overflow, and legends on their borders
fdad4643d 2026-09-28 tui: add ignored info widget gallery test for visual review
e4b4d3ad6 2026-09-28 tui: swarm dock lists each agent with its activity
c4e3be925 2026-09-28 tui: add Commits info widget
6b938ea84 2026-09-28 tui: verify commits parsing against a real git repo
49e720caf 2026-09-28 dev_cargo: parallel rustc front-end on stable via a workspace wrapper
0da8f4006 2026-09-29 Add headless multi-turn RAM benchmark vs Claude Code
6c33b79ca 2026-09-29 tui: Alt+Shift+M shows/hides the diagram pane, Alt+M stays side-panel only
84d95f4e7 2026-09-29 style: cargo fmt (fixes Format / Quality Guardrails on master)
571de3b84 2026-09-29 fix(desktop_selfdev): bring instance description under the 25-token cap
c943a29dc 2026-09-29 fix(clippy): allow the intentional label-registry test lock, name the publish hook type
2dfaa1ccc 2026-09-29 dev_cargo: apply the parallel front-end through RUSTC_WRAPPER
8c7415b02 2026-09-29 chore(release): prepare v0.89.1

### v0.89.1..v0.89.2
88332e8f5 2026-09-24 test(tui): pin the Updates box empty in the live batch prepare test
56392f252 2026-09-24 test(copilot): keep save_github_token_creates_config_dir out of the real config
6906ae3f5 2026-09-24 test(communicate): isolate JCODE_HOME so server tests ignore the real agents.swarm_model
dc16df5f5 2026-09-25 test(tui): serialize and scope changelog render fixtures
29161388f 2026-09-26 test(copilot): isolate trust config via JCODE_HOME and restore env on drop
9e04dbcfd 2026-09-26 test(communicate): prove spawn isolation against a conflicting outer config
cf9370a06 2026-09-26 test(communicate): clear JCODE_SWARM_MODEL in the isolation test
c454df770 2026-09-29 fix(cursor): route native MCP tool results back to the Cursor stream
2cab3a1eb 2026-09-29 fix(cursor): answer AgentService mcp_state requests so MCP tool calls proceed
9cdf00162 2026-09-29 fix(cursor): send model ids Cursor actually serves (grok-4.6, claude-opus-5-5, ...)
2475895e3 2026-09-29 fix(cursor): answer AgentService allowlist prechecks instead of stalling
ebc402bf3 2026-09-29 chore(release): prepare v0.89.2

### v0.89.2..v0.89.3
76df6464b 2026-09-29 feat(cloud): /cloud and /local move a live session to a cloud host and back
52fae65e5 2026-09-29 telemetry: per-response usage_report with correct model/session attribution
babcb06b8 2026-09-29 telemetry: emit usage_report from the local TUI turn loop
dbbe60c93 2026-09-29 telemetry: count usage from agent streams that fail mid-response
6ec6323d1 2026-09-29 feat(tui): report agent state, resume command, and release to herdr
de65ade33 2026-09-29 chore(release): prepare v0.89.3

### v0.89.3..master
3272c0372 2026-09-30 docs: update weekly stars chart
c427cc191 2026-09-30 feat(websearch): provider-native server-side web search (engine = "native")
c5481cf61 2026-09-30 websearch: label downgraded native search summaries
e33916627 2026-10-01 docs: update weekly stars chart
024b9d847 2026-09-30 agent: skill installs no longer invalidate the prompt cache
a908f4d4d 2026-09-30 fix(openai): end fresh websocket streams on terminal error frames
11aa88461 2026-09-30 websearch: prefer provider-native search by default; respect session tool policy
4e3e4f513 2026-09-30 test(tui): retry rollback discards aborted native search rows
907196a1a 2026-09-30 anthropic: only skip prefill repair for turns ending on a server tool block
3942ceee3 2026-09-30 websearch: never replace an SDK custom websearch tool with provider search
5f1c091cf 2026-09-30 sdk: document provider-native web search and test its bridge events
