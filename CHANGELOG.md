# CHANGELOG

## Unreleased
- added signed stable-version update checks, About-page install/restart flow, portable download fallback, and a clickable main-header update indicator
- added Windows x64 release automation for the MSI installer, directly runnable portable executable, GitHub Raw update manifest, and Tauri signing artifacts
- added macOS universal `.dmg` and compressed `.app` assets to the unified GitHub Release workflow
- standardized release filenames with explicit version, platform, architecture, and portable markers
- moved autostart into General settings and added a persisted option to show its toggle on the main page
- hid the UI scrollbar while preserving scrolling, and removed the refresh action from the Settings page
- matched cc-switch's native Lucide icon treatment for Settings, navigation, startup, and refresh; placed the Power-icon autostart switch in the main header control row with a 44 × 24 green switch
- replaced the refresh action's misleading plus icon with a Lucide RefreshCw icon and a loading rotation state
- made Settings a dedicated secondary page with a back affordance instead of rendering it below the main navigation tabs; removed the duplicate autostart control from the Settings page
- matched the cc-switch settings TabList with a translucent rounded surface and blue active tab, and rebuilt About as an application information card
- routed the AirSlate title and About GitHub actions through Tauri's opener plugin so both open in the system browser
- reduced the AirSlate wordmark weight to match the lighter cc-switch header treatment
- aligned the Tauri window mechanism with cc-switch: 1000 × 650 default size, 900 × 600 minimum size, hidden startup window, persisted position/size/maximized state, and content-owned scrolling
- realigned the AirSlate shell against the local cc-switch frontend source: a 64 px toolbar, 20 px navigation icons, text-sm medium tab labels, muted rounded tab groups, and lightweight white active states
- rebuilt Settings as a full-height tab workspace with cc-switch sizing and card treatment
- made the AirSlate title link to the project GitHub page and added General, Advanced, and About settings tabs
- bounded queued contact moves to the newest eight samples within a 16 ms ordered window, coalesced stale hover moves, and preserved down, up, cancel, gesture, and shortcut boundaries; removed per-frame injection logs and redundant hold-indicator redraws to prevent intermittent input backlog without flattening pen pressure
- completed the macOS CoreGraphics key-code mapping for the expanded shortcut catalog
- fixed Windows pen mapping on extended desktops by deriving each target's mapping extent from its monitor rectangle, preventing DPI-scaled primary-display input from spilling onto a secondary display
- added a GitHub Actions workflow that builds universal Intel and Apple Silicon macOS `.app` and `.dmg` artifacts on a real macOS runner
- integrated macOS platform support from PR #6 while preserving the expanded shortcut model, including generic left/right click execution on both desktop platforms
- fixed Retina/HiDPI pen mapping to use CoreGraphics event-coordinate display bounds, corrected CoreGraphics setter FFI signatures, and preserved non-zero fine wheel deltas
- removed fabricated monitor and no-op input implementations for unsupported platforms; the server now explicitly compiles only for Windows and macOS
- added a per-binding special-action popover that exposes only actions supported by the selected gesture, including left/right click, coordinate movement, left/right drag, wheel, and rotation movement
- separated keyboard keys from special actions so keys remain independently editable, may be empty, and execute together with the selected special action without either setting overwriting the other
- generalized pointer injection to support both left and right click facts, and added runtime coverage for combined keyboard-plus-click execution and empty keyboard components
- expanded shortcut recording and Windows injection from the original limited key set to cover left/right modifiers, Windows keys, navigation and arrow keys, F1-F24, punctuation, the numeric keypad, lock keys, media controls, and browser controls
- switched desktop key capture to physical `KeyboardEvent.code` values so main-keyboard and numpad keys, plus left/right modifiers, remain distinct in saved presets; Escape can now be recorded instead of being consumed as an implicit cancel command
- marked Win32 extended keys during `SendInput`, preserving the runtime identity of right-side modifiers, navigation keys, numpad Enter/divide, and multimedia keys

## 1.4.10 - 2026-04-30
- added a navigation-bar launch-at-startup toggle to the Tauri desktop UI and persisted the setting through the existing config/bootstrap bridge so the shell can render the current autostart state on load
- wired Tauri's autostart plugin through the Rust desktop shell, passing `--autostart` on login launch and skipping initial main-window creation in that startup path so the app stays tray-only until the user opens the UI
- removed the connection-status badge from the LAN IPv4 hero card header so the address panel shows only the IP information surface
- fixed the Tauri bundle build hooks to run frontend commands from the actual frontend working directory, so release packaging resolves the correct `package.json` and succeeds again
- verified `npm --prefix ./frontend run build`, `cargo build --release`, and `cargo tauri build`, producing the Windows installer and standalone executable

## 1.4.9 - 2026-04-29
- made the radial menu inner ring configurable from the shortcut UI by dragging one inner key onto another to swap their positions
- exposed persisted inner-ring bindings through the desktop bridge, added a Tauri update command, and made the native overlay render the configured keys instead of hardcoded labels
- verified `cargo fmt --check`, `cargo test`, `cargo build`, `npm --prefix ./frontend run build`, and Vite dev-server startup on `http://127.0.0.1:5173`

## 1.4.8 - 2026-04-29
- updated the shortcut radial-menu illustration so the outer ring uses arrow icons inside white circular controls and the inner ring uses matching keycap pills
- labeled the fixed inner ring with Alt / Ctrl / Shift / Space only and kept the outer ring labels direction-only
- verified `npm --prefix ./frontend run build`

## 1.4.7 - 2026-04-29
- refined the pressure curve card into a modern minimalist control surface with live entry/release/bend readouts, balanced preset controls, and a more polished square adjustment board
- improved the curve editor visual hierarchy with axis labels, layered glass-style paneling, subtle grid texture, elevated bezier strokes, and clearer draggable handles
- verified `npm --prefix ./frontend run build`

## 1.4.6 - 2026-04-28
- rebuilt the React desktop UI around a connection-first control console, making LAN IPv4 addresses the primary readable surface and moving monitor plus pressure status into focused panels
- split the shortcut studio into clearer preset management, effective binding groups, and a redesigned SVG radial menu that only represents the center, inner ring, and outer ring from real runtime data
- replaced the scattered card/dashboard styling with unified shell, panel, button, select, key-token, pressure-curve, and radial-menu visual systems
- verified `npm --prefix ./frontend run build` and `cargo build`; Tauri dev startup from the current npm prefix still fails because the CLI resolves `frontend/` as the project root instead of the repository-level `tauri.conf.json`

## 1.4.5 - 2026-04-27
- extended long-press shortcut bindings from single-key holds to full key-combo holds, so edited long-press gestures now press all configured keys together and release them in reverse order when the gesture ends or expires
- updated the desktop bridge and shortcut UI to treat hold bindings as multi-key values, allowing live recording and display of combination keys for long-press gestures instead of forcing single-key capture
- added runtime coverage for hold-combo press/release order and verified `cargo test`, `cargo build`, and `npm --prefix ./frontend run build`

## 1.4.4 - 2026-04-27
- fixed shortcut config persistence after live key editing by serializing `custom_bindings` through explicit string binding ids instead of raw enum map keys, which removes the TOML `map key was not a string` failure once users save edited shortcuts
- added persistence and runtime tests proving edited bindings survive config round-trip and are actually emitted by the shortcut runtime executor path after reload
- verified `cargo test`, `cargo build`, and `npm --prefix ./frontend run build`

## 1.4.3 - 2026-04-27
- replaced the shortcut key editing flow with live keyboard capture, so clicking an editable binding or radial outer slot now waits for a real key press or key combination and persists the detected mapping directly from the UI
- unified backend shortcut updates behind typed key-sequence commands for hold, chord, drag, wheel, rotate, and radial outer-slot bindings, while removing the stale bootstrap key catalog that only existed for dropdown-based selection
- verified `npm --prefix ./frontend run build`, `cargo build`, `cargo test`, and frontend dev startup on `http://127.0.0.1:4173`

## 1.4.2 - 2026-04-27
- enabled Tauri's `custom-protocol` production feature in `Cargo.toml`, so direct `cargo build --release` builds no longer compile the desktop shell in dev mode or route the main window to `http://localhost:5173`
- verified `cargo check` and `cargo build --release`, and confirmed the latest Tauri build output now emits `cargo:dev=false` plus `cargo:rustc-cfg=custom_protocol` for the release path

## 1.4.1 - 2026-04-27
- fixed the Tauri frontend build wiring so desktop packaging now runs the React build from `frontend/` instead of the repository root, matching the actual project layout
- moved the frontend production build into `build.rs` so direct Cargo builds regenerate the embedded web assets before producing the desktop executable, which prevents stale or blank release windows after UI changes
- verified `cargo fmt`, `cargo check`, `cargo clean`, `cargo build`, `cargo test`, short-lived `target/debug/airslate_pc_server.exe`, and `cargo build --release`

## 1.4.0 - 2026-04-27
- replaced the old shortcut profile toggle flow with a named preset library model, so the desktop UI now tracks one active preset at a time, allows creating additional editable presets with custom names, and lets every preset restore itself back to the default mapping baseline
- redesigned the Tauri shortcut settings UI into a coherent dark control-surface layout, promoted `gesture:two_pan` to the top of the information hierarchy, and added a left-side radial ring diagram with fixed inner-ring labels plus right-side outer-slot editors that map cleanly to each visible direction
- expanded desktop key editing support across the preset workflow so outer-ring slots can be reassigned from a larger built-in key catalog while preserving the fixed inner-ring defaults and the existing runtime shortcut execution path
- verified `cargo fmt`, `cargo check`, `cargo clean`, `cargo build`, `cargo test`, `cargo run`, and `npm run build --prefix frontend` after the preset-system and UI redesign

## 1.3.1 - 2026-04-26
- changed Harmony gesture defaults so `squeeze` now right-clicks at the current hover or last in-range point and `twoRotate` now triggers a single `R` press instead of entering pointer-rotate control
- retuned long-press hold TTL for the current 200ms Harmony repeat cadence, and updated shortcut tests plus protocol docs to match the new packet timing contract
- verified `cargo fmt`, `cargo test`, `cargo build`, and `npm run build --prefix frontend` after the shortcut mapping update

## 1.3.0 - 2026-04-26
- implemented stage 10 `twoPan` radial menu semantics end to end, including radial-menu profile persistence, polar hit-testing, inner-ring toggle modifiers, outer-ring one-shot chords, and cleanup on session end/runtime shutdown
- added a native Win32 + Direct2D radial overlay service with a visually windowless layered host, topmost translucent rendering, and explicit inner-4 / outer-8 ring segmentation instead of routing the menu through the Tauri window system
- extended the Tauri desktop bridge and React shortcut page so `gesture:two_pan` now exposes the current radial layout and allows editing all 8 outer-ring slots from the control panel
- verified `cargo fmt`, `cargo test`, `cargo build`, and `npm run build --prefix frontend` after the stage 10 integration

## 1.2.0 - 2026-04-26
- replaced the phase 9 `eframe/egui` desktop shell with a fresh Tauri v2 + React + TypeScript + Tailwind desktop UI, rebuilding the visual layer from scratch instead of carrying over the old layout
- preserved the Rust runtime boundaries while adding a Tauri desktop bridge for bootstrap state, monitor selection, pressure sensitivity, shortcut profile display, category toggles, and preset-to-custom cloning
- added a tray-driven window lifecycle so the backend can stay alive while closing the main window destroys the WebView and reopening the UI recreates it on demand to avoid idle background CPU use
- generated standard application icons from `doc/foreground.png` and wired the project for Tauri packaging, frontend builds, and single-instance desktop behavior
- verified `npm run build --prefix frontend`, `cargo fmt`, `cargo check`, `cargo test`, and `cargo build` against the new desktop shell

## 1.1.1 - 2026-04-26
- removed `TwoSwipe` and `ThreeSwipe` protocol support so the server no longer accepts or exposes the client-cancelled two-finger and three-finger swipe gestures
- trimmed shortcut bindings, default presets, and UI rows to keep only `oneSwipe` while preserving the existing `Swipe` category toggle and runtime handling path
- updated protocol and planning docs to match the reduced swipe surface, and added protocol coverage asserting `gestureType=10/11` is rejected
- verified `cargo fmt`, `cargo check`, `cargo test`, `cargo build`, and a short-lived local startup run after the swipe removal

## 1.1.0 - 2026-04-26
- implemented phase 9 first-pass desktop UI with `eframe/egui`, adding Wireless and Shortcut tabs on the main thread while keeping TCP handshake and UDP ingest services running in background threads
- refactored runtime configuration into shared state so monitor selection, pressure sensitivity, and shortcut profile changes can flow into later handshake, stylus mapping, and shortcut resolution without rebuilding core protocol or injector modules
- extended persisted config to store phase 9 pressure sensitivity and shortcut profile data, including custom bindings and trigger-category toggles
- added shortcut profile resolution on top of the immutable default preset so custom bindings and category disables can override later gesture behavior while preserving existing shortcut engine semantics
- added initial UI-driven runtime actions for monitor selection, pressure sensitivity, preset cloning, custom binding disable toggles, and trigger-category enable/disable controls
- verified `cargo check`, `cargo test`, and `cargo build` against the new stage 9 runtime and UI wiring

## 1.0.0 - 2026-04-26
- implemented phase 8 advanced control behaviors for `threePan`, `twoPinch`, `twoRotate`, and stylus `doubleTap` without changing the protocol, session, handshake, or UDP ingest layers
- replaced deferred phase-8 shortcut placeholders with typed advanced actions, pointer-context tracking, and unified cleanup semantics so modifier keys and mouse buttons release correctly on `End`, TTL expiry, runtime shutdown, and session end
- extended the input pipeline shortcut worker to maintain hover and last-in-range screen coordinates from mapped stylus frames so `doubleTap` can right-click at the current hover point or the last valid in-range position
- extended the Windows shortcut executor to send relative mouse move, wheel, right-button down/up, and targeted right-click input through `SendInput` alongside the existing keyboard shortcut path
- added stage 8 tests covering advanced preset mappings, drag/wheel/rotate command emission, hover fallback behavior, pointer-context lifecycle, queue isolation, and cleanup ordering
- verified `cargo fmt`, `cargo check`, `cargo test`, and `cargo build`; short-lived startup validation was also exercised by launching the server and stopping it manually after startup

## 0.9.0 - 2026-04-26
- refactored post-UDP runtime into two internal FIFO workers so stylus pen injection and gesture/shortcut execution no longer block each other on the same sink call chain
- routed stylus frames into a pen worker and gesture plus stylus shortcut pulses into a dedicated shortcut worker while keeping UDP decode, session gating, and wire protocol unchanged
- fanned `SessionEnded` into both workers so pen cancel cleanup and shortcut hold release remain synchronized across the split runtime
- added queue-focused tests covering blocked stylus injection isolation, stylus flag shortcut dispatch on the shortcut queue, dual worker session-end cleanup, stale post-session event suppression, and worker shutdown safety
- verified `cargo fmt`, `cargo check`, `cargo test`, `cargo build`, and short-lived startup verification against the dual-queue runtime

## 0.8.0 - 2026-04-25
- implemented phase 7 shortcut engine for parsed stylus flag triggers and gesture events without changing the UDP ingest layer
- added default shortcut preset handling for tap, swipe, and long-press keyboard mappings plus explicit phase-8 deferral for advanced mouse-driven behaviors
- integrated a Windows keyboard shortcut executor via `SendInput` alongside the existing pen injector, while keeping `shortcut` domain logic separate from `windows_injector`
- updated the input pipeline to route stylus flags, gesture frames, TTL expiry, and session-end cleanup through the new shortcut runtime so held modifiers release correctly
- added phase 7 tests covering hold/update/end behavior, TTL expiry, long-press cadence renewal, trigger deduplication, pipeline dispatch, and session-end hold release
- verified `cargo check`, `cargo build`, and `cargo test`; short-lived startup verification was also run against the updated runtime
- fixed stylus flag tap pulse handling so `squeeze` / `doubleTap` / `twoTap` / `threeTap` / `fourTap` trigger once per flagged packet without requiring a later clear packet from the tablet

## 0.7.0 - 2026-04-25
- implemented phase 6 stylus input pipeline and first Windows pen injection path for the AirSlate PC server
- replaced the phase 5 logging sink with a real stylus pipeline that maps accepted `StylusFrame` events into virtual-screen pen injection commands
- added Windows Synthetic Pointer Injection support for pen input with coordinate, pressure, tilt, cancel, and session-end cleanup handling
- added phase 6 tests covering workspace coordinate mapping, pressure and tilt normalization, cancel/reset behavior, and injector pointer flag translation
- verified `cargo check`, `cargo build`, `cargo test`, and short-lived local startup logging for the updated runtime

## 0.6.0 - 2026-04-25
- implemented phase 5 UDP ingest and event dispatch for the AirSlate PC server on port `48563`
- added shared session state with UDP source IP binding, same-IP cross-port acceptance, and `Session Disconnect` release handling
- added `IncomingEvent` dispatch with log-backed stage 5 verification for stylus, gesture, and session-end events
- updated application startup to run TCP handshake and UDP ingest services concurrently
- added UDP ingest tests covering active-session gating, source-IP mismatch ignore behavior, invalid datagrams, and disconnect release flow

## 0.5.0 - 2026-04-25
- implemented phase 4 TCP handshake service for the AirSlate PC server on port `48562`
- added blocking handshake orchestration that decodes one `Handshake Request`, checks workspace and session state, returns `HandshakeResponse` or `HandshakeError`, and closes the connection
- added handshake tests covering success, unsupported protocol, invalid request, already connected, and no active workspace paths
- integrated phase 4 listener startup into the application runtime

## 0.4.0 - 2026-04-25
- implemented phase 3 in-memory session management for the AirSlate PC server
- added `SessionId`, `ActiveSession`, and `SessionService` with single-active-session enforcement and `Session Disconnect` matching by `sessionId`
- added unit tests covering session creation, concurrent rejection, disconnect ignore/release behavior, and protocol length validation for generated session ids
- added phase 3 startup logging and session-specific application errors

## 0.3.0 - 2026-04-25
- implemented phase 2 workspace service for the AirSlate PC server
- added Windows monitor enumeration with active workspace snapshot and physical pixel resolution lookup
- added selected monitor configuration, workspace error handling, and startup logging for detected displays and the active workspace

## 0.2.0 - 2026-04-25
- implemented phase 1 protocol layer for the AirSlate PC server
- added strict binary models, encoding, decoding, and validation for handshake packets, stylus frames, and gesture frames
- added fixed-size UTF-8 field helpers and protocol-specific error types
- added unit tests covering round-trip encoding, exact packet sizes, golden field offsets, and invalid packet rejection
