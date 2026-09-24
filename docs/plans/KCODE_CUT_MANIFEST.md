# kcode cut manifest

This is the record of what the `rewrite/cut-first` branch deleted, kept as
text so the deletions survive as knowledge after the history is squashed.

- Base commit (branch point): `688e67df3a032f6e4b1e074d9b83e985ffef850c`
- Head at capture: `a41b1a0f8`
- Cut commits: 44
- Deleted paths: 650
- Net diff vs base: `1079 files changed, 2457 insertions(+), 241527 deletions(-)`

## Deleted by area

| area | paths |
|---|---|
| `crates` | 286 |
| `scripts` | 99 |
| `ios` | 76 |
| `telemetry-worker` | 53 |
| `sdk` | 52 |
| `docs` | 39 |
| `assets` | 25 |
| `src` | 10 |
| `tests` | 4 |
| `.github` | 4 |
| `TELEMETRY.md` | 1 |
| `graphify-out` | 1 |

## Cut commits

```
 b12a275ff chore(cut): delete 19 source files rustc never compiles
 e8a54ea8f chore(cut): remove the unwired goal tool and a dead hotkey helper
 d178bc02b chore(cut): delete the unreachable voice module and two never-enabled features
 682242e2e chore(cut): delete the SDKs, the iOS app, the phone relay and the WebSocket gateway
 ab8a7b8c2 chore(cut): remove the Telegram and Discord integrations
 053bb138b chore(cut): remove the jcode cloud command family
 7fdf78300 chore(cut): remove the notification integrations (ntfy, desktop, email/IMAP)
 7c24f01cc chore(cut): remove product telemetry, feedback, and the consent surfaces
 d89be989d chore(cut): remove sponsored discovery and the dev bench binaries
 28679d34e chore(cut): fold jcode-ambient-types and jcode-batch-types into their consumers
 540b2e4c6 chore(cut): fold four type shims into their consumer layers
 1661dafd2 chore(cut): collapse the one duplicate wire type shape and drop provably dead items
 6dc89abd0 chore(cut): delete 30 unreferenced public items (finding 7, groups 1-2)
 fb66f2551 chore(cut): delete auth/provider unreferenced helpers (finding 7, batch 2)
 d4bc84811 chore(cut): add storage read_json_or_default helpers and collapse sidecar loads
 f11b0ae06 chore(cut): remove the jemalloc allocator support entirely (finding 15)
 f44262042 chore(cut): remove unreferenced media and dead scripts (item 3)
 39d2cf4e2 chore(cut): remove the harness API surface and its bridge (item 4a step 1)
 999709620 docs(cut): retire docs for cut surfaces (SDK, iOS, cloud, harness API, telemetry)
 e0836beb6 docs(cut): correct references to cut crates and features
 237381aca chore(cut): remove Windows support across source, scripts, CI, and deps
 2af241d75 chore(cut): orphaned jade helpers and post-cut dead code
 cff214e50 docs(test): drop setup-hints reference from the notification probe comment
 83e84c317 chore(cut): residual platform cfgs and stale docs (items 3 and 4)
 644ea64de docs: correct the residual stale-reference count and last dead paths
 376d30cdf chore: untrack derived graphify output and ignore it
 df19e8db7 chore(cut): remove the self-dev, install, update and launcher machinery
 06206c90c fix(cut): qualify relocated session-recovery calls and trim stale imports
 62f0f6f50 fix(cut): compile the branch green after the machinery removal
 db4948851 chore(cut): remove the client installer and its CI/docs surface
 1c3137152 docs(cut): retire the obsolete selfdev plan and fix drifted paths
 441f93a20 docs(cut): stop three reference docs instructing removed commands
 826ecea74 chore(ci): disable fork CI with one-trigger restore
 49d434cfe style(colors): drop the palette-harmony scorer and proximity attribution
 49a3616c1 chore(cut): remove the macOS menubar app
 f9ad1a6f4 chore(cut): remove the macOS computer-use tool
 563cc6d0f chore(cut): remove the productivity dashboard
 fd3cdd49e feat!: remove dictation
 0cbc95310 feat!: remove gmail and the Google/Gmail login
 5cb08e7a2 feat!: remove memory and ambient
 526775884 feat!: remove mermaid, diagrams, inline images, latex and replay
 1d6ebbdf0 feat!: rebrand jcode as kcode (phase 1: identity)
 af166fe27 docs: record kcode rename phase 1 as done
 a41b1a0f8 fix(tui): render the No pill on the onboarding OpenAI prompt
```

## Deleted paths

```
assets/app-icons/Jcode.icns
assets/demos/duck_fast-on-mid-stream_autoedit_timeline.json
assets/demos/duck_fast-on-mid-stream_autoedit_v2_timeline.json
assets/demos/duck_fast-on-mid-stream_autoedit_v2_trimmed_timeline.json
assets/demos/edited_timeline.json
assets/demos/exports/memory_demo_1m40_spedup.mp4
assets/demos/exports/memory_demo_1m40_spedup_v2.mp4
assets/demos/jcode-claudeai-demo.mp4
assets/demos/jcode_demo.mp4
assets/demos/jcode_mermaid_demo_final.mp4
assets/demos/jcode_mermaid_demo.mp4
assets/demos/jcode_mermaid_demo_v2.mp4
assets/demos/jcode_replay_duck_fast-on-mid-stream_autoedit_2x.mp4
assets/demos/jcode_replay_duck_fast-on-mid-stream_autoedit_trimmed_2x.mp4
assets/demos/jcode-vs-claude-code.png
assets/demos/jcode_wolf_demo_final.mp4
assets/demos/jcode_wolf_demo_v2.mp4
assets/demos/memory_demo.mp4
assets/demos/pelican-bike/index.html
assets/demos/pelican-bike/pelican.js
assets/demos/pelican-bike/styles.css
assets/demos/wolf_timeline.json
assets/demos/workflow.mp4
assets/niri-screenshot.png
assets/readme/100-sessions-spawn-demo.gif
crates/jcode-ambient-types/Cargo.toml
crates/jcode-app-core/src/agent_tests/concurrency_construction.rs
crates/jcode-app-core/src/agent_tests/concurrency.rs
crates/jcode-app-core/src/agent_tests/desktop_selfdev.rs
crates/jcode-app-core/src/ambient/directives.rs
crates/jcode-app-core/src/ambient/manager.rs
crates/jcode-app-core/src/ambient/paths.rs
crates/jcode-app-core/src/ambient/persistence.rs
crates/jcode-app-core/src/ambient/prompt.rs
crates/jcode-app-core/src/ambient.rs
crates/jcode-app-core/src/ambient/runner_live_delivery_tests.rs
crates/jcode-app-core/src/ambient/runner.rs
crates/jcode-app-core/src/ambient_runner.rs
crates/jcode-app-core/src/ambient/runner_tests.rs
crates/jcode-app-core/src/ambient/scheduler.rs
crates/jcode-app-core/src/ambient_scheduler.rs
crates/jcode-app-core/src/ambient_tests.rs
crates/jcode-app-core/src/ambient/types.rs
crates/jcode-app-core/src/channel.rs
crates/jcode-app-core/src/message_notifications.rs
crates/jcode-app-core/src/notifications.rs
crates/jcode-app-core/src/protocol_memory.rs
crates/jcode-app-core/src/protocol_tests/comm_requests.rs
crates/jcode-app-core/src/protocol_tests/comm_responses.rs
crates/jcode-app-core/src/protocol_tests/core_events.rs
crates/jcode-app-core/src/protocol_tests/misc_events.rs
crates/jcode-app-core/src/protocol_tests/randomized.rs
crates/jcode-app-core/src/protocol_tests.rs
crates/jcode-app-core/src/replay.rs
crates/jcode-app-core/src/replay/tests.rs
crates/jcode-app-core/src/server/client_session_tests/concurrency.rs
crates/jcode-app-core/src/server/debug_ambient.rs
crates/jcode-app-core/src/server/jade_relay.rs
crates/jcode-app-core/src/session_active_pids.rs
crates/jcode-app-core/src/session_rebuild.rs
crates/jcode-app-core/src/setup_hints.rs
crates/jcode-app-core/src/stdin_detect_tests.rs
crates/jcode-app-core/src/telemetry_state.rs
crates/jcode-app-core/src/telemetry_tests.rs
crates/jcode-app-core/src/tool/ambient.rs
crates/jcode-app-core/src/tool/ambient/tests.rs
crates/jcode-app-core/src/tool/computer/ax.rs
crates/jcode-app-core/src/tool/computer/coverage_tests.rs
crates/jcode-app-core/src/tool/computer/discover.rs
crates/jcode-app-core/src/tool/computer/input.rs
crates/jcode-app-core/src/tool/computer/keys.rs
crates/jcode-app-core/src/tool/computer/mod.rs
crates/jcode-app-core/src/tool/computer/osa.rs
crates/jcode-app-core/src/tool/computer/screen.rs
crates/jcode-app-core/src/tool/computer/setup.rs
crates/jcode-app-core/src/tool/computer/sys.rs
crates/jcode-app-core/src/tool/computer/tests.rs
crates/jcode-app-core/src/tool/computer/win.rs
crates/jcode-app-core/src/tool/desktop_selfdev.rs
crates/jcode-app-core/src/tool/desktop_selfdev_tests.rs
crates/jcode-app-core/src/tool/discover.rs
crates/jcode-app-core/src/tool/discover_secrets.rs
crates/jcode-app-core/src/tool/feedback.rs
crates/jcode-app-core/src/tool/gmail.rs
crates/jcode-app-core/src/tool/goal.rs
crates/jcode-app-core/src/tool/goal_tests.rs
crates/jcode-app-core/src/tool/memory.rs
crates/jcode-app-core/src/tool/selfdev/build_queue.rs
crates/jcode-app-core/src/tool/selfdev/launch.rs
crates/jcode-app-core/src/tool/selfdev/mod.rs
crates/jcode-app-core/src/tool/selfdev/reload.rs
crates/jcode-app-core/src/tool/selfdev/setup.rs
crates/jcode-app-core/src/tool/selfdev/status.rs
crates/jcode-app-core/src/tool/selfdev/tests.rs
crates/jcode-app-core/src/tool/serde_coerce.rs
crates/jcode-app-core/src/update_dev_guard.rs
crates/jcode-app-core/src/update_metadata.rs
crates/jcode-app-core/src/update_rate_limit.rs
crates/jcode-app-core/src/update.rs
crates/jcode-app-core/src/usage_display.rs
crates/jcode-app-core/src/usage_openai.rs
crates/jcode-app-core/src/usage_tests.rs
crates/jcode-auth-types/Cargo.toml
crates/jcode-azure-auth/Cargo.toml
crates/jcode-azure-auth/src/lib.rs
crates/jcode-base/src/auth/google.rs
crates/jcode-base/src/dictation.rs
crates/jcode-base/src/dictation_tests.rs
crates/jcode-base/src/embedding_backend.rs
crates/jcode-base/src/embedding.rs
crates/jcode-base/src/embedding_stub.rs
crates/jcode-base/src/gateway/auth.rs
crates/jcode-base/src/gateway/control.rs
crates/jcode-base/src/gateway/registry.rs
crates/jcode-base/src/gateway.rs
crates/jcode-base/src/gateway_tests.rs
crates/jcode-base/src/gmail.rs
crates/jcode-base/src/memory/activity.rs
crates/jcode-base/src/memory_agent.rs
crates/jcode-base/src/memory_agent_tests.rs
crates/jcode-base/src/memory/cache.rs
crates/jcode-base/src/memory_graph.rs
crates/jcode-base/src/memory_judge_metrics.rs
crates/jcode-base/src/memory_log.rs
crates/jcode-base/src/memory/pending.rs
crates/jcode-base/src/memory_prompt.rs
crates/jcode-base/src/memory_rerank.rs
crates/jcode-base/src/memory.rs
crates/jcode-base/src/memory_tests.rs
crates/jcode-base/src/memory_types.rs
crates/jcode-base/src/prompt/desktop_selfdev_mode.txt
crates/jcode-base/src/protocol/notifications.rs
crates/jcode-base/src/sidecar.rs
crates/jcode-base/src/sponsors/provenance.rs
crates/jcode-base/src/sponsors.rs
crates/jcode-base/src/telegram.rs
crates/jcode-base/src/voice.rs
crates/jcode-base/tests/launch_hotkeys_roundtrip.rs
crates/jcode-batch-types/Cargo.toml
crates/jcode-batch-types/src/lib.rs
crates/jcode-build-support/Cargo.toml
crates/jcode-build-support/examples/write_dev_sidecar.rs
crates/jcode-build-support/src/lib.rs
crates/jcode-build-support/src/paths.rs
crates/jcode-build-support/src/platform_support.rs
crates/jcode-build-support/src/source_state.rs
crates/jcode-build-support/src/storage_helpers.rs
crates/jcode-build-support/src/tests.rs
crates/jcode-embedding/Cargo.toml
crates/jcode-embedding/src/lib.rs
crates/jcode-embedding/tests/embed_latency_probe.rs
crates/jcode-gateway-types/Cargo.toml
crates/jcode-gateway-types/src/lib.rs
crates/jcode-harness-api/Cargo.toml
crates/jcode-harness-api/examples/harness_repl.rs
crates/jcode-harness-api-server/Cargo.toml
crates/jcode-harness-api-server/src/background_progress.rs
crates/jcode-harness-api-server/src/background_progress_tests.rs
crates/jcode-harness-api-server/src/bin/bridge.rs
crates/jcode-harness-api-server/src/framing_tests.rs
crates/jcode-harness-api-server/src/lib.rs
crates/jcode-harness-api-server/src/stdio_tests.rs
crates/jcode-harness-api-server/src/translate.rs
crates/jcode-harness-api-server/src/translate_tests.rs
crates/jcode-harness-api/src/client.rs
crates/jcode-harness-api/src/events.rs
crates/jcode-harness-api/src/harness_api_tests/capability_coverage.rs
crates/jcode-harness-api/src/harness_api_tests/schema_snapshot.rs
crates/jcode-harness-api/src/harness_api_tests/swarm_metadata.rs
crates/jcode-harness-api/src/lib.rs
crates/jcode-harness-api/src/requests.rs
crates/jcode-harness-api/src/sockets.rs
crates/jcode-harness-api/src/swarm_metadata.rs
crates/jcode-memory-types/Cargo.toml
crates/jcode-memory-types/src/graph/graph_tests.rs
crates/jcode-memory-types/src/graph.rs
crates/jcode-memory-types/src/lib.rs
crates/jcode-notify-email/Cargo.toml
crates/jcode-notify-email/src/lib.rs
crates/jcode-productivity-core/Cargo.toml
crates/jcode-productivity-core/examples/run.rs
crates/jcode-productivity-core/src/aggregate.rs
crates/jcode-productivity-core/src/dashboard.rs
crates/jcode-productivity-core/src/lib.rs
crates/jcode-productivity-core/src/markdown.rs
crates/jcode-productivity-core/src/model.rs
crates/jcode-productivity-core/src/scan.rs
crates/jcode-productivity-core/src/tests.rs
crates/jcode-sdk/Cargo.toml
crates/jcode-sdk/src/auth/callback.rs
crates/jcode-sdk/src/auth.rs
crates/jcode-sdk/src/auth/tests.rs
crates/jcode-sdk/src/client.rs
crates/jcode-sdk/src/diagnostics.rs
crates/jcode-sdk/src/errors.rs
crates/jcode-sdk/src/launch.rs
crates/jcode-sdk/src/lib.rs
crates/jcode-sdk/src/sdk_tests/parity.rs
crates/jcode-sdk/src/shared_ssh_tests.rs
crates/jcode-sdk/src/ssh_integration_tests.rs
crates/jcode-sdk/src/ssh.rs
crates/jcode-sdk/src/structured.rs
crates/jcode-sdk/src/worktrees.rs
crates/jcode-sdk/tests/client_behavior.rs
crates/jcode-sdk/tests/edit_stats.rs
crates/jcode-sdk/tests/lifecycle_events.rs
crates/jcode-sdk/tests/structured_output.rs
crates/jcode-selfdev-types/Cargo.toml
crates/jcode-selfdev-types/src/desktop.rs
crates/jcode-selfdev-types/src/lib.rs
crates/jcode-setup-hints/Cargo.toml
crates/jcode-setup-hints/src/cli_launch_hints.rs
crates/jcode-setup-hints/src/keymap/chord.rs
crates/jcode-setup-hints/src/keymap/conflicts.rs
crates/jcode-setup-hints/src/keymap/external.rs
crates/jcode-setup-hints/src/keymap/macos_hotkeys.rs
crates/jcode-setup-hints/src/keymap/mod.rs
crates/jcode-setup-hints/src/keymap/report.rs
crates/jcode-setup-hints/src/keymap/source.rs
crates/jcode-setup-hints/src/keymap/terminal.rs
crates/jcode-setup-hints/src/launch_hotkeys.rs
crates/jcode-setup-hints/src/lib.rs
crates/jcode-setup-hints/src/linux_env.rs
crates/jcode-setup-hints/src/linux_niri_fuzz_corpus.txt
crates/jcode-setup-hints/src/linux_niri.rs
crates/jcode-setup-hints/src/linux_niri_shortcut_tests.rs
crates/jcode-setup-hints/src/macos_launcher.rs
crates/jcode-setup-hints/src/macos_launcher_tests.rs
crates/jcode-setup-hints/src/macos_terminal.rs
crates/jcode-setup-hints/src/setup_hints_tests.rs
crates/jcode-setup-hints/src/windows_hotkeys.rs
crates/jcode-setup-hints/src/windows_setup.rs
crates/jcode-side-panel-types/Cargo.toml
crates/jcode-telemetry-core/Cargo.toml
crates/jcode-telemetry-core/CONCURRENCY.md
crates/jcode-telemetry-core/examples/concurrency_probe.rs
crates/jcode-telemetry-core/src/concurrency.rs
crates/jcode-telemetry-core/src/concurrency/tests.rs
crates/jcode-telemetry-core/src/lib.rs
crates/jcode-telemetry-core/src/lifecycle.rs
crates/jcode-telemetry-core/src/onboarding_trace.rs
crates/jcode-telemetry-core/src/state_support.rs
crates/jcode-telemetry-core/src/tests.rs
crates/jcode-telemetry-core/tests/session_creation_latency.rs
crates/jcode-terminal-launch/src/windows_portable_tests.rs
crates/jcode-tool-types/Cargo.toml
crates/jcode-tool-types/src/lib.rs
crates/jcode-transport/src/windows.rs
crates/jcode-tui-core/src/graph_topology.rs
crates/jcode-tui-markdown/src/markdown_latex_image.rs
crates/jcode-tui-markdown/src/markdown_mermaid_fallback.rs
crates/jcode-tui-markdown/src/markdown_tests/cases/latex_streaming.rs
crates/jcode-tui-markdown/src/markdown_tests/cases/placeholders.rs
crates/jcode-tui-markdown/tests/latex_deferred_resolution.rs
crates/jcode-tui-markdown/tests/latex_draw_path_latency.rs
crates/jcode-tui-mermaid/build.rs
crates/jcode-tui-mermaid/Cargo.toml
crates/jcode-tui-mermaid/examples/plan_graph_probe.rs
crates/jcode-tui-mermaid/examples/swarm_plan_fixture.json
crates/jcode-tui-mermaid/examples/swarm_plan_stress.rs
crates/jcode-tui-mermaid/src/debug.rs
crates/jcode-tui-mermaid/src/lib.rs
crates/jcode-tui-mermaid/src/mermaid_active.rs
crates/jcode-tui-mermaid/src/mermaid_cache_render.rs
crates/jcode-tui-mermaid/src/mermaid_content.rs
crates/jcode-tui-mermaid/src/mermaid_debug.rs
crates/jcode-tui-mermaid/src/mermaid_inline.rs
crates/jcode-tui-mermaid/src/mermaid_model.rs
crates/jcode-tui-mermaid/src/mermaid_runtime.rs
crates/jcode-tui-mermaid/src/mermaid_svg.rs
crates/jcode-tui-mermaid/src/mermaid_tests/part_01.rs
crates/jcode-tui-mermaid/src/mermaid_tests/part_02.rs
crates/jcode-tui-mermaid/src/mermaid_tests.rs
crates/jcode-tui-mermaid/src/mermaid_viewport.rs
crates/jcode-tui-mermaid/src/mermaid_widget.rs
crates/jcode-tui-mermaid/tests/layout_cache_cross_width_parity.rs
crates/jcode-tui-mermaid/tests/layout_cache_memory_probe.rs
crates/jcode-tui-mermaid/tests/layout_cache_pixel_parity.rs
crates/jcode-tui-mermaid/tests/layout_cache_resize_probe.rs
crates/jcode-tui-render/src/memory_tiles.rs
crates/jcode-tui/src/tui/app/commands_remote.rs
crates/jcode-tui/src/tui/app/dictation.rs
crates/jcode-tui/src/tui/app/productivity.rs
crates/jcode-tui/src/tui/app/replay.rs
crates/jcode-tui/src/tui/app/tests/swarm_plan_graph_inline.rs
crates/jcode-tui/src/tui/info_widget_graph.rs
crates/jcode-tui/src/tui/info_widget_memory_render.rs
crates/jcode-tui/src/tui/info_widget_memory_utils.rs
crates/jcode-tui/src/tui/info_widget_timeline.rs
crates/jcode-tui/src/tui/mermaid.rs
crates/jcode-tui/src/tui/swarm_plan_graph.rs
crates/jcode-tui/src/tui/ui_diagram_pane.rs
crates/jcode-tui/src/tui/ui_inline_image.rs
crates/jcode-tui/src/tui/ui_memory.rs
crates/jcode-tui/src/tui/ui_panel_image_preview.rs
crates/jcode-tui/src/tui/ui_pinned_layout.rs
crates/jcode-tui/src/tui/ui_pinned_mermaid_debug.rs
crates/jcode-tui/src/tui/ui_pinned_tests.rs
crates/jcode-tui/src/tui/ui_tests/basic/image_regions.rs
crates/jcode-tui/src/tui/ui_tests/diagrams/part_01.rs
crates/jcode-tui/src/tui/ui_tests/diagrams/part_02.rs
crates/jcode-tui/src/tui/ui_tests/diagrams.rs
crates/jcode-tui/src/tui/ui_tests/palette_topology.rs
crates/jcode-tui/src/video_export.rs
crates/jcode-tui-style/examples/light_bench.rs
crates/jcode-tui-style/src/harmony/generate.rs
crates/jcode-tui-style/src/harmony/graph.rs
crates/jcode-tui-style/src/harmony/measured.rs
crates/jcode-tui-style/src/harmony.rs
crates/jcode-update-core/Cargo.toml
crates/jcode-update-core/src/lib.rs
docs/AGENTCARD_DISCOVERY_DEMO.md
docs/AMBIENT_MODE.md
docs/ATTRIBUTION_BENCHMARK.md
docs/audits/CODE_QUALITY_AUDIT_2026-04-18.md
docs/CRATE_OWNERSHIP_BOUNDARIES.md
docs/DESKTOP_AUTH_SDK.md
docs/dev/ACCOUNT_FLOWS_OBSERVABILITY_PRIVACY.md
docs/dev/crate-splitting-plan.md
docs/discovery-baselines/claude-fable-5-after.json
docs/discovery-baselines/claude-fable-5-before.json
docs/discovery-baselines/flash-lite-before.json
docs/DISCOVERY_BENCHMARK.md
docs/DISCOVERY_CONVERSION_ANALYSIS.md
docs/DISCOVERY_ELICITATION_SPEC.md
docs/DISCOVERY_RATE_BENCHMARK.md
docs/GMAIL_COMPOSIO_BACKEND.md
docs/IOS_APP.md
docs/JCODE_CLOUD_AWS.md
docs/jcode_reddit_dashboard.png
docs/KEYMAP_CONFLICTS.md
docs/MEMORY_ARCHITECTURE.md
docs/MERMAID_RENDERING_REDESIGN.md
docs/MODULAR_ARCHITECTURE_RFC.md
docs/OPENRELAY_DISCOVERY_TEST.md
docs/plans/CLIENT_CORE_PRESENTATION_SPLIT_PLAN.md
docs/plans/CODE_QUALITY_10_10_PLAN.md
docs/plans/CODE_QUALITY_TODO.md
docs/plans/COMPILE_PERFORMANCE_PLAN.md
docs/plans/MEMORY_GRAPH_PLAN.md
docs/plans/SERVER_SERVICE_SPLIT_PLAN.md
docs/plans/UNIFIED_SELFDEV_SERVER_PLAN.md
docs/reddit_dashboard.py
docs/REFACTORING.md
docs/REMOTE_HANDOFF.md
docs/SESSION_CREATION_LATENCY.md
docs/SPONSORED_DISCOVERY_SPONSOR_ONBOARDING.md
docs/SPONSOR_IMPLEMENTATION.md
docs/sponsors/agentcard.md
docs/WINDOWS.md
.github/scripts/verify_windows_install.ps1
.github/workflows/ios-testflight.yml
.github/workflows/publish-typescript-sdk.yml
.github/workflows/windows-smoke.yml
graphify-out/cache/stat-index.json
ios/.gitignore
ios/Package.swift
ios/PRODUCTION_CHECKLIST.md
ios/project.yml
ios/Sources/JCodeKit/Connection.swift
ios/Sources/JCodeKit/CredentialStore.swift
ios/Sources/JCodeKit/Gateway.swift
ios/Sources/JCodeKit/Pairing.swift
ios/Sources/JCodeKit/SessionReducer.swift
ios/Sources/JCodeKit/Transport.swift
ios/Sources/JCodeKit/Wire.swift
ios/Sources/JCodeMobile/AppModel.swift
ios/Sources/JCodeMobile/Assets.xcassets/AppIcon.appiconset/AppIcon.png
ios/Sources/JCodeMobile/Assets.xcassets/AppIcon.appiconset/Contents.json
ios/Sources/JCodeMobile/Assets.xcassets/Contents.json
ios/Sources/JCodeMobile/Assets.xcassets/LaunchBackground.colorset/Contents.json
ios/Sources/JCodeMobile/Info.plist
ios/Sources/JCodeMobile/JCodeMobileApp.swift
ios/Sources/JCodeMobile/MarkdownText.swift
ios/Sources/JCodeMobile/PrivacyInfo.xcprivacy
ios/Sources/JCodeMobile/QRScannerView.swift
ios/Sources/JCodeMobile/Theme.swift
ios/Sources/JCodeMobile/Views/ChatView.swift
ios/Sources/JCodeMobile/Views/Composer.swift
ios/Sources/JCodeMobile/Views/ConnectionBanner.swift
ios/Sources/JCodeMobile/Views/EntryView.swift
ios/Sources/JCodeMobile/Views/PairingView.swift
ios/Sources/JCodeMobile/Views/RootView.swift
ios/Sources/JCodeMobile/Views/SettingsSections.swift
ios/Sources/JCodeMobile/Views/SettingsView.swift
ios/Sources/JCodeMobile/Views/ToolCallCard.swift
ios/Sources/JCodeMobile/Views/TranscriptView.swift
ios/TestHarness/check_production.sh
ios/TestHarness/mock_gateway.py
ios/TestHarness/protocol_smoke_test.py
ios/TestHarness/README.md
ios/TestHarness/reward/aggregate.py
ios/TestHarness/reward/AI_SLOP_RESEARCH.md
ios/TestHarness/reward/context.py
ios/TestHarness/reward/__init__.py
ios/TestHarness/reward/interaction/cost_model.py
ios/TestHarness/reward/interaction/engine.py
ios/TestHarness/reward/interaction/__init__.py
ios/TestHarness/reward/interaction/log_mining.py
ios/TestHarness/reward/interaction/LOG_SCHEMA.md
ios/TestHarness/reward/interaction/model.py
ios/TestHarness/reward/interaction/test_engine.py
ios/TestHarness/reward/interaction/ui_map.py
ios/TestHarness/reward/interaction/user_model.py
ios/TestHarness/reward/REWARD_SPEC.md
ios/TestHarness/reward/scorers/accessibility.py
ios/TestHarness/reward/scorers/ai_patterns.py
ios/TestHarness/reward/scorers/consistency.py
ios/TestHarness/reward/scorers/content_safety.py
ios/TestHarness/reward/scorers/contrast.py
ios/TestHarness/reward/scorers/information_density.py
ios/TestHarness/reward/scorers/__init__.py
ios/TestHarness/reward/scorers/interaction_cost.py
ios/TestHarness/reward/scorers/layout_robustness.py
ios/TestHarness/reward/scorers/perf.py
ios/TestHarness/reward/scorers/reachability.py
ios/TestHarness/reward/scorers/rhythm.py
ios/TestHarness/reward/scorers/simplicity.py
ios/TestHarness/reward/scorers/space_efficiency.py
ios/TestHarness/reward/scorers/styling.py
ios/TestHarness/reward/scorers/touch_targets.py
ios/TestHarness/reward/scorers/visual_hierarchy.py
ios/TestHarness/reward/test_determinism.py
ios/TestHarness/reward/types.py
ios/TestHarness/run_e2e.sh
ios/TestHarness/ui_lint.py
ios/TestHarness/ui_matrix.py
ios/TestHarness/ui_metrics.py
ios/Tests/JCodeKitTests/ConnectionTests.swift
ios/Tests/JCodeKitTests/SessionReducerTests.swift
ios/Tests/JCodeKitTests/WireTests.swift
scripts/analyze_openai_ws_cache.py
scripts/antigravity_multiturn_coverage.sh
scripts/antigravity_schema_probe.sh
scripts/attribution_benchmark_sponsors.json
scripts/audit_terminal_bench_submission.py
scripts/auth_regression_matrix.sh
scripts/auto_screenshot.sh
scripts/benchmark_attribution.py
scripts/benchmark_discovery.py
scripts/benchmark_discovery_rate.py
scripts/benchmark_swarm.py
scripts/benchmark_takehome.py
scripts/benchmark_tools.sh
scripts/bench_memory_cli.py
scripts/bench_selfdev_build.sh
scripts/bench_selfdev_checkpoints.sh
scripts/capture_demo.sh
scripts/capture_screenshot.sh
scripts/check_donut_animates_live.py
scripts/check_powershell_syntax.ps1
scripts/compare_discovery_rate.sh
scripts/demo_shop.py
scripts/diagnose_idle_render_cost.py
scripts/discovery_benchmark_cases.json
scripts/discovery_rate_cases.json
scripts/fuzz/niri_insert_point_fuzz.py
scripts/install.ps1
scripts/install_release.sh
scripts/install.sh
scripts/invoke_cargo_with_timeout.ps1
scripts/jcode_harbor_claude_agent.py
scripts/jcode_memory_snapshot.py
scripts/jcode_monitor.py
scripts/launch_agentcard_discovery_demo.sh
scripts/measure_animation_cpu_cost.py
scripts/mock_sponsor_service.py
scripts/oauth_helper.py
scripts/openrelay_discovery_test_server.py
scripts/phone-server/breaker-lambda.py
scripts/phone-server/IAM-LEAST-PRIVILEGE.md
scripts/phone-server/idle-autostop.sh
scripts/phone-server/jcode-pair-service.py
scripts/phone-server/README.md
scripts/phone-server/testflight-setup.py
scripts/phone-server/test_wake_lambda.py
scripts/phone-server/units/idle-autostop.service
scripts/phone-server/units/idle-autostop.timer
scripts/phone-server/units/jcode-pair.service
scripts/phone-server/units/jcode-serve.service
scripts/phone-server/wake-lambda.py
scripts/pin_no_area_state.py
scripts/prepare_sdk_runtime_packages.sh
scripts/probe_idle_state.py
scripts/profile_idle_donut.py
scripts/record_demo.sh
scripts/reload_recovery_audit.py
scripts/remote/gateway_client.py
scripts/remote/README.md
scripts/remote/remote_check.py
scripts/replay_recording.sh
scripts/repro_live_duplicate.py
scripts/repro_slash_flicker.py
scripts/repro_startup_lag.py
scripts/repro/tls-bad-record-mac/Cargo.toml
scripts/repro/tls-bad-record-mac/README.md
scripts/repro/tls-bad-record-mac/src/main.rs
scripts/run_openrelay_discovery_test.sh
scripts/run_tb21_publishable.sh
scripts/sdk_publish_preflight.sh
scripts/setup_friction_eval.sh
scripts/stale_server_upgrade_sandbox.sh
scripts/tb_baseline_cc_opus48.tsv
scripts/tb_compare.py
scripts/test_benchmark_attribution.py
scripts/test_benchmark_discovery.py
scripts/test_benchmark_discovery_rate.py
scripts/test_caching_detailed.py
scripts/test_demo_shop.py
scripts/test_desktop_selfdev.py
scripts/test_install_conversion.sh
scripts/test_install_release_metadata.sh
scripts/test_memory.py
scripts/test_mock_sponsor_service.py
scripts/test_oauth_usage.py
scripts/test_openrelay_discovery_test.py
scripts/test_reload.py
scripts/test_sdk_e2e.sh
scripts/test_sdk_package.sh
scripts/test_windows_launcher_install.ps1
scripts/test_windows_setup_evaluation.ps1
scripts/uninstall.ps1
scripts/uninstall.sh
scripts/update_packages.sh
scripts/verify_alt_shift_e_reaches_terminal.sh
scripts/verify_discovery_select.py
scripts/verify_donut_still_animates.py
scripts/watch_release_run.sh
scripts/which_overlay_blocks_donut.py
scripts/why_no_animation_area.py
sdk/npm/darwin-arm64/bin/.gitkeep
sdk/npm/darwin-arm64/package.json
sdk/npm/darwin-arm64/README.md
sdk/npm/darwin-x64/bin/.gitkeep
sdk/npm/darwin-x64/package.json
sdk/npm/darwin-x64/README.md
sdk/npm/linux-arm64/bin/.gitkeep
sdk/npm/linux-arm64/package.json
sdk/npm/linux-arm64/README.md
sdk/npm/linux-x64/bin/.gitkeep
sdk/npm/linux-x64/package.json
sdk/npm/linux-x64/README.md
sdk/npm/win32-arm64/bin/.gitkeep
sdk/npm/win32-arm64/package.json
sdk/npm/win32-arm64/README.md
sdk/npm/win32-x64/bin/.gitkeep
sdk/npm/win32-x64/package.json
sdk/npm/win32-x64/README.md
sdk/typescript/examples/demo-app/index.mjs
sdk/typescript/examples/demo-app/package.json
sdk/typescript/examples/demo-app/README.md
sdk/typescript/examples/stream-chat.mjs
sdk/typescript/.gitignore
sdk/typescript/LICENSE
sdk/typescript/package.json
sdk/typescript/package-lock.json
sdk/typescript/README.md
sdk/typescript/RELEASING.md
sdk/typescript/src/binary.ts
sdk/typescript/src/client.ts
sdk/typescript/src/errors.ts
sdk/typescript/src/framing.ts
sdk/typescript/src/index.ts
sdk/typescript/src/launch.ts
sdk/typescript/src/protocol.ts
sdk/typescript/src/sockets.ts
sdk/typescript/src/structured.ts
sdk/typescript/test/client.test.ts
sdk/typescript/test/error-docs.test.ts
sdk/typescript/test/launch.test.ts
sdk/typescript/test/live-capabilities.mjs
sdk/typescript/test/live-control.mjs
sdk/typescript/test/live-isolation.mjs
sdk/typescript/test/live-launch.mjs
sdk/typescript/test/live-options.mjs
sdk/typescript/test/live-turn.mjs
sdk/typescript/test/mock-harness.ts
sdk/typescript/test/model-usage.test.ts
sdk/typescript/test/pipe-name.test.ts
sdk/typescript/test/schema-parity.test.ts
sdk/typescript/test/structured.test.ts
sdk/typescript/tsconfig.json
src/bin/memory_recall_bench.rs
src/bin/mermaid_side_panel_probe.rs
src/bin/session_memory_bench.rs
src/bin/tui_bench.rs
src/bin/tui_bench/side_panel.rs
src/cli/commands/menubar.rs
src/cli/macos_notification_broker.rs
src/cli/selfdev.rs
src/cli/selfdev_tests.rs
src/cli/telemetry.rs
TELEMETRY.md
telemetry-worker/CONCURRENCY_ROLLOUT.md
telemetry-worker/concurrency.sql
telemetry-worker/conversion.sql
telemetry-worker/dau.sql
telemetry-worker/discovery.sql
telemetry-worker/geo.sql
telemetry-worker/.gitignore
telemetry-worker/health.sql
telemetry-worker/migrations/0001_expand_events.sql
telemetry-worker/migrations/0002_transport_metrics.sql
telemetry-worker/migrations/0003_usage_expansion.sql
telemetry-worker/migrations/0004_telemetry_phase123.sql
telemetry-worker/migrations/0005_workflow_turn_telemetry.sql
telemetry-worker/migrations/0006_token_usage.sql
telemetry-worker/migrations/0007_dashboard_indexes.sql
telemetry-worker/migrations/0008_agent_time_and_churn.sql
telemetry-worker/migrations/0009_feedback_text.sql
telemetry-worker/migrations/0010_daily_active_users.sql
telemetry-worker/migrations/0011_backfill_daily_active_recent.sql
telemetry-worker/migrations/0012_daily_active_ci_flag.sql
telemetry-worker/migrations/0013_detail_table_turn_session_fields.sql
telemetry-worker/migrations/0014_full_history_dau_backfill.sql
telemetry-worker/migrations/0015_auth_failure_reason.sql
telemetry-worker/migrations/0016_web_subscription_analytics.sql
telemetry-worker/migrations/0017_discovery_telemetry.sql
telemetry-worker/migrations/0018_web_quality_telemetry.sql
telemetry-worker/migrations/0019_discovery_benchmark_runs.sql
telemetry-worker/migrations/0020_install_conversion_funnel.sql
telemetry-worker/migrations/0021_todo_telemetry.sql
telemetry-worker/migrations/0022_geo_country.sql
telemetry-worker/migrations/0023_model_prices.sql
telemetry-worker/migrations/0024_todo_session_aggregates.sql
telemetry-worker/migrations/0025_transcript_uploads.sql
telemetry-worker/migrations/0026_concurrency_tracking.sql
telemetry-worker/opt-outs.sql
telemetry-worker/package.json
telemetry-worker/prompt-users.sql
telemetry-worker/README.md
telemetry-worker/repair-daily-active.sql
telemetry-worker/schema.sql
telemetry-worker/scripts/model-price-report.mjs
telemetry-worker/scripts/run-dashboard.mjs
telemetry-worker/scripts/sync-model-prices.mjs
telemetry-worker/src/concurrency.js
telemetry-worker/src/worker.js
telemetry-worker/test/concurrency.test.mjs
telemetry-worker/test/model-price-report.test.mjs
telemetry-worker/test/token-value.test.mjs
telemetry-worker/test/worker.test.mjs
telemetry-worker/token-value-daily.sql
telemetry-worker/token-value.sql
telemetry-worker/users.sql
telemetry-worker/wrangler.toml
tests/api_stdio_cli.rs
tests/e2e/ambient.rs
tests/e2e/transport.rs
tests/e2e/windows_lifecycle.rs
```
