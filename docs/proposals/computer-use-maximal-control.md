# Roadmap: maximal macOS control for the `computer` tool

Goal: give the agent as much reliable control over macOS as the platform allows,
including **background control that does not disturb what the user is looking at**.

This builds on the v1 `computer` tool (PR #345): screenshot, coordinate
mouse/keyboard, scroll, AX-tree read, cursor, permission check.

Everything below is implementable in Rust with crates that are already in the
lockfile or available on crates.io (`accessibility-sys`, `screencapturekit`,
`objc2-app-kit`, `core-graphics`). No Swift/ObjC build step.

---

## The one hard constraint

macOS has **one HID cursor and one keyboard-focus** shared by the whole login
session. Synthetic *coordinate* input (CGEvent) is therefore always visible: it
moves the real cursor and types into the focused app.

**Background / not-in-view control must avoid CGEvent** and instead go through:

1. **Accessibility (AX) actions** - act on a specific element by reference.
2. **Apple Events / scripting** - drive scriptable apps with no UI.
3. **Per-window capture** - "see" a window without raising it.

True simultaneous "you work + I work independently" needs a **separate display
or login session** (see Tier 4).

---

## Tier 0 - done (v1, PR #345)

- `screenshot` (main display, point/pixel scale aware)
- `move` / `click` / `double_click` / `right_click` / `drag` / `scroll`
- `type` / `key` (chords)
- `ui` (AX tree read via osascript)
- `cursor`, `check_permissions`

## Tier 1 - AX semantic actions  ← highest leverage for background control

Read + act on elements by reference, no cursor movement, target app need not be
frontmost. Uses `accessibility-sys` (`AXUIElementPerformAction`,
`AXUIElementSetAttributeValue`, `AXUIElementCopyElementAtPosition`,
`AXUIElementCopyAttributeValue`).

- `find_element { role?, title?, value?, pid?, app? }` -> stable element handles
- `element_at { x, y }` -> element under a point (AXUIElementCopyElementAtPosition)
- `press { element }` -> `AXPress` (click a button in a background window)
- `set_value { element, value }` -> type into a field without focus
- `get_value { element }`
- `perform_action { element, ax_action }` -> any advertised AX action
- `select_menu { app, path: ["File","Export…"] }` -> drive the menu bar of any app

Handle format: `pid` + AX path (index chain) or a session-scoped element id cache,
so the model can act structurally instead of by pixels.

Why it matters: this is the actual "click things you're not looking at" capability.

## Tier 2 - app / window / system management

Mostly `objc2-app-kit` (`NSWorkspace`, `NSRunningApplication`) + AX window
attributes + CoreGraphics window list.

- `list_apps` / `activate_app { app }` / `hide_app` / `quit_app`
- `list_windows { pid? }` (CGWindowList) with ids, titles, bounds, on/off-screen
- `focus_window` / `move_window` / `resize_window` / `minimize_window` / `close_window`
  (AX window actions - can target background windows)
- `window_screenshot { window_id }` -> capture a specific window even if occluded
  (`CGWindowListCreateImage` now, ScreenCaptureKit later)
- `spaces` awareness (which Space an app is on; activating may switch Spaces - visible)

## Tier 3 - clipboard, input fidelity, observation

- `get_clipboard` / `set_clipboard { text }` (`NSPasteboard` via objc2-app-kit)
- `key_down` / `key_up` (hold modifiers, game-style input)
- `type_into { element, text }` (AX set value + confirm) for reliability over blind typing
- `wait_for { element|condition, timeout }` using `AXObserver*` notifications
  (e.g. wait for a sheet to appear) instead of sleep-and-poll
- `paste_type { text }` - set clipboard + Cmd-V for fast/large text entry

## Tier 4 - true background / parallel operation (advanced)

These give genuinely off-screen, non-interfering control. Higher setup cost.

- **Apple Events scripting bridge**: `run_applescript { script }` / `run_jxa`.
  Fully headless for scriptable apps (Mail, Notes, Safari, Finder, Music, System
  Settings panes, Terminal, many pro apps). No cursor, no focus. Per-app
  Automation permission (prompts on first use).
- **Virtual / headless display**: route the agent's cursor+windows to a second
  (virtual) display the user isn't looking at. Options: a virtual display driver
  (e.g. BetterDisplay/`CGVirtualDisplay` private API) or a real unused monitor.
  Lets the agent move windows there and use coordinate input without touching the
  user's screen.
- **Separate login / Screen Sharing session**: a second macOS session has its own
  cursor and focus; the agent drives that one. Strongest isolation, most setup.
- **Shortcuts integration**: invoke the user's `Shortcuts` automations
  (`shortcuts run …`) as high-level, sanctioned actions.

## Tier 5 - sensors / extras (optional, opt-in)

- `ocr { region|window }` via Vision framework (read text in images / non-AX apps).
- `screen_record { seconds }` short clips via ScreenCaptureKit.
- Audio in/out control, notifications, `do_not_disturb` toggling via scripting.
- Camera/mic are separate TCC permissions; keep strictly opt-in.

---

## Permissions (TCC) - the gatekeeping reality

| Permission | Unlocks | Auto-grantable? |
|---|---|---|
| **Accessibility** | CGEvent input, all AX read/act, window control | No - user toggles once (we can prompt + deep-link) |
| **Screen Recording** | screenshots, window/ocr capture | Request API exists (`CGRequestScreenCaptureAccess`) |
| **Automation (Apple Events)** | scripting each app | Prompts per target app on first send |
| **Input Monitoring** | reading global input stream (only if we add capture) | Request API exists |

Plan: a `request_permissions` action that calls
`AXIsProcessTrustedWithOptions(prompt=true)` (adds jcode to the list + shows the
dialog) and deep-links to the exact System Settings pane, then polls
`AXIsProcessTrusted()`. One prompt + one toggle; never zero-touch for Accessibility
(Apple's anti-malware boundary).

Important: the permission attaches to the **host binary/terminal** running jcode.
For a stable experience we likely want a signed jcode.app with a fixed bundle id so
the grant persists across updates (otherwise each new binary path re-prompts).

## Safety model (high blast radius)

- Gated like `bash`: refuses early if required permission missing.
- `dry_run` on mutating actions: resolve + report target without acting.
- Prefer AX semantic actions over blind coordinate clicks (auditable, robust).
- Screenshot/element echo on destructive coordinate clicks.
- No global input *capture* unless explicitly enabled (keeps us out of Input
  Monitoring by default).
- Per-action audit log; optional allowlist/denylist of target apps.

## Suggested build order

1. **Tier 1 (AX actions)** - biggest capability jump, enables background control.
2. **Tier 2 window mgmt + per-window screenshot** - "see and act on hidden windows".
3. **Tier 3 clipboard + AXObserver waits** - reliability.
4. **`run_applescript`/JXA bridge (Tier 4)** - headless scripting for many apps.
5. **Virtual-display / second-session (Tier 4)** - true parallel, non-interfering.
6. Signed jcode.app bundle for durable permissions.
7. Vision OCR (Tier 5) as needed.

## Crates

- `accessibility-sys` 0.2 (AX read/act/observe) - on crates.io
- `screencapturekit` 7 (modern capture) - on crates.io; `core-graphics` window list as fallback
- `objc2-app-kit` / `objc2-foundation` 0.3 - already in lockfile (NSWorkspace, NSPasteboard)
- `core-graphics` 0.23 - already a direct dep (CGEvent, CGWindowList, CGDisplay)

---

## Tool interface design (decided)

### Single tool, progressive disclosure

One `computer` tool, `action`-dispatched (like `browser`). To keep always-on
context flat regardless of how many tiers exist, the schema uses **progressive
disclosure**:

- **Always-on core (~370 tokens, measured with tiktoken cl100k_base):**
  `screenshot, ui, ocr, click, type, key, press, set_value, run_applescript,
  setup, discover`.
- **`discover { category }`** returns full specs for advanced actions on demand
  (`mouse|keyboard|ax|windows|apps|clipboard|scripting|displays|system|all`),
  ~130 tokens per category, paid only when used.
- **Shared handle types** (`element`, `window_id`, `region`) defined once and
  reused, so params do not multiply with actions.

Measured always-on cost:

| Design | Actions visible | Always-on tokens |
|---|---|---|
| Current v1 tool | 12 | ~720 |
| Flat, all tiers (~46 actions) | 46 | ~1,020 |
| **Progressive core** | 11 | **~370** |

Background control is a property of the *mechanism*, not the tier: CGEvent =
visible; **AX actions (press/set_value/select_menu) + Apple Events = background**.

### `setup` / `check_permissions` action

A first-class `setup` action that:

1. **Reports** status of every requirement: Accessibility (`AXIsProcessTrusted`),
   Screen Recording (`CGPreflightScreenCaptureAccess`), Automation (per-app, via
   first Apple Event), plus install/bundle health.
2. **Requests** what it can programmatically:
   - `AXIsProcessTrustedWithOptions(prompt=true)` — shows the Accessibility dialog
     and pre-adds jcode to the list (toggled off).
   - `CGRequestScreenCaptureAccess()` — prompts for Screen Recording.
   - First Apple Event to a target app — triggers its Automation prompt.
3. **Deep-links** to the exact System Settings pane for anything still missing:
   - `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`
   - `…?Privacy_ScreenCapture`
   - `…?Privacy_Automation`
4. **Polls** `AXIsProcessTrusted()` until granted, then reports "ready".

**Hard limit:** the Accessibility *toggle itself cannot be flipped by any API*
(Apple anti-malware boundary). `tccutil` can only reset, not grant. So the best
achievable UX is **"one or two prompts + one toggle,"** never zero-touch.

### Durable permissions: signed app bundle

TCC permissions attach to the **running binary's identity**. A bare dev/cli binary
changes path/signature across updates, so macOS re-prompts every time. To make a
grant stick:

- Ship/install jcode as a **signed `.app` bundle with a stable bundle id**
  (e.g. `com.jcode.app`) and a Designated Requirement, so the Accessibility /
  Screen Recording grant persists across updates.
- `setup` should detect "running from an unstable/unsigned path" and offer to
  install the proper bundle, so the user grants **once**.

### Build order (updated)

1. Progressive-disclosure refactor of the v1 tool (core + `discover`).
2. `setup` action (check + request + deep-link + poll).
3. Tier 1 AX actions (background control).
4. Tier 2 window/app management + per-window screenshot.
5. Tier 3 clipboard + AXObserver waits.
6. `run_applescript`/JXA bridge (Tier 4 headless scripting).
7. Signed app bundle for durable permissions.
8. Tier 5 OCR (Vision). (Camera/audio intentionally excluded.)
9. Virtual-display / second-session for true parallel work (advanced).
