# Proposal: native `computer` tool for macOS computer use

## Summary

Add a single native tool, **`computer`**, that lets the agent observe and control
the macOS GUI — screenshots, the accessibility (AX) tree, mouse/keyboard input,
window/app management, and clipboard — through one `action`-dispatched interface.

This mirrors the existing **`browser`** tool (`crates/jcode-app-core/src/tool/browser.rs`):
one registered tool, an `action: String` that selects a sub-operation, with optional
typed params. It gives jcode a closed control loop (*see screen → decide → act*)
without depending on a browser or external automation tooling.

## Motivation

- The agent can already drive a browser; it cannot drive native macOS apps, system
  UI, or anything outside the browser sandbox.
- "Computer use" agents need exactly three primitives: **read the screen**, **read
  UI structure**, and **synthesize input**. macOS exposes all three through the
  Accessibility / Quartz Event Services / ScreenCaptureKit stack.
- A single, well-scoped tool keeps the tool surface small and the permission story
  in one place.

## Architecture

```
crates/jcode-macos-control/        (new) cfg(target_os = "macos") platform crate
  └─ AX (accessibility-sys), CGEvent (core-graphics),
     CoreFoundation (core-foundation), screenshots (ScreenCaptureKit / CGDisplay),
     app/window control (objc2 + objc2-app-kit), clipboard (objc2 NSPasteboard)

crates/jcode-app-core/src/tool/computer.rs   (new) ComputerTool
  └─ thin dispatch layer: parse input -> call jcode-macos-control -> ToolOutput
  └─ registered in crates/jcode-app-core/src/tool/mod.rs base_tools()
```

- All native APIs are reached through existing Rust bindings (`objc2`,
  `accessibility-sys`, `core-graphics`, `core-foundation`) — **no Swift/ObjC build
  step**.
- On non-macOS targets the tool still registers but every action returns a clean
  `unsupported on this platform` error, so the tool list stays stable across OSes.
- `screenshot` returns its image via `ToolOutput::with_image` (base64), matching how
  `browser` returns screenshots today.

## Permissions (the important part)

macOS splits this across **four** TCC permissions. Programmatic *request* support
differs per permission:

| Permission | Used for | Programmatic request |
|---|---|---|
| **Accessibility** | drive other apps' UI, inject `CGEvent` input | ⚠️ prompt + deep-link only; user must toggle |
| **Screen Recording** | screenshots / `get_ui_tree` of some apps | ✅ `CGRequestScreenCaptureAccess()` |
| **Input Monitoring** | reading the global input stream | ✅ `IOHIDRequestAccess(...)` |
| **Automation** (Apple Events) | scripting cooperating apps | ✅ prompts on first send, per target app |

**Accessibility is the one that cannot be auto-granted** (Apple's anti-malware
boundary), and it is required for input injection. Best achievable flow, exposed via
the `request_permissions` action:

1. `AXIsProcessTrustedWithOptions([kAXTrustedCheckOptionPrompt: true])` — shows the
   system dialog *and auto-adds jcode to the Accessibility list* (toggled off).
2. Deep-link to the exact pane:
   `open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"`.
3. Poll `AXIsProcessTrusted()` until granted, then report ready.

So the experience becomes **one prompt + one toggle**, not "go hunt in Settings" —
but never zero-touch for Accessibility.

`check_permissions` reports the current state of all four so the agent can tell the
user precisely what is missing before attempting control.

## Actions

`action` (required) selects the operation. Params below are optional and validated
per action.

**Permissions**
- `check_permissions` → status of accessibility / screen-recording / input-monitoring
- `request_permissions` → prompt + deep-link flow above

**Observe**
- `screenshot` — `{ display?, window_id?, region? }` → image
- `get_ui_tree` — `{ pid? | frontmost, max_depth? }` → serialized AX tree (role, title, value, position, size, actions)
- `find_element` — `{ role?, title?, value?, pid? }` → matching elements + their identifiers
- `element_at` — `{ x, y }` → element under the point

**Mouse**
- `move` — `{ x, y }`
- `click` / `double_click` / `right_click` — `{ x?, y? }` (current position if omitted)
- `drag` — `{ from: [x,y], to: [x,y] }`
- `scroll` — `{ x?, y?, dx, dy }`

**Keyboard**
- `type` — `{ text }`
- `key` — `{ keys: "cmd+shift+4" }` (chord)
- `key_down` / `key_up` — `{ key }`

**Semantic AX (preferred over raw input when available)**
- `press` — `{ element }` (AXPress)
- `set_value` — `{ element, value }`
- `get_value` — `{ element }`
- `perform_action` — `{ element, ax_action }`
- `select_menu` — `{ app, path: ["File", "Export…"] }`

**Window / app**
- `list_apps`, `activate_app` `{ app }`
- `list_windows` `{ pid? }`, `focus_window` `{ window }`
- `move_window` `{ window, x, y }`, `resize_window` `{ window, w, h }`
- `minimize_window` / `close_window` `{ window }`

**Clipboard**
- `get_clipboard`, `set_clipboard` `{ text }`

> Element identifiers: `find_element` / `get_ui_tree` return stable-enough handles
> (e.g. `pid` + AX path or a session-scoped element id) that semantic actions accept,
> so the agent can act structurally instead of by pixel coordinates when possible.

## Safety

Computer use is high-blast-radius, so:

- **Permission-gated** like other powerful tools; refuses early with a clear message
  if Accessibility/Screen Recording is missing.
- **`dry_run` param** on mutating actions — resolves and reports the target without
  acting.
- **Screenshot-assisted confirmation** for destructive coordinate clicks (return the
  region/element being targeted).
- **No global input *capture*** in v1 (we synthesize input but do not log the user's
  keystrokes), keeping us out of Input Monitoring unless a future feature needs it.
- Prefer **semantic AX actions** over blind coordinate input wherever the element is
  resolvable — more robust and more auditable.

## Implementation plan

1. `jcode-macos-control` crate: permissions, screenshot, AX read, AX action,
   CGEvent input, window/app control, clipboard. Unit-test the pure parts
   (input parsing, chord parsing, tree serialization).
2. `ComputerTool` in `tool/computer.rs`: input struct + `action` dispatch +
   schema + description; register `"computer"` in `tool/mod.rs` `base_tools()`.
3. Default-off / gated rollout + docs in `docs/`.
4. Follow-up: Windows/Linux backends behind the same tool surface.

## Open questions

- Element handle format — `pid`+AX-path vs an opaque session-scoped id cache?
- Should `request_permissions` block-and-poll, or return immediately with status and
  let the agent re-check?
- Default enablement: opt-in flag vs always-registered-but-gated?
