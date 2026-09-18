# Cubacadabra Studio

Cubacadabra Studio is the native desktop host for a local Cubacadabra game
package. It opens the same Rust engine used by the iOS, Android, and web
clients, then forwards desktop input to it.

The first visual-workbench shell places the live game renderer inside a native
desktop workspace. Raw source projects expose a focused scene-authoring loop:
select a platform, adjust its position or size, duplicate or delete it, save,
and rebuild the preview without losing the Studio session. Luau is authored
through the embedded Codex integration rather than a Studio code editor.
Local package loading, keyboard movement, mouse camera control, wheel zoom, and
package image assets continue to use the shared engine.

## Run a game

cargo run --release -vv -- --path /Users/aa/cubacadabra/examples/survival-101

Studio accepts either a built package or a raw game project.

Launching Studio without `--path` opens a project chooser with recent projects
and **Open Project…** / **New Project…** actions. Projects opened successfully
are kept in the recent-project list for the next launch.

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

Raw projects may also include creator-only `studio.json` preview preferences:

```json
{
  "previewWorld": "reference-world",
  "reviewCamera": "showcase"
}
```

`previewWorld` must name a world in the authored manifest. Studio applies it to
the temporary runtime manifest without changing the package's shipping launch
destination. `reviewCamera` accepts `gameplay`, `overview`, or `showcase`.

When `--path` points at a raw project, Studio calls the shared Rust
`cubacadabra-builder` library in-process. The package is loaded from a
temporary Studio-owned directory and removed when Studio exits. The native
`cubacadabra` CLI in `../tools` uses that same library for terminal builds.

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

- `World`, `Files`, and `Morphs`: switch the available workspaces
- `ChatGPT · <plan>`: open the in-window Codex chat for the current project
- `Play` / `Stop`: start or stop the current preview; starting again resets it
- `Rebuild & Play`: save the source scene, rebuild the current project, and start
  a fresh preview
- `Restart`: reset the current project without reopening the editor
- `WASD` or arrow keys: move
- `Shift`: sprint
- `Space`: jump
- drag with the left mouse button: orbit the camera
- mouse wheel: zoom
- `Overview` / `Showcase`: inspect the world with left-drag orbit,
  right- or middle-drag pan, and wheel/pinch zoom, including while stopped
- click the current review preset again to reframe the world
- `Close Window`: close Studio, with a save/discard prompt for dirty projects

## Native preview verification

Debug builds support an opt-in, app-owned GPU framebuffer probe. It renders
the production scene and editor overlay without requiring OS screen-recording
permission or an available on-screen drawable, writes PNGs, and exits:

```sh
CUBA_STUDIO_PROBE_DIR=/tmp/maze-gameplay \
  cargo run -- --path ../examples/maze-101

CUBA_STUDIO_PROBE_DIR=/tmp/maze-review CUBA_STUDIO_PROBE_REVIEW=1 \
  CUBA_STUDIO_PROBE_WORLD=maze-world-reference \
  cargo run -- --path ../examples/maze-101
```

The review probe exercises the native input handlers for stopped orbit/pan,
zoom, and preset reset, asserting that the gameplay camera stays unchanged.
It does not test OS event delivery, physical trackpad gestures, or presentation
to the window surface. The probe and texture readback support are omitted from
release builds.

## ChatGPT connection

Studio launches `codex app-server` over its default stdio transport, reads any
cached ChatGPT account, and opens the browser flow when the user selects
`Connect ChatGPT`. Once connected, select the account label in the top bar to
open the in-window Codex chat. Each conversation and turn uses the currently
open project as its working directory, can write only inside that project, and
has network access disabled by default. When a turn completes, Studio rereads
the project files, rebuilds the preview, and starts the game only if that build
succeeds; a failed build leaves the last working preview running and shows the
build error. Studio reports source changes without opening source files, lists
any changed non-source files, and can undo that turn when those files have not
been edited again. Codex owns and refreshes the ChatGPT credentials; Studio
only keeps the account email and plan label in memory for connection status.

Packaged builds should place the pinned Codex executable next to the Studio
executable on Windows and Linux, or in `Contents/Resources/codex` on macOS.
Development builds fall back to `codex` on `PATH`. Set
`CUBACADABRA_CODEX_PATH` to test a specific executable:

```sh
CUBACADABRA_CODEX_PATH=/absolute/path/to/codex \
  cargo run --release -- --path /Users/aa/test-for-studio
```

Studio currently expects the sibling engine repository at `../rust` and the
creator-toolchain repository at `../tools` at build time. Release artifacts
contain native Rust binaries only; opening and rebuilding raw projects does
not require Python or a separately installed CLI. Codex remains an optional
integration and must be packaged separately or configured with
`CUBACADABRA_CODEX_PATH`.

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
