# Message voice

How kcode talks to the user in system and status messages: TUI notices, CLI
output, notifications. The rule is to say what happened and what we did about
it, in the user's terms - not to narrate internal mechanics.

This applies to anyone writing user-facing strings. It does not apply to logs,
tracing, or developer-facing errors, which should keep precise internal names.

## The formula

> [What happened.] [What we did about it.] [How to change the behaviour.]

Only the first part is required, but the order holds.

## Principles

1. **Lead with what happened, in the user's terms.** The user cares about
   outcomes ("your agent didn't finish its work"), not mechanism names ("todo
   completion gate", "auto-poke armed", "queued continuation").
2. **Say what we did for them, past tense.** When the harness acts
   automatically, frame it as already handled: "We stopped automatic retries",
   not "Auto-retry circuit open".
3. **Escape hatch last, and short.** Controls go after the human sentence:
   "Run /login to re-authenticate, then send again." Never lead with the flag or
   the setting name.
4. **No internal jargon in user-facing text.** Words to avoid: gate, armed,
   dispatch, continuation, follow-up, queued dispatch, state, flag.
5. **Plain sentences over labels.** "Your agent stopped with 5 todos
   unfinished." beats "todo gate: 5 open".
6. **Warnings explain the consequence, then what to do.** "Stopped automatic
   retries: 3 consecutive auth failures, so the same request cannot succeed."
   Not "Gate exhausted after N attempts". Log lines keep the internal names.

## Examples

| before | after |
|---|---|
| `Auto-retry circuit open after 3 auth errors.` | `🛑 Stopped automatic retries: 3 consecutive credential/auth failures. The current login or API key for Anthropic is not working, so resending the same request cannot succeed. Run /login to re-authenticate (or /model to switch to a working route), then send again.` |
| `Network down. Backoff armed.` | `📡 Network appears offline - waiting to retry automatically.` |

## Out of scope

- Log lines, tracing and debug output keep precise internal names.
- Errors meant for developers (panics, internal errors) are not user-facing.
