# Shared engine safety

`../rust` is shared by all four Cubacadabra clients: Studio, iOS, Android, and
the web/WASM client. When changing `../rust` to support a Studio issue:

- Treat iOS, Android, and web/WASM behavior and performance as protected.
- Prefer Studio-only Cargo features, desktop-only modules, and platform-gated
  APIs over changes to shared runtime paths or default features.
- Do not add per-frame work, state, dependencies, or backend initialization to
  mobile or web paths unless the client change is intentional and verified.
- Preserve the existing iOS Metal path, Android backend feature path, and web
  renderer/WASM build path.
- Verify the normal engine build, Studio desktop build, Android feature set,
  web/WASM feature set, and relevant engine tests. If a target cannot be built
  locally, report the toolchain limitation explicitly.

# Studio shell logo

Preserve the Studio logo in the top bar. It is embedded from
`assets/logo.png`, loaded into an egui texture during `StudioShell` setup, and
rendered as the 20×20 image in `src/shell.rs`. Do not replace it with a generic
painted icon or remove its texture initialization when changing shell layout
or styling.

# Cross-platform desktop integration

Use a shared Rust Studio UI with a thin native integration layer for each
desktop platform. Cubacadabra Studio should look and operate like the same
product on macOS, Windows, and Linux while respecting the conventions each OS
owns. Do not build three separate editor interfaces.

## Keep these shared on every platform

- Keep the scene hierarchy, viewport, inspector, asset browser, materials UI,
  testing UI, workspace tabs, Play controls, panels, docking, styling, and
  editor interaction model in the shared Rust/egui shell.
- Keep `World`, `Assets`, `Materials`, and `Test` as in-window workspace tabs.
  They are product navigation, not operating-system application menus.
- Keep command behavior and state transitions shared. Platform menu items and
  shortcuts should dispatch the same `StudioCommand` values used by in-window
  controls instead of duplicating business logic.
- Preserve names and command locations closely enough that documentation,
  screenshots, support instructions, and user workflows remain recognizable
  across platforms.
- Do not replace custom editor surfaces with AppKit, WinUI/Win32, or GTK
  widgets merely to make one build appear more native.

## Menu placement

- On macOS, put true application menus in the system menu bar with AppKit:
  `Cubacadabra Studio`, `File`, `Edit`, `Window`, and eventually `Help`.
  Do not duplicate those menus inside the Studio window.
- Keep the logo, workspace tabs, project context, Play control, and live status
  in the shared in-window toolbar on macOS; these are Studio controls rather
  than macOS application menus.
- On Windows and Linux, keep `File`, `Edit`, and `Window` in the application
  window alongside the shared toolbar. Do not leave a blank row where a global
  menu would be on macOS.
- Use native shortcut conventions and labels: Command shortcuts and macOS menu
  roles on macOS; Ctrl-based shortcuts in the Windows/Linux in-window menus.
- Use native responder-chain actions for standard operations such as Close,
  Undo, Redo, Cut, Copy, Paste, Select All, Minimize, Zoom, Hide, and Quit when
  the platform supports them. Keep unavailable actions disabled rather than
  presenting fake behavior.
- On macOS, place Settings under the application menu with Command-comma,
  About under the application menu, and window commands under Window.

## Native platform boundary

Use native APIs where users expect the operating system to own the experience:

- macOS: AppKit menus, About panel, application/Dock icon, Open and Save
  panels, represented document URL and edited state, Finder Open With and file
  associations, Finder drag and drop, pasteboard integration, fullscreen,
  standard cursors and contextual menus, save-before-close alerts, application
  lifecycle/reopen behavior, recent documents, and system appearance,
  accessibility, and reduced-motion settings.
- Windows: native file dialogs, taskbar integration, file associations,
  Explorer drag and drop, clipboard, window lifecycle, and standard system
  alerts where appropriate. Keep the main menu inside the Studio window.
- Linux: XDG desktop files and MIME associations, portal-backed file dialogs,
  clipboard and drag-and-drop integration, and normal desktop/window-manager
  behavior. Keep the main menu inside the Studio window.

Platform integration should stop at the OS boundary. Native dialogs, menus,
window behavior, and desktop services should hand results back to shared Rust
commands and state.

## macOS implementation rules

- Rust may call AppKit directly through target-gated `objc2` crates. A Swift,
  Objective-C, or SwiftUI wrapper is not required.
- Keep AppKit code in macOS-only modules and dependencies under
  `target.'cfg(target_os = "macos")'.dependencies`; it must not affect Windows,
  Linux, mobile, or web builds.
- Preserve Winit's normal application lifecycle and install native menus only
  after Winit has initialized `NSApplication` and its default application menu.
- The standard About panel must receive `assets/logo.png` explicitly through
  `NSAboutPanelOptionApplicationIcon`; setting only the Dock application icon
  is not sufficient and previously left the generic icon in About.
- Keep the native application icon and About logo sourced from the same
  `assets/logo.png` used by the shared Studio toolbar.
- Package `.app` builds with correct `Info.plist` metadata, including
  `CFBundleName`, `CFBundleShortVersionString`, `CFBundleVersion`, copyright,
  application icon, and future document-type declarations. Runtime fallbacks
  should still behave sensibly for unbundled `cargo run` builds.
- Use represented URLs, edited-state indicators, reopen handling, and standard
  close/quit semantics when document editing becomes functional. Closing the
  last window and quitting the application are distinct macOS lifecycle events.
- Consider native titlebar or `NSToolbar` integration only when it improves
  window behavior without forking the shared editor UI or hiding useful Studio
  controls.

## Verification

- Verify macOS menu titles, placement, shortcuts, enabled states, About logo,
  and command dispatch in a running application, not only with a compile check.
- Verify that macOS does not render duplicate in-window application menus and
  that Windows/Linux still render the complete in-window menu row.
- Run Studio tests and formatting checks, and compile every available desktop
  target after platform-layer changes. If a Windows or Linux target toolchain
  is unavailable locally, report that limitation explicitly.
- Keep platform code out of `../rust` unless engine support is genuinely
  required; the shared-engine safety rules above still apply.
