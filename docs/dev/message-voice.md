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
   automatically, frame it as already handled: "We poked it for you", not
   "Auto-poking..." or "Poke scheduled".
3. **Escape hatch last, and short.** Controls go after the human sentence:
   "... `/poke off` to stop." Never lead with the flag or the setting name.
4. **No internal jargon in user-facing text.** Words to avoid: gate, armed,
   dispatch, continuation, follow-up, queued dispatch, state, flag.
5. **Plain sentences over labels.** "Your agent stopped with 5 todos
   unfinished." beats "Auto-poke: 5 incomplete todos."
6. **Warnings explain the consequence, then what to do.** "We stopped poking
   because it wasn't making progress. Review the remaining todos." Not "Gate
   exhausted after N attempts."

## Examples

| before | after |
|---|---|
| `👉 Auto-poking: 5 incomplete todos. /poke off to stop.` | `👉 5 incomplete todos. We poked it for you. /poke off to stop.` |
| `Auto-poking: todos complete; sending confidence summary follow-up.` | `Todos are done. Asking the agent for a final confidence check.` |
| `⚠️ Todo completion gate: validation still failing after repeated nudges. Auto-poke stopped; review the remaining todos manually.` | `⚠️ We poked the agent several times but it stopped making progress. Giving up; review the remaining todos yourself.` |

## Out of scope

- Log lines, tracing and debug output keep precise internal names.
- Errors meant for developers (panics, internal errors) are not user-facing.
