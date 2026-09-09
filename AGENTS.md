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
