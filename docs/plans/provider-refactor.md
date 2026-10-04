# The provider refactor

Status: audit complete, nothing landed. Eight read-only sweeps of the tier
(2026-10-04), plus a read of maki as the reference implementation. Every finding
below is one checkbox with its evidence beside it, cited at `file:line`, and each
is independent except where a line names a gate. Counts are measured at the step,
not read from here. Raw sweep reports (long form, same evidence):
`~/.kcode/scratch/provider-audit/*.md`.

## The root, in one line

The tier's cost is not the wires, it is that **provider identity is written down
about 20 times and converted between 19 ways**: a crate pair per provider, an
`ActiveProvider` enum, a `RuntimeKey`, a `ModelRouteApiMethod`, an `api_method`
string, a session `provider_key`, a session `route_api_method`, a `LoginProviderTarget`,
a `LoginProviderAuthStateKey`, protocol newtypes that dissolve to `String`, and
nine label/key/display tables. The crate split itself is a compile-time
workaround, not a design: nine `-runtime` crate headers say they exist "so
provider edits do not rebuild the base -> app-core -> tui spine", and
`kcode-base/src/provider/external.rs:1-20` is the registry that hides the
concrete types.

## Size in scope (2026-10-04)

| region | non-test | total |
|---|---|---|
| 9 `kcode-provider-*-runtime` transport crates | 18,832 | 33,307 |
| 6 provider wire crates (anthropic, openai, openrouter, gemini, copilot, antigravity) | 6,098 | ~6,900 |
| `kcode-base/src/provider/*` + `catalog_routes.rs` + `provider_catalog.rs` | 11,755 | 19,746 |
| `kcode-provider-core` (trait + shared types) | 7,302 | 7,302 |
| `kcode-provider-metadata` + `kcode-provider-env` | 2,312 | 2,489 |
| `kcode-provider-bedrock` | 1,981 | 1,981 |
| `kcode-provider-doctor` | 4,269 | 6,701 |
| `kcode-base/src/auth` | ~12,000 | 19,669 |
| TUI provider surfaces | 14,384 | ~15,700 |
| `kcode-tui-account-picker` | 1,301 | ~1,400 |
| CLI provider surfaces (`src/cli`) | ~7,140 | ~9,000 |
| adjacent: `kcode-base/src/live_tests.rs`, `kcode-schema-dialect` | 3,080 + 2,657 | same |
| **in-scope total** | **~92,000** | **~125,000** |

## The target

Minimal surface that does 90%+ of the job, whose extension points make a new
provider shape a data row or one module, never a crate. Four nouns, nothing else:

1. **`Codec`** - the only per-wire Rust. Four methods: build request, parse
   stream, fetch catalog, count tokens.
2. **`Declaration`** - the only per-provider fact: slug, display, `codec` (or a
   `base` to borrow a bespoke one), auth kind and env, base url, headers, model
   rows (prefixes, tier, vision, prices, limits), thinking dialect, capability
   flags (compaction, service tiers, tools-internally, deferred tools,
   transport), optional body hook.
3. **`Transport`** - http-sse | websocket | process-stdio | aws-sdk, with one
   send/stream/retry driver (the audit found four verbatim copies).
4. **`Registry`** - slug to declaration to live provider; one slot lookup, no
   enum arms.

### The 90% line

"90%" is functionality, not usership, and it is structural, not measured (the
local usage ledgers are unreliable and back nothing). The core carries request
building, streaming, tool calls, reasoning/effort, images, retry with
`Retry-After`, usage accounting, catalog and pricing, login, and the picker's
route list, over four network wires: `openai-chat` (the compat wire the
declaration table rides, so it serves OpenRouter, the 42 `OpenAiCompatibleProfile`
rows, Copilot's chat path and every API-key endpoint by construction),
`openai-responses` (+ websocket, prewarm, chatgpt-web), `anthropic-messages`
(+ OAuth preflight, thinking budgets, 1M context), and `google-generate`
(+ the Code Assist envelope; antigravity is a dialect of it).

The remaining 10% is real but not core, one module each behind the same
boundary: AWS SDK (bedrock), raw h2+protobuf (cursor), ACP over a child process
(grok-build), a CLI subprocess (claude-cli), OpenAI's browser-cookie/web-socket
corners. `kcode provider-doctor` becomes a consumer of the codecs.

### Malleability gate

Met when all three hold, proven by the change that adds the next provider:

- a compatible endpoint = **1 declaration row**, no Rust, no crate;
- a new wire = **1 codec module + 1 enum arm**, no `kcode-base` edit;
- picker, login, docs, catalog and usage pick it up with no edit elsewhere.

## The spine: one identity, one row, one credential value

The cross-cutting finding, present in all four boundaries. One provider id type
plus the declaration row plus one credential/route value deletes about **19
conversion tables, 10 enums and 9 label tables**, measured at ~1,930 non-test
lines outside the provider crates alone (see F1-F4, F8, F10-F13, and E4, C4,
D3).

The vocabularies today, by definition site: `ActiveProvider` (8) at
`provider-core/selection.rs:5-14`, 521 refs in 24 files; `provider_key()` /
`provider_label()` (`selection.rs:82-106`); `RuntimeKey` (15 + `Other`) at
`provider-core/lib.rs:699-728`; `ModelRouteApiMethod` (14 + `Other`) at
`lib.rs:867-895`, with `parse` at `:910` and 539 `api_method` references in 60
files; `api_method` as the wire and sqlite spelling (`lib.rs:681`); `AuthRoute`
(`auth_mode.rs:48-127`, 5 output spellings at `:194-253`); the session's
`provider_key` (`base/session.rs:138`) and `route_api_method` (`:145`);
`LoginProviderTarget` (14) at `provider-metadata/lib.rs:25-40` (207 refs in 19
files); `LoginProviderAuthStateKey` (`lib.rs:93-105`); protocol `AuthProviderId`
/ `RuntimeProviderKey` / `CatalogNamespace` (`kcode-protocol/lib.rs:79,93,107`,
dissolved to `String` at `lifecycle.rs:47`); `ProviderAvailability` (9 bools) at
`selection.rs:17-27`; ledger source keys (`provider_activity.rs:310-391`,
written by `provider/mod.rs:697-746`); `MultiAccountProviderKind`
(`usage/model.rs:296-315`); `NativeProviderKind` (`provider-doctor/provider_e2e.rs:1308-1329`);
dialect ids (`schema-dialect/registry.rs:14,74,107,165,185,205`).

The conversion tables to delete or shrink to a lookup, all named in the sweeps:
`AuthRoute::parse` + `parse_explicit_credential_prefix` + 5 spelling fns
(`auth_mode.rs:142,160,194-253`); `RuntimeKey::from_api_method`/`stable_id`/
`routed_model_spec` (`lib.rs:731,755,816`); `ModelRouteApiMethod::from_auth_route`/
`parse`/`api_method_kind` (`:899,910,1124`); `parse_provider_hint`/
`provider_from_model_key`/`cli_provider_arg_for_session_key`
(`selection.rs:68,108,138`); `provider_key_from_hint` (`models.rs:135`);
`derive_session_provider_key` (`session.rs:293`); `session_provider_key_from_provider_name`/
`_matches_provider_name`/`canonical_session_provider_key`
(`selection.rs:224,309,327`); `model_switch_request_for_session_model`/
`_for_session_route` (`selection.rs:352,444`, the 85-line six-vocabulary ladder);
`normalized_auth_provider_id` (`lifecycle.rs:831`); `route_matches_activation`
(`:743`); `model_switch_request_for_provider_id` (`:1133`); `source_key_for_provider_label`
(`provider_activity.rs:310`); `history_provider_name_from_session`
(`client_state.rs:53`); `model_usage::key` (`:17`) and `serving_route`
(`:178-191`); `normalize_model_route_provider_labels_match` alias matrix
(`provider-core/lib.rs:1014-1050`).

Already drifted, proof that this is bug-shaped and not cosmetic: four metadata
profiles disagree with base's own static lists (`opencode-go`, `cortecs`,
`comtegra`, `huggingface`); azure is named four ways (`azure`, `azure-openai`,
`aoai`, and the `azure-openai-responses` stray); `-p google` is advertised by
`provider-core/selection.rs:156` and `scriptable.rs:261` but rejected by clap
(`provider_init.rs:28-47`); `source_key_for_provider_label` matches by substring
(`contains("openai")` at `provider_activity.rs:360`); the resume path silently
falls back to auto on an unknown key (`selection.rs:400-432`, a shipped bug of
exactly this class).

## Findings

Format: move - what it removes. `surface` counts concepts/representations,
`lines` is non-test. Ranked within each boundary, biggest per risk first.

### A. Wires and transport crates

- [ ] **Copilot onto the compat codec - condense**: the wire crate (306 lines)
 is a reduced copy of openrouter's builder (`kcode-provider-copilot/src/lib.rs:60-241`
 vs `openrouter/src/request.rs:49-400`), and its SSE machine
 (`copilot-runtime/src/lib.rs:698-886`, 189 lines) re-implements
 `openrouter/src/stream.rs:34-479`. Keep Copilot's headers, tier detection and
 premium accounting as declaration fields. surface −2 (builder, parser),
 lines −1,200 to −1,500, risk low.
- [ ] **One send/stream/retry driver - unify**: "Retries use a fresh unpooled
 client" is verbatim in four crates (`openai_provider_impl.rs:454`,
 `openrouter_sse_stream.rs:80`, `anthropic-runtime/src/lib.rs:1684`,
 `copilot-runtime/src/lib.rs:487`); five `is_retryable_error` wrappers
 (`openai_stream_runtime.rs:1627`, `anthropic-runtime:2146`,
 `openrouter_sse_stream:308`, `copilot-runtime:889`, `claude-cli-runtime:1071`);
 gemini's 172-line helper quartet (`gemini-runtime/src/lib.rs:354-525`) is four
 copies of one send; claude-cli's `lib.rs:695-710` a sixth. surface −4 loops,
 lines −1,000 to −1,300, risk low-med.
- [ ] **Gemini and Antigravity - unify**: `antigravity/src/lib.rs:280-423`
 mirrors `gemini/src/lib.rs:571-673`; shared types already live in
 `kcode-provider-gemini`; 246 lines are line-for-line identical after rename.
 One codec, `dialect = "antigravity"`. surface −1 wire, lines −500 to −700,
 risk med.
- [ ] **Test consolidation - condense**: three `EnvVarGuard`s
 (`anthropic_tests.rs:3`, `gemini_tests.rs:4`, `openai_tests.rs:22`), a
 `SharedEnvLock` (`openrouter_tests.rs:13-34`) and ~15 loopback fake servers;
 `openai_tests/parsing_tools.rs` (688 lines) tests functions owned by other
 crates; `copilot_tests.rs` (~350 of 762) re-covers the openrouter builder. One
 fake-SSE harness under the codec seam. surface −3 guards, −1 harness,
 lines −1,200 to −1,800 (test), risk near-zero.
- [ ] **Crate merge - rearrange**: 20 provider crates toward one, downstream of
 `kcode-base`, preserving the rebuild property through `external.rs`. Only 30
 `use kcode_provider_*::` sites in 19 files sit outside the tier, so the merge is
 mostly mechanical. surface −19 crates, lines −1,000 to −2,000 (boilerplate),
 risk med.
- [ ] **Tail transports - keep behind the boundary** (capability facts, not
 cuts): OpenAI persistent WS + prewarm + chatgpt-web ~3k
 (`openai_websocket_prewarm.rs:37-268`, `chatgpt_web.rs:13-80,162-531`); native
 compaction `openai_provider_impl.rs:1091-1201`; cursor raw h2+protobuf 1,005
 (`agent_transport.rs:180-434,445-620`); grok ACP 880; gemini Code Assist
 envelope; copilot device exchange; anthropic OAuth preflight. Floor estimate for
 the nine runtime crates: 10.5-11.5k non-test with every capability kept.

### B. `kcode-base/src/provider` and `provider-core`

- [ ] **The `ActiveProvider` dispatch fan-out - re-core**: 56 `match provider`
 sites / 285 arms in the tier, 24 of them `match self.active_provider()` (130
 arms, all `mod.rs:1574-2801`), 169 `ActiveProvider::` refs in `mod.rs`, and
 `complete_on_provider` / `complete_split_on_provider` near-duplicates
 (`dispatch.rs:24-321`). One slot lookup replaces every arm. surface −1 concept
 in 56 places, −1 enum, lines −500, risk med.
- [ ] **The model-metadata fan-out - unify**: one Claude model lives in 7
 places, one OpenAI model in 6. `ALL_CLAUDE_MODELS` (`core/models.rs:13-27`) is
 byte-identical to `base/provider/anthropic.rs:73-87`;
 `base/provider_catalog.rs:342-659` is 318 lines of `push()` lists over 29 arms;
 `metadata/catalog.rs` is 94 consts; `auth/lifecycle.rs:303,320`;
 `core/anthropic.rs:158-172`; `core/pricing.rs`; bedrock's 114-line if-chain
 (`bedrock/src/lib.rs:857-970`). One table, id lookup.
 surface −5 representations, lines −1,500 to −2,000, risk med.
- [ ] **`kcode-provider-metadata/src/catalog.rs` - condense**: 1,071 of 1,284
 lines are 94 consts; only 10 have a production consumer outside the crate and
 both id resolvers exist (`provider_catalog.rs:309`, `metadata/lib.rs:232`). Two
 table literals. This is the row shape noun 2 needs. surface −1 file,
 lines −600 to −800, risk med.
- [ ] **`kcode-provider-core/src/openai_schema.rs` - delete**: 755 lines
 superseded by `kcode-schema-dialect`'s `OPENAI` spec, whose comment says it is
 "the sanitizer this replaces" (`registry.rs:56-63`); one live caller
 (`kcode-provider-openai/src/request.rs:54-57`); openrouter already uses the
 dialect (`openrouter/src/request.rs:21`). surface −1 sanitizer, lines −755,
 risk med.
- [ ] **Catalog caches and glue - unify**: `model_cache.rs` is the clean
 mechanism but `models.rs:30-57,307-352` runs a parallel store; the
 unavailability structs are declared twice (`models.rs:53-57` vs
 `model_catalog_service.rs:6-10`); OpenRouter carries a ~300-line twin cache
 (`openrouter-runtime/src/lib.rs:598-894`); seed/fetch/store/publish is repeated
 in five runtimes. surface −1 cache layer, −4 glue copies, lines −300 to −500,
 risk low-med.
- [ ] **Route builders - unify**: 42 `ModelRoute {` sites; the Anthropic and
 OpenAI families are each built in four places (`catalog_routes.rs:36-141`,
 `:332-453`, `:940-987`, `route_builders.rs:121-180`). Routes fall out of the
 declaration table. surface −3 builders, lines −300, risk low.
- [ ] **`provider_catalog.rs:342-720` - rearrange**: static per-profile model
 and context tables already owned by metadata's `default_model`, the live
 `/models` cache and the family classifier. lines −350 to −500, risk low-med.
- [ ] **The `Provider` trait - split**: 58 methods, 3 required; 40 implemented
 by ≤2 of 10 backends, 11 by nobody, and 10 of those 11 exist only for
 `MultiProvider` (49/58 overridden in a 1,246-line impl). Split into the codec
 seam (the 8 methods every backend has) and the orchestrator surface, read off
 the capability flags. Enabled by the dispatch item. surface −1 fused interface,
 lines −200 to −400, risk med-high.
- [ ] **Dead weight - delete**: `model_cache.rs:47-72` (`load_model_list`/
 `store_model_list`, zero callers), `provider/claude.rs` (8 lines, zero items),
 the constant-true `openai_compatible_profile_model_supports_chat`
 (`provider_catalog.rs:661-663`), test-only scaffolds (`models.rs:592-646`,
 `provider_catalog.rs:665-674`), dead pricing fns (`core/pricing.rs:86,228` plus
 four production-dead siblings, ~132 lines), 11 dead `pub` items in core, the
 diverged tool-name copy (`claude-cli/src/lib.rs:1080-1135` vs
 `core/anthropic.rs:368-398`). surface −15 dead items, lines −400, risk none.
- [ ] **`kcode-provider-doctor` - condense onto the codecs**: 4,269 non-test
 (probes 1,874, runner 2,380, plus a 1,980-line test-only driver); a sixth
 request-building implementation for eight runtimes it already depends on
 (`Cargo.toml:30-36`); 39 production reaches into `kcode-base::live_tests`. Make
 it a codec consumer. surface −1 sixth implementation, lines −1,000 to −1,500,
 risk med.
- [ ] **Bedrock - split behind the boundary**: 775 of 1,981 lines under
 `cfg(feature = "aws-sdk")`; base names its concrete type in 15 places and needs
 three pre-construction statics a factory cannot give (`has_credentials`,
 `is_bedrock_model_id`, `configured_bearer_token`: `mod.rs:354`,
 `accessors.rs:53`, `startup.rs:127`, `catalog_routes.rs:627,883`,
 `auth/mod.rs:554,604,1094`). Zero lines saved until those three become
 capability flags, so it is last. surface −15 call sites, risk high.

### C. Auth (`kcode-base/src/auth`, ~12,000 non-test)

- [ ] **Generic external-source consent - condense**: 13 near-identical fns in
 `provider_init.rs:356-870` (515 lines) differing only in label, source and
 login hint, all three of which the declaration row knows.
 surface −12 fns, lines −400, risk low-med.
- [ ] **Per-kind login, not per-provider - re-core**: one begin/complete per
 kind (`browser-oauth`, `device-code`, `api-key`, `cli`, `local`) reading urls,
 scopes, client id, input kind and account name off the row. Removes the
 provider arms in `scriptable.rs:58-174,228-263,541-548`, `login.rs:286-335`,
 and 6 API-key paste copies (`login.rs:493-517,547-573,590-615,1106-1129`,
 Cursor `:961-985`, Bedrock `:631-647`). surface −12 arms, lines −300, risk med.
- [ ] **Per-kind auth-state/assessment - condense**: `mod.rs:507-897` (~390
 lines) becomes per-kind functions reading `api_key_env`, token store and
 external sources off the row. surface −4, lines −250, risk med.
- [ ] **Row-fields instead of id tables - rearrange**: `lifecycle.rs:344-361`
 (model orders), `:494-522` (frontier families), `:885-902` (aliases),
 `:943-992` (env bindings), `:1133-1160` (switch prefixes), plus
 `doctor.rs:18-33` and `activation.rs:194-212`. surface −7 tables,
 lines −260, risk low.
- [ ] **One Google OAuth - unify**: `gemini.rs` + `antigravity.rs` are the same
 program twice (246 normalized-identical lines), with twin refresh wrappers over
 a shared `google_oauth::refresh_access_token`. surface −1 implementation,
 lines −240, risk med.
- [ ] **Account CRUD through `account_store` - unify**: `claude.rs` and
 `codex.rs` each hand-roll ~165/170 lines of account CRUD while
 `account_store.rs:1-11` states the one-mechanism intent and only Claude and
 Codex use it. surface −2 implementations, lines −200, risk low.
- [ ] **TUI re-implements the login flows - condense** (gate: C2): ~1,200 lines
 in `tui/app/auth.rs:406-1578` plus `handle_login_input:1579-2255` duplicate the
 CLI driver. Two front-ends, one flow library. surface −9, lines −700, risk high.
- [ ] **Four diagnostics rosters - unify**: `kcode auth doctor`
 (`auth/doctor.rs` 228), `kcode auth test` (`src/cli/auth_test/*` 2,170),
 `kcode provider-doctor` (`provider_e2e.rs:1308-1400`), and
 `kcode provider-test-coverage` (`live_tests.rs:1704-1712`) each carry their own
 provider roster; keep one roster plus a tier selector. surface −20,
 lines −300, risk med.
- [ ] **Dead and misfiled - delete**: `auth/login_flows.rs` (49, zero callers),
 `transfer::available_local` (7, zero callers), the Gemini CLI resolver cluster
 (`gemini.rs:106-184,604-640`, ~66, no non-test callers), the 1-line empty
 `auth/tests.rs`, the stray `azure-openai-responses` id (`external.rs:592`),
 `test_sandbox.rs` (157, test-only code shipped in the production crate),
 `auth/commands.rs` (130, PATH lookup misfiled in auth, keep but move).
 surface −6, lines −300, risk none.
- [ ] **`transfer.rs` onto the shared store - condense**: `secure_publish`
 (~130 lines) alongside `storage::write_json_secret`, two re-declared store
 layouts (`:142-165,285-297`), duplicated label defaults (`:419,445`), and
 `TransferProvider` as a second provider enum (`:33`). Security-sensitive: needs
 its own test proof. surface −3, lines −150, risk med.

Facts: 249 `LoginProviderTarget::` occurrences in 17 production files; 218
provider-literal match arms repo-wide; 77 `LoginProviderAuthStateKey::` arms;
the auth-kind model is written 8+ times (`metadata/lib.rs:1-22`,
`provider-core/auth_mode.rs:37-85`, `protocol/lib.rs:121-141`,
`auth/types.rs:28-39` vs `protocol/lib.rs:121-127` same name twice,
`auth/env_facts.rs:146-158`, `transfer.rs:33-36`, `external.rs:163-172`, and
more), with exactly two behavioral uses (`doctor.rs:214-216`,
`bus_events.rs:155-166`); everything else is display.

### D. TUI

- [ ] **Delete the login overlay - delete**: `tui/login_picker.rs` is 1,046
 lines (881 non-test) that nothing outside its own `#[cfg(test)]` module ever
 constructs; it also carries 2 hand-written color tables and 10 plumbing sites.
 surface −1 screen, lines −1,046, risk low.
- [ ] **One account-list builder - condense**: three near-identical builders
 (`auth_account_picker.rs:469,768,924`) and 15 thirteen-field `PickerEntry`
 literals collapse to one function; the `entry()` helper already exists
 (`auth_remote/picker.rs:71`). surface −2 builders, lines −350 to −420, risk low.
- [ ] **Declaration-drive provider identity - re-core**: `WidgetProviderKind`
 (44 `tui_state.rs` sites), the string-keyed `provider_style`/`auth_kind_color`
 (`login_picker.rs:620,642`, `overlay_render.rs:239`), five hand lists
 (`auth_remote.rs:12`, `ui_header.rs:353`, `auth_account_commands.rs:610`,
 `auth.rs:219`, `inline_interactive.rs:489`) and four per-provider account
 dispatch copies become one descriptor lookup plus flags. surface −5,
 lines −600 to −800, risk med.
- [ ] **One OAuth login runner - condense**: PKCE + listener + 300s wait + QR +
 preflight + manual fallback duplicated at `auth.rs:412-470,553-570,698-780,826-844,877-1040,1410-1530`,
 with two inline PKCE copies (412-431, 703-722) while `generate_pkce_public`
 exists at `base/auth/oauth.rs:122`. surface −4, lines −300 to −380, risk med.
- [ ] **Merge the model-name prettifiers - unify**: `ui_header.rs:68-269` +
 `ui_status.rs:40-60` into `helpers/model_names.rs`. surface −2,
 lines −200 to −240, risk med (header copy is user-visible).
- [ ] **Share overlay chrome - unify**: `hotkey`, `metric_span`,
 `truncate_with_ellipsis`, `centered_rect`, `mask_email`, `provider_style`; 3-4
 copies each, two byte-identical. surface −5, lines −110, risk low.
- [ ] **Declaration-driven logout - rearrange**: `start_logout_provider`/
 `start_logout_all` carry 10 arms and 5 hardcoded env pairs with silent
 omissions. surface −1 list, lines −140, risk med.
- [ ] **Billing by declaration - rearrange**: `misc_ui.rs:83-89,193-218,354-372`
 derives `is_anthropic`/`is_openai`/billed twice independently
 (`update_cost_impl:169`, `resolve_remote_cost_pricing:348`) from name
 substrings plus a hardcoded billed list. One `billed_per_token` flag.
 surface −2, lines −40, risk med (money-adjacent; needs the tests that name it).
- [ ] **SSH login lists from the declaration - rearrange**: `PROVIDERS`
 (`auth_remote.rs:12`), the `"1".."6"` mask (`:311`), the import pair, the dead
 `"google"` (`picker.rs:8`) and the stale message (`:99`). surface −2,
 lines −25, risk low.
- [ ] **Compat settings as a row editor - condense**: 4 `AccountCommand`
 variants plus `save_openai_compat_setting` (`:749-857`) become one field editor
 over the four `KCODE_OPENAI_COMPAT_*` names. surface −3, lines −90, risk low.
- [ ] **`LoginCompleted` carries the descriptor id - condense**: 29 payload
 literals in `auth.rs` plus loose re-resolution (`bus_events.rs:134,154`,
 `metadata/lib.rs:239-255`). surface −1 fuzzy map, lines −40, risk med
 (protocol-adjacent, older hosts).
- [ ] **Small cleanups - delete**: two test-only helpers in `auth.rs` (−58),
 stale literals (`auth_remote.rs:99,311`, `picker.rs:8`, `login_picker.rs:624,636`),
 `RECOMMENDED_MODELS` read from core (`inline_interactive.rs:459`). lines −80,
 risk low.

Facts: 14,384 non-test in scope; ~240 identity/route branch sites plus 33
provider-id payload literals across 24 files; nine hand-maintained provider
lists. Recommended reduction if all land: ~2,800 lines.

### E. CLI, config, docs

- [ ] **Collapse per-target dispatch into driver kinds - re-core**: removes 4
 Rust tables across `login.rs:286-335`, `scriptable.rs:14-18,229-259`,
 `provider_init.rs:882-911,928-985,1078-1222`. surface −4 tables, lines −420,
 risk med (credential policy is sensitive).
- [ ] **Delete per-provider credential policy - condense**:
 `provider_init.rs:327-867` (541 lines, 7 `ensure_*`/`maybe_enable_*` pairs)
 hand-codes precedence the declaration row can carry. surface −7 pairs,
 lines −400, risk med.
- [ ] **Stop marshalling config through env - rearrange**:
 `provider_catalog.rs:722-809,839-1101` (~350 lines) exists to pass a
 `NamedProviderConfig` already in-process, through 27 process env vars a runtime
 re-reads (`openrouter-runtime/src/lib.rs:129-287`). surface −27 env vars,
 lines −300, risk med.
- [ ] **One identity table - unify**: merge `LoginProviderTarget::key`
 (`metadata/lib.rs:50-67`), `normalized_login_provider_id`
 (`lifecycle.rs:885-903`), `AuthRoute::cli_provider_arg` (`auth_mode.rs:243-250`),
 `cli_provider_arg_for_session_key` (`selection.rs:138-163`),
 `session_provider_key_from_provider_name` (`selection.rs:309-327`) into the
 declaration. Fixes the `google`/azure/`claude-api` drift at the source.
 surface −4 tables, lines −150, risk med.
- [ ] **Generate the auth-test and doctor rosters from flags - condense**:
 `auth_test/choice.rs:49-96,137-145`, `probes.rs:8-41`, `types.rs:15-80`,
 `doctor.rs:18-33`, `provider_e2e.rs:1308-1400`. surface −3 rosters,
 lines −250, risk low-med.
- [ ] **`provider add` with serde - condense**: `provider_setup.rs:406-576`
 (170 lines) hand-writes TOML emitting 9 of 19 fields; `config_file.rs:507-534`
 proves the round trip. surface −1 writer, −4 helpers, lines −150, risk low.
- [ ] **Dead and undocumented spellings - delete**: `claude-subprocess` (7
 sites: `provider_init.rs:44,54,78-80,1061-1077`, `auth_test/types.rs:55`,
 `provider_setup.rs:293`), `google` (`scriptable.rs:261`,
 `selection.rs:156`), the unreachable `jcode` doctor arm (`doctor.rs:29`), four
 redundant reserved names (`provider_setup.rs:291-298`), `KCODE_USE_CLAUDE_CLI`
 (`provider/startup.rs:130-143`). surface −12 spellings, lines −40, risk low.
- [ ] **Docs - rearrange**: `providers.md:24` promises an auth kind the text
 output never prints (`report_info.rs:376-388`); `providers.md:52` lists `azure`
 under API-key providers while the file is `azure-openai.env`; `[providers.<name>]`'s
 19 TOML keys are documented nowhere in `docs/user`; `auth.md:30` states the
 `<provider>.env` convention its three exceptions break. ~66 enumerated lines to
 either generate from the registry or correct. lines −60, risk none.

Facts: 13 Rust + 4 doc tables; 52 descriptors, 42 profiles, 84 aliases, ~136
spellings, 51 `-p` providers; `provider list` (51 + `auto`), clap (53 values) and
the login menu (52 incl. `auto-import`) are three different sets; a slug passes 3
registry lookups + 2 profile resolutions, then a 27-var env handoff; ~7,140
non-test CLI lines. Net available: −1,500 to −1,800 lines, −16 tables, −30+
spellings, no provider lost.

### F. Consumers and tendrils

- [ ] **One session route id - re-core**: the session stores the same route
 twice (`session.rs:136-146`), with 15 `provider_key` writers and 10
 `route_api_method` writers, and three tables re-derive it
 (`selection.rs:224-490`, `session.rs:293-336`, `client_state.rs:53-88`).
 surface −1 stored fact (twice-stored), −3 fns, lines −350, risk med (session
 JSON on disk).
- [ ] **Delete the `RuntimeKey`/`ModelRouteApiMethod`/`api_method`/`stable_id`
 quartet - delete**: 2 enums (15 + 15 variants), 2 spelling fns, 2 parse fns;
 539 `api_method` refs in 60 files. surface −2 enums, −2 tables,
 lines −260 core + −120 consumers, risk med (wire + sqlite).
- [ ] **Collapse the nine label↔key↔display tables - unify**:
 `client_state.rs:53-88`, `provider_activity.rs:310-391`,
 `external_auth.rs:112-172`, `models.rs:135-146`, `lifecycle.rs:743-880,1133-1160`,
 `debug_command_exec.rs:549-575`, `session.rs:293-336`, `selection.rs:309-345`.
 surface −9 tables, lines −500, risk med.
- [ ] **One credential value type - unify**: `ActiveCredential`
 (`active_method.rs:21-26`) + `AuthMode` (`auth_mode.rs:79-86`) +
 `ResolvedCredential` (`provider-core/lib.rs:1195-1201`). The decision is
 already unified (`active_method.rs:70-110` delegates to `pinned_mode_for`); the
 value type is not. surface −2 enums, lines −60, risk low.
- [ ] **One tool-alias table - unify**: four tables plus one shim, 151 lines;
 three live divergences (`file_grep` -> `kgrep` in `tool-core/lib.rs:389` vs
 `grep` in `tui-tool-display/lib.rs:14`; `Skill` -> `skill_manage` in
 tool-core/provider-core vs `skill` in `claude-cli-runtime:1127`; `Task` vs
 `Agent`), and only tool-core strips `functions.`. surface −3 tables,
 lines −110, risk low (one behavior change).
- [ ] **One retry classifier - unify**: 21 of `network_retry.rs:29-70`'s 28
 markers are verbatim from `transport.rs:33-95`; each side has faults the other
 lacks (8 vs 21), so the disagreement is live in both directions.
 surface −1 marker list, lines −60, risk low-med.
- [ ] **Move `live_tests` out of `kcode-base` - rearrange**: `pub mod live_tests`
 (`kcode-base/src/lib.rs:36`) ships a 3,080-line roster in the base lib; 46
 production cross-crate refs (39 in `live_provider_probes.rs`, 7 in
 `provider_e2e.rs`) plus `src/cli/auth_test/run.rs` (28). surface −1 module in
 base, lines −3,080 relocated, risk med.
- [ ] **One credential identity on the wire - unify**:
 `AuthProviderId`/`RuntimeProviderKey`/`CatalogNamespace`
 (`protocol/lib.rs:79,93,107`) dissolve to `String` one hop later
 (`lifecycle.rs:44-50`); `AuthChanged` carries the label/runtime/namespace
 triple (`:144-154`). surface −3 newtypes, −2 string fields, lines −120, risk
 med (wire compat).
- [ ] **`dialect` on the row - rearrange**: 13 hardcoded `registry::CONST` call
 sites in 7 crates; `by_id` has one caller, a test
 (`registry.rs:249`, `tests/recovery_coverage.rs:64`); antigravity picks its
 dialect from a model string. surface −13 call sites, lines −50, risk low.
- [ ] **`ProviderAvailability` bools to rows - rearrange**: `selection.rs:17-57`,
 18 refs in 7 files, an 8-arm match, and a 9th special field
 (`copilot_premium_zero`). surface −1 struct, lines −60, risk low-med.
- [ ] **Retire the parallel enums - delete**: `LoginProviderAuthStateKey`,
 `MultiAccountProviderKind`, `NativeProviderKind`, `NamedProviderType`,
 `AuthCredentialSource`/`AuthMethod` (6 enums each restating the id set).
 surface −6, lines −140, risk low.
- [ ] **`ProviderUsage.provider_name` - rearrange**: display string used as
 identity in the usage ledger and its cache keys (`kcode-usage-types/lib.rs:3`).
 surface −1 identity-by-label, lines −40, risk low-med.
- [ ] **`external_auth`'s three tables to descriptor rows - unify**:
 `external_auth.rs:112-127,133-143,161-172,565-575` re-key the same 6 ids the
 descriptor already carries. surface −3, lines −61, risk low.

Persisted identity to watch: session JSON (tolerant but silently falls back),
`model-usage-v1.sqlite3` PK `(turn_id, model, provider, api_method)` (a rename
orphans rows), `provider_activity.json` (substring-matched source keys),
`config.toml`, `schema-quirks.json` (keyed by dialect id), wire DTOs, and
`EnvSnapshot.provider` = `provider.name()` (`session-types/lib.rs:182`).

## Straight deletes

Safe tier, in the order to land them: `tui/login_picker.rs` (1,046);
`auth/login_flows.rs` (49); `transfer::available_local` (7); the Gemini CLI
resolver cluster (~66); `model_cache.rs:47-72` (~25); `provider/claude.rs` (8);
the empty `auth/tests.rs` (1); `openai_schema.rs` after the dialect port (755);
`test_sandbox.rs` out of the production crate (157); the constant-true
`openai_compatible_profile_model_supports_chat`; test scaffolds
(`models.rs:592-646`, `provider_catalog.rs:665-674`, ~60); dead pricing fns
(~132); 11 dead core `pub` items; the `claude-subprocess` sites (7);
`-p google` (2); the `jcode` doctor arm; four reserved names; three stale TUI
literals. About 2,500 lines, none of it a capability.

## Desktop-only cuts (not this lane)

There is no desktop app and no desktop dependency in the tree (no tauri, wry,
egui or electron anywhere), so anything that exists only to serve one is dead.
Both real clients already declare `supports_pdf_panels: false`
(`kcode-tui/src/tui/backend.rs:274`, `src/cli/acp.rs:768`) and only tests set it
true, so the PDF-panel branch is unreachable: `supports_pdf_panels` plumbing (45
sites across `server/client_state.rs:153-747`, `client_writer.rs:11-45`,
`client_session.rs:978-1438`, `client_lifecycle.rs:544-1574`),
`protocol/side_panel.rs:74,98` (`pdf_data`), the
"Open this panel in the desktop app" fallback (`side_panel.rs:539`) and the
`kcode-pdf`-backed panel projection. `kcode-pdf` text extraction itself is
model-facing (`tool/read.rs:178-552`) and stays. Route the panel/PDF cuts to
`plans/app-shape.md` or `plans/server-shape.md`; this lane keeps only
`image_clamp.rs`. `power_inhibit.rs` (375) and `turn_notify.rs` serve the
terminal session, not a desktop app, and stay.

## Decisions this lane needs

- [ ] **(decision) claude-cli**: delete (1,137 lines, zero tests, self-declared
 deprecated at `src/lib.rs:682-684`) or keep as a tail module? Delete is the
 cheapest single cut and loses only a deprecated capability.
- [ ] **(decision) cursor, grok-build, bedrock**: keep behind the boundary (one
 module each, none in the core set) or drop? Recommendation: keep; the
 malleability gate is what keeps them cheap, not deletion.
- [ ] **(decision) google and antigravity**: one codec with a dialect field
 (recommended) or two.
- [ ] **(decision) The boundary**: this lane covers the provider tier, auth, and
 the provider surfaces of the CLI/TUI. `kcode-base/src/auth` is audited here but
 lands on its own schedule; the TUI items belong to `plans/app-shape.md` and the
 CLI items to `plans/hygiene.md` unless this lane takes them.

## Gate and landing order

Each item lands separately, proven by the suites that name it
(`scripts/test.sh crate kcode-base provider`, then the `-runtime` suites for the
wire items, `auth_remote` for auth, the TUI suite for TUI). Re-measure at each
step: `find crates/kcode-provider* -name '*.rs' | xargs wc -l`, split non-test vs
`#[cfg(test)]`, plus `crates/kcode-base/src/provider`, `provider_catalog.rs`,
`crates/kcode-base/src/auth`, and the TUI/CLI files named above. Suggested
order: straight deletes; the spine (one id type); the dispatch fan-out (unblocks
the trait split and the declaration row); the declaration table (metadata +
model data); auth kinds; TUI and CLI views; tail transports; then the crate
merge.

Acceptance for the whole lane is the malleability gate: the next provider shape
arrives as one declaration row or one codec module, with no `kcode-base` edit,
no new crate, and no edit to picker, login, docs or catalog.

## Where the evidence is thin (read before cutting)

The eight sweeps read at `file:line` and marked what they did not read. Any item
resting on one of these regions needs its own read first, and none of the line
estimates above are measured diffs:

- Runtime crates (hibiscus): full bodies of `openai_stream_runtime.rs` ws
 sections, `openai_websocket_prewarm.rs`, `kcode-provider-openai/src/stream.rs`,
 `openrouter/src/stream.rs`, `anthropic/src/lib.rs`, grok's `lib.rs` mid-section,
 cursor's `agent_transport.rs` mid-section, claude-cli's mid-section, and all
 test files except partial reads. The retry/copy counts are grep plus spot
 reads, not full-file diffs.
- Base/provider (sunflower): `provider/tests/*`, `tests.rs` bodies,
 `provider_catalog_tests.rs`, `pricing.rs:300-507`, `selection.rs` tests,
 `image_clamp.rs` internals beyond its head, `stream_timeout.rs` bodies.
- Core (blossom): `kcode-provider-doctor` was only grepped; `provider/mod.rs`
 beyond grep; `bedrock` beyond the cited lines.
- Auth (tulip): `live_provider_probes.rs`, `lifecycle_driver_tests.rs`,
 `live_tests.rs`, all `*_tests.rs` bodies, and the TUI `handle_login_input`
 bodies were structural reads only.
- TUI (rose): `auth.rs` ranges 1600-1780, 1960-2245, 2500-2690;
 `inline_interactive.rs` ~60% read; `model_context.rs` ~25%; `info_widget_model.rs`
 ~10%; `ui_input.rs`/`ui_header.rs`/`tui_state.rs` partial; `auth_remote/*`
 bodies partial.
- CLI (maple): full command bodies only for the files named; the
 `KCODE_OPENAI_COMPAT_*` profile semantics were traced through the env bridge,
 not by running it.

Two audit claims were withdrawn after a second read and are deliberately absent:
`fallback_pick` and the `AVAILABLE_MODELS` lists are live, not dead.
