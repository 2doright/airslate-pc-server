# CHANGELOG

## Unreleased

## 1.6.4-beta.5 - 2026-08-26

- show a clear message when wired mode is off so the device can be connected to and used with the
  computer normally
- make smart pen-down calibration respond after 200 ms and tolerate a slightly wider stable hover
  range

## 1.6.4-beta.4 - 2026-08-26

- added a default-on switch to the wired connection card; turning it off stops USB scanning and
  accessory negotiation, releases a device already switched into accessory mode, and is rejected
  while a wired session is connected so tablet file transfer remains available
- shorten the smart pen-down calibration rolling window from 300 ms to 250 ms
- keep stable MSI versions as plain major.minor.patch values while continuing to map beta.N to the
  fourth numeric version field

## 1.6.4-beta.3 - 2026-08-26

- measure smart pen-down calibration dwell time with the PC's monotonic receive clock so a
  stationary Hover-to-Down pause is counted even when the device sends no intervening Move frames

## 1.6.4-beta.2 - 2026-08-26

- evaluate smart pen-down calibration in the latest 300 ms window ending at Down, including the
  quiet interval after the final Hover; use that final Hover as the correction anchor, require the
  recent Hover trajectory to remain within 96 logical units of it, and remove the sample-count gate
- streamline the setup and feature guide so the most common connection steps are easier to scan
- preserve beta SemVer versions while mapping Windows MSI ProductVersion to a numeric build field
- generate the repository's native Star History SVG every six hours so the README chart no longer depends on the hosted chart endpoint, and skip rendering, committing, and pushing when the underlying GitHub Star event history is unchanged

## 1.6.3 - 2026-07-26

- run the complete Rust test suite on both Windows and macOS before packaging a tagged release
- added default-on smart pen-down calibration that recognizes a locally anchored hover, removes
  the HarmonyOS hover-to-touch coordinate discontinuity from the complete stroke, and exposes the
  behavior in General settings
- publish semantic-version tags with a prerelease suffix as GitHub Pre-releases without replacing
  the stable Windows updater manifest

## 1.6.2 - 2026-07-22

- added an Advanced four-level Hover trajectory retention slider: the default preserves every hover point, the two middle levels merge points within 4 ms or 8 ms, and the highest level keeps only the latest queued hover point
- added two-finger pan to the shortcut list and made the radial menu an optional special action for two- and three-finger pan gestures

## 1.6.1 - 2026-07-18

- added a persisted General setting for the initial Harmony USB interface signature (class/subclass/protocol), defaulting to `FF/50/01` and applying to the live USB discovery scan
- added a live USB descriptor scanner beside the General interface setting; it keeps the initial interface visible alongside the live re-enumerated interface, includes USB product/manufacturer names when provided, and only lists devices inserted after the scanner opens
- narrowed the USB scanner to devices identified by the `HDC Device` product name, requires both the initial and re-enumerated interface before showing the original identification interface, and simplified the card and refresh controls; the General input now shows the default `FF/50/01` value as its blank-state hint and clears incomplete or invalid values automatically, while the wired connection card explains where to configure the device interface

## 1.6.0 - 2026-07-17

- moved the Windows inbox WinUSB helper into the USB accessory service and removed the standalone dry-run binary, so macOS universal Tauri packaging no longer expects a Windows-only executable

- aligned the USB status and UI documentation with the current authorization-before-handshake flow and compact user-facing panel
- keep the wired panel in `等待授权` through Bulk opening and USB_READY submission; show `正在连接` only after the tablet's formal handshake request is received
- keep the selected settings tab blue while hovered and give unselected tabs a visible neutral hover state
- simplified the wired USB connection card to match the compact LAN IPv4 layout, placed it beside IPv4, moved the half-width monitor selector to the next row, and moved refresh into the same compact header icon; technical descriptor facts are no longer shown in the main connection view
- removed the duplicate wired-session disconnect action from the card; the shared top-right session control remains the single disconnect entry point
- vertically centered the wired connection status content to match the adjacent IPv4 card
- made the authorization state use the visible `等待授权` label in the card body and kept the initial no-device state as `未连接`
- prevented a bootstrap refresh from overwriting a newer live USB status event, so `等待授权` is not replaced by a stale `未连接`/`等待连接` snapshot
- report the post-`USB_READY` authorization wait as `等待授权`, and guide a failed startup connection to replug the USB cable
- keep a failed initial USB candidate in the actionable cable-replug state until physical re-enumeration or an explicit retry, avoiding repeated `等待授权`/`连接失败` flashes
- make the waiting USB panel actionable when the known tablet file-transfer interface is visible:
  ask the user to start and authorize the AirSlate wired connection, then retry, without treating
  the file-transfer endpoints as an accessory candidate or raw session
- distinguish the real tablet-file-transfer waiting state from an unrecognized USB inventory in
  the status event and discovery log, including the nusb Windows backend and reported driver
- added a production wired-connection panel to the desktop UI, backed by the USB service's
  real status snapshot and descriptor facts; the panel exposes authorization/handshake/connected/
  error states, selected interface and Bulk endpoints, actionable retry, and the existing shared
  session disconnect path without claiming success from enumeration alone
- treat nusb 0.2.4/WinUSB submit-time `ERROR_BAD_COMMAND` (`TransferError::Disconnected`) as
  recoverable only before any USB_READY byte was sent, while the exact accessory LocationPath is
  still present; reopen descriptor-selected Bulk endpoints with a six-attempt bound and keep real
  disappearance, partial-write, and driver failures terminal
- log low-noise discovery state transitions, visible USB descriptor summaries, and the explicit
  post-session cleanup return to the initial accessory-compatible scan; add reconnect lifecycle coverage proving a
  released USB connection does not block the next connection id
- consume a complete 72-byte pre-handshake request before handling a nusb transfer error, so a
  full request reported together with STALL cannot enter a blocking pipe reset; add stage logs from
  request parsing through response flush without logging client data
- add low-noise pre-handshake USB Bulk IN diagnostics for every non-empty completion, buffered
  framing progress, timeout runs, STALL detection, and successful pipe reset without logging payloads
- send the USB-only 8-byte ASLT `USB_READY` bootstrap before waiting for Harmony's formal
  handshake; pre-authorization Bulk OUT timeout/STALL is retried with bounded backoff and nusb
  `clear_halt`, while partial completions resume from the unsent byte and real I/O failures remain terminal
- recover a pre-handshake USB Bulk IN STALL with nusb `Endpoint::clear_halt` (WinUSB
  `ResetPipe`) so granting HarmonyOS accessory permission after PC enumeration cannot strand the
  tablet waiting for a response; an active-session STALL remains a terminal I/O failure
- promoted the verified USBAccessory path into a formal wired transport that shares handshake,
  session cleanup, stylus/gesture dispatch, shortcut processing, and Windows input injection with wireless
- replaced the ASUB PING/PONG probe with strict ASLT stream framing for handshake, disconnect, stylus,
  and gesture packets, including short-read/coalesced-read/short-write handling and terminal protocol errors
- keep the Harmony accessory selection identity fixed at manufacturer `AirSlate` and product
  `AirSlate PC Server`
- identify the re-enumerated data function by its exact Windows LocationPath and descriptor-selected
  configuration/interface/alternate/Bulk endpoints instead of fixed VID/PID or endpoint addresses
- identify the initial accessory-compatible function by its `FF/50/01` interface signature, allowing
  a re-enumerated accessory interface even when Windows uses a different device display name
- add an explicit, LocationPath-bound helper that can select the Microsoft inbox `winusb.inf`
  driver for one unique unclaimed Accessory devnode; the formal service invokes it through one UAC
- build, enumerate, and select the inbox WinUSB node from the target device's associated driver
  list instead of passing a global class-list node to `DiInstallDevice`

## 1.5.6 - 2026-07-17

- fixed Tauri and Cargo startup selection so the MSI launches `airslate_pc_server.exe` instead of a diagnostic CLI binary
- fixed the native radial overlay service lifetime so the radial menu and hold indicator remain available after application initialization

## 1.5.5 - 2026-07-15

- completed macOS synthetic tablet-point events by carrying the client's logical pen position as
  full-resolution tablet-space X/Y alongside the mapped global screen location

## 1.5.4 - 2026-07-14

- fixed a macOS startup regression that terminated the desktop shell before it opened when
  Accessibility post-event permission had not yet been granted; permission is now requested and
  enforced at the actual input-injection boundary

## 1.5.3 - 2026-07-14

- replaced the unusable PC-side AppGallery review link with guidance to leave a five-star review
  from Huawei AppGallery on a HarmonyOS device
- fixed macOS stylus injection to require active Accessibility post-event permission instead of
  reporting successful injection when macOS rejects synthetic input
- fixed macOS tablet proximity events to emit one enter and one leave per in-range lifecycle
  instead of repeatedly emitting enter events without a matching leave
- disabled dev and test incremental compilation caches so reusable dependency artifacts remain
  available without allowing `target` to accumulate per-compilation incremental state

## 1.5.2 - 2026-07-13

- fixed Windows pen mapping on mixed-resolution multi-monitor layouts whose virtual desktop has a
  negative left or top origin, so the full selected display is reachable without a constant offset

## 1.5.1 - 2026-07-12
- added opt-in Advanced input strategies for latest-sample contact prioritization and new-stroke preemption, with a persistent 0–100 ms backlog-tolerance control
- added main-page session status controls, including a clear waiting state, local disconnect action, shared session cleanup, and live Tauri status events
- added an independent LAN IPv4 refresh action and bound handshake sessions to the peer's IPv4 address
- added opt-in `AIRSLATE_INPUT_METRICS=1` pipeline measurements for sequence gaps, queue depth, queue wait, and injection duration
- replaced lossy contact-move backlog compaction with an ordered lossless queue, preserving every accepted in-contact Move sample during high-frequency multi-stroke bursts while still coalescing only non-contact hover moves
- moved normal stylus coordinate and pressure mapping out of the pen worker's duplicate hot path and reduced shortcut-context work for contact moves
- replaced the Windows pen frame-id mutex with atomic sequencing and corrected synthetic pen history metadata
- removed the window-size flash when reopening the control panel by restoring persisted geometry before recreating the window
- refreshed the waiting and main-page connection controls with the cc-switch-inspired orange treatment, circular add-action shape, elevated shadow, and color-only hover transition
- changed the radial-menu inner-ring toggle to a circular switch while preserving the nine-grid editor layout
- added explicit red fidelity warnings for opt-in lossy input strategies and streamlined the About page with Update, Issues, and Discussions actions
- changed the default window size to 2064 × 1232 and the minimum window size to 960 × 540 (16:9)
- the new input strategies are opt-in; lossless input remains the default
- pipeline metrics are disabled unless `AIRSLATE_INPUT_METRICS=1` is set

## 1.5.0 - 2026-07-12
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
