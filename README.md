# Cubacadabra Studio

Cubacadabra Studio is the native desktop host for a local Cubacadabra game
package. It opens the same Rust engine used by the iOS, Android, and web
clients, then forwards desktop input to it.

The first version intentionally keeps the host small: one game window, local
package loading, keyboard movement, mouse camera control, wheel zoom, and
package image assets.

## Run a game

The game directory should contain the built package files:

```text
assets/
game.luau
manifest.json
package.json
```

From this repository, run:

```sh
cargo run --release -- --path /Users/aa/test-for-studio
```

The resulting binary can be invoked as:

```sh
./target/release/studio --path /Users/aa/test-for-studio
```

Controls:

- `WASD` or arrow keys: move
- `Shift`: sprint
- `Space`: jump
- drag with the left mouse button: orbit the camera
- mouse wheel: zoom
- `Escape`: quit

Studio currently expects the sibling engine repository at `../rust`, matching
the layout used by the other Cubacadabra clients. The desktop host is a single
binary crate for now; platform packaging and future editor services can grow
under `crates/` without making the first window more complex.
