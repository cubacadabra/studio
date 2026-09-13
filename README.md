# Cubacadabra Studio

Cubacadabra Studio is the native desktop host for a local Cubacadabra game
package. It opens the same Rust engine used by the iOS, Android, and web
clients, then forwards desktop input to it.

The first visual-workbench shell places the live game renderer inside a native
desktop workspace. Its World, Assets, Materials, and Test layouts are an early
interaction preview; project editing and multi-session testing are not wired up
yet. Local package loading, keyboard movement, mouse camera control, wheel zoom,
and package image assets continue to use the shared engine.

## Run a game

cargo run --release -vv -- --path /Users/aa/cubacadabra/examples/survival-101

Studio accepts either a built package or a raw game project.

A built package contains:

```text
assets/
game.luau
manifest.json
package.json
```

A raw project contains:

```text
assets/
manifest.json
src/main.luau
```

When `--path` points at a raw project, Studio invokes the installed
`cubacadabra build-game` command and loads its package from a temporary
Studio-owned directory. This keeps Studio's raw-project behavior aligned with
the CLI's module bundling, SDK resolution, manifest validation, and effects
handling. The temporary package is removed when Studio exits.

From this repository, run:

```sh
cargo run --release -- --path /Users/aa/test-for-studio
```

The raw source project works the same way:

```sh
cargo run --release -- --path /Users/aa/cubacadabra/examples/the-wild-west
```

The resulting binary can be invoked as:

```sh
./target/release/studio --path /Users/aa/test-for-studio
```

Controls:

- `World`, `Assets`, `Materials`, and `Test`: switch workspace previews
- `Connect ChatGPT`: connect the user's ChatGPT subscription through Codex App Server
- `Play`: enable or pause game input
- `WASD` or arrow keys: move
- `Shift`: sprint
- `Space`: jump
- drag with the left mouse button: orbit the camera
- mouse wheel: zoom
- `Escape`: quit

## ChatGPT connection

Studio launches `codex app-server` over its default stdio transport, reads any
cached ChatGPT account, and opens the browser flow when the user selects
`Connect ChatGPT`. Codex owns and refreshes the ChatGPT credentials; Studio
only keeps the account email and plan label in memory for connection status.

Packaged builds should place the pinned Codex executable next to the Studio
executable on Windows and Linux, or in `Contents/Resources/codex` on macOS.
Development builds fall back to `codex` on `PATH`. Set
`CUBACADABRA_CODEX_PATH` to test a specific executable:

```sh
CUBACADABRA_CODEX_PATH=/absolute/path/to/codex \
  cargo run --release -- --path /Users/aa/test-for-studio
```

Studio currently expects the sibling engine repository at `../rust` at build
time. For local source-project fallback, it can use the sibling `../tools`
repository when the `cubacadabra` command is not installed. The desktop host is
a single binary crate for now; platform packaging and future editor services
can grow under `crates/` without making the first window more complex.

Studio connects the running game to the multiplayer Worker over WebSockets.
The backend defaults to the local Worker at `http://127.0.0.1:8787`; set
`CUBACADABRA_BACKEND_URL` to use another backend, for example:

```sh
CUBACADABRA_BACKEND_URL=https://api.cubacadabra.com \
  cargo run --release -- --path /Users/aa/cubacadabra/examples/survival-101
```

The configured HTTP or HTTPS URL is converted to `ws://` or `wss://` for the
game session. Studio reconnects in the background if the Worker is unavailable.

### Licensing

Copyright (C) 2026 Andrew Arrow

Licensed under the GNU General Public License v3.0 or later.
See [LICENSE](LICENSE).
