# What this fork removed

kcode is jcode minus a large amount of surface area. Two rounds of removal, and
what was deliberately kept.

## The fork cut

Roughly **241,000 lines across 1,079 files**, in one commit
(`feat!: remove mermaid, diagrams, inline images, latex and replay`):

- mermaid/diagram rendering, inline images, LaTeX, session replay
- agent memory and ambient mode
- Gmail and Google login, dictation, the productivity dashboard
- the macOS computer-use tool, the menubar app, the client installer
- the iOS app, the telemetry worker, the TypeScript SDK

## The account cut

The jcode.sh account, subscription and hosted-model surface:

- `kcode account login/status/manage/logout` and its device-auth flow
- the `jcode` provider ("Jcode Subscription") - **this removes the only route to
  Jcode's hosted models**
- `/subscription`, `/subscribe`, `/hosted`, `/support`, and the hosted-model
  nudge
- the `subscription_api`, `subscription_catalog`, `account_login`,
  `provider/jcode`, `cli/account`, `jcode_device`, `subscribe_nudge` and
  `support` modules
- the "Jcode subscription" pill in the login-import summary

## Ambient leftovers, removed later

The fork cut ambient mode but left its CLI and transcript types behind. Now
gone as well:

- `kcode permissions` and the `jcode-tui-permissions` crate. With ambient mode
  removed, nothing ever enqueued a permission request, so the review TUI was
  unreachable.
- The dangling `#[command(subcommand)]` and ambient doc comment on the
  `Permissions` variant, which had no subcommands.
- `AmbientTranscript` and `SafetySystem::save_transcript` in `jcode-base` (zero
  callers).

## Deliberately kept

- `/account` and `/accounts` - the multi-account picker for Claude and OpenAI.
- `grok-build` - independent of the account surface.
- The `jcode` provider id string, `jcode.sh` URLs, and the `_jcode` ACP
  capability: these name the *service*, not this binary, and removing them would
  break wire compatibility.

## Why docs for removed features do not live here

They are deleted, not archived. `git log` is the archive. A doc set that
describes features the code does not have is worse than no doc set, because it
teaches readers to distrust all of it.
