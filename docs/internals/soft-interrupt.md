# Soft interrupt

A soft interrupt injects a message into a running turn without cancelling it. The
message is queued and delivered at the next safe point, so the model sees it
naturally on the following loop iteration.

A hard interrupt (cancel, wait, resend) loses whatever partial work was in
flight, adds the cancellation delay, and re-sends the full context. Soft
interrupt does none of that.

## The API constraint

Anthropic requires every `tool_use` block to be immediately followed by its
matching `tool_result`; no user text may sit between them. Injection is therefore
only safe where no `tool_use` is awaiting a result.

## Safe points

| point | when | notes |
|---|---|---|
| **B** | turn ends with no tool calls | inject, then continue the loop instead of breaking |
| **C** | between tool executions | **urgent only**: skip the remaining tools, recording stub `tool_result`s for each first |
| **D** | after all tools, before the next provider call | the default and safest point |

Non-urgent interrupts are always deferred to point D. Urgent ones may abort the
remaining tools at point C.

## Entries

A queued entry carries its content (and optional `images`), an `urgent` flag, and
a `source` (`User`, `System`, or `BackgroundTask`) that decides its display role.
Pending entries are drained and injected together.

The queue is persisted per session and restored on resume/reload, so a queued
message survives a server reload.

## Protocol

- Request `soft_interrupt {id, content, images, urgent}`.
- Request `cancel_soft_interrupts {id}` removes pending entries before injection.
- Event `soft_interrupt_injected {content, point}` reports which safe point it
  landed at, so the TUI can show it.

Debug socket: `queue_interrupt:<content>` and `queue_interrupt_urgent:<content>`.
