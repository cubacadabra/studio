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

When `--path` points at a raw project, Studio assembles the runtime inputs in
memory. It recursively expands local `-- @include` directives, expands the
Cubacadabra SDK includes from the sibling `tools` repository, resolves
`manifest.effects.source`, and reads assets directly from the project. It does
not write generated files back into the game directory.

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
- `Play`: enable or pause game input
- `WASD` or arrow keys: move
- `Shift`: sprint
- `Space`: jump
- drag with the left mouse button: orbit the camera
- mouse wheel: zoom
- `Escape`: quit

Studio currently expects the sibling engine repository at `../rust` and the
sibling `../tools` repository at build time for embedded SDK source, matching
the layout used by the other Cubacadabra clients. The desktop host is a single
binary crate for now; platform packaging and future editor services can grow
under `crates/` without making the first window more complex.

Studio connects the running game to the multiplayer Worker over WebSockets.
The backend defaults to the local Worker at `http://127.0.0.1:8787`; set
`CUBACADABRA_BACKEND_URL` to use another backend, for example:

```sh
CUBACADABRA_BACKEND_URL=https://api.cubacadabra.com \
  cargo run --release -- --path /Users/aa/cubacadabra/examples/survival-101
```

The configured HTTP or HTTPS URL is converted to `ws://` or `wss://` for the
game session. Studio reconnects in the background if the Worker is unavailable.
