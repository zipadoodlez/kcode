# Auth

Most "kcode has no credential for provider X" reports are wrong: the credential
exists and works, it just lives somewhere the person did not look. This doc says
where each one lives, so you stop guessing.

## Ask, don't grep

`kcode auth status --json` is the canonical answer for every provider. Each entry
reports `status`, `auth_kind` (`OAuth` vs `API key`), `credential_source` (env
var, app config file, or a kcode-managed file), and the exact `method`.

Grepping for a key name is how you end up with the wrong conclusion:
`printenv ANTHROPIC_API_KEY` returning nothing does **not** mean there is no key,
and grepping for `sk-ant-api` misses an OAuth-only setup.

## Where credentials live

Claude and OpenAI each support **two entirely independent credential paths**,
exposed as **two separate login providers**. This is the single most common
source of confusion.

| concept | login provider id | kind | where it lives |
|---|---|---|---|
| Claude via subscription | `claude` | OAuth | `~/.kcode/auth.json` → `anthropic_accounts[].access` |
| Claude via Anthropic API key | `anthropic-api` | API key | `ANTHROPIC_API_KEY`, or `~/.config/kcode/anthropic.env` |
| OpenAI via subscription | `openai` | OAuth | `~/.kcode/openai-auth.json` (Codex/ChatGPT login) |
| OpenAI via API key | `openai-api` | API key | `OPENAI_API_KEY`, or `~/.config/kcode/openai.env` |

Other providers follow the same shape: `~/.config/kcode/<provider>.env` for API
keys (`openrouter.env`, `gemini.env`, `cursor.env`, `azure-openai.env`,
`cerebras.env`, …), and `~/.kcode/auth.json` for OAuth accounts. Native OAuth
tokens live under `~/.kcode`: `auth.json` (Claude accounts),
`openai-auth.json` (OpenAI), and `gemini_oauth.json` / `antigravity_oauth.json`
for those providers. Which command logs in to which provider is in
[providers.md](providers.md).

Three traps worth internalising:

- **An OAuth token is not an API key.** Anthropic OAuth access tokens look like
  `sk-ant-oat01-...` (refresh tokens `sk-ant-ort01-...`); a real API key looks
  like `sk-ant-api03-...`. A search for one will not find the other.
- **API keys usually live in the app config dir, not an env var.**
  `kcode login --provider anthropic-api` writes
  `~/.config/kcode/anthropic.env`.
- **`claude` and `anthropic-api` are different providers.** Having a Claude
  subscription does not make `anthropic-api` usable, and vice versa.

## Choosing a default

```toml
# ~/.kcode/config.toml
[provider]
default_provider = "claude"          # Claude subscription (OAuth)
# default_provider = "anthropic-api" # Claude via direct API key instead
default_model = "claude-opus-4-8"
anthropic_reasoning_effort = "xhigh"
```

`anthropic-api` deliberately **does not** fall back to OAuth: if no API key is
configured, the request fails rather than silently spending the wrong credential.

## "It says expired" - the validation cache is not live state

`~/.kcode/auth-validation.json` caches the result of the **last** runtime auth
test per provider. It is history, not current state: a token that has since
auto-refreshed can still show a days-old `expired` entry. Records older than
**7 days** are labelled `stale` for exactly this reason - treat a stale record as
"unknown, re-check", never as fact, and re-validate with:

```sh
kcode auth-test --provider <id>
```

## Importing logins from other agent tools

On a fresh install kcode can reuse logins left behind by other coding agents,
both OAuth tokens and API keys. Import is **consent-gated**: kcode lists the
sources it found and reads them only after you approve each one. Nothing is
copied into kcode's stores - the external file is read in place - and you can
remove the access again from the same surface.

| tool | auth file |
|---|---|
| OpenCode | `~/.local/share/opencode/auth.json` |
| pi | `~/.pi/agent/auth.json` |
| OpenClaw | `~/.openclaw/agent/auth.json`, `~/.openclaw/agents/<id>/agent/auth{,-profiles}.json`, `~/.openclaw/credentials/oauth.json` |
| Hermes | `~/.hermes/auth.json` |

Tool-specific importers also exist for Claude Code, Codex, Gemini CLI, GitHub
Copilot and Cursor.

Two safety notes on foreign files:

- `$ENV_VAR` references inside them are resolved against your environment.
- Values beginning with `!` (shell commands in the pi/OpenClaw format) are
  **never executed** and are skipped.

## Decision tree for "is provider X authenticated?"

1. Run `kcode auth status --json` and read the entry for the **specific** provider
   id - `claude` and `anthropic-api` are different rows.
2. Only if you must inspect files: OAuth → `~/.kcode/auth.json`; API key →
   the provider's env var or `~/.config/kcode/<provider>.env`.
3. Ignore `auth-validation.json` verdicts older than 7 days (shown as `stale`);
   run `kcode auth-test --provider <id>` instead.

For contributors: the single source of truth is `AuthStatus` in
`crates/jcode-base/src/auth/mod.rs`; the route vocabulary shared by the runtime,
the CLI and model prefixes is centralized in
`crates/jcode-provider-core/src/auth_mode.rs` (`AuthRoute`). Do not re-parse those
strings by hand.
