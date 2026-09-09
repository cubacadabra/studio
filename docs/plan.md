Studio should become a visual production workbench built around the real Cubacadabra runtime—not a conventional IDE
  and not a simplified Blender clone.

  Its defining loop should be:

  select or replace an asset
          ↓
  see it in the real renderer
          ↓
  adjust it directly
          ↓
  validate across target conditions
          ↓
  save a clean repository change

  ## The window model

  I would use five concepts:

   Concept            Purpose
  ━━━━━━━━━━━━━━━━━  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
   Project Window     One native OS window per open game project
  ─────────────────  ─────────────────────────────────────────────────────────────────────
   Workspace          A task-oriented layout: World, Assets, Materials, Animation, Test
  ─────────────────  ─────────────────────────────────────────────────────────────────────
   Area               A resizable tile within a workspace
  ─────────────────  ─────────────────────────────────────────────────────────────────────
   Editor             The content shown in an area: viewport, scene tree, inspector, etc.
  ─────────────────  ─────────────────────────────────────────────────────────────────────
   Runtime Session    An independently running game client used by a viewport

  This borrows Blender’s strongest idea: non-overlapping areas containing specialized editors, grouped into
  task-oriented workspaces. Editors can be maximized or duplicated into another OS window when needed. Blender’s UI
  documentation (https://docs.blender.org/manual/en/latest/interface/window_system/areas.html) and design paradigms
  (https://developer.blender.org/docs/features/interface/human_interface_guidelines/paradigms/) describe that model
  well.

  But I would make Cubacadabra substantially simpler:

  - Ship carefully designed workspace presets.
  - Let artists resize, collapse, maximize, and later move areas.
  - Do not begin with unlimited splitting, docking, floating, and configuration.
  - Remember layouts per user and project.
  - Provide “Reset Workspace” whenever customization arrives.

  ## The default World workspace

  ┌──────────────────────────────────────────────────────────────────────┐
  │ File  Edit  Window   World  Assets  Materials  Test     ▶ Play  ● Live│
  ├────────────────┬─────────────────────────────────────┬───────────────┤
  │ SCENE          │ VIEWPORT                            │ INSPECTOR     │
  │                │                                     │               │
  │ World          │      Actual Cubacadabra renderer    │ Tree_014      │
  │ ├ Environment  │                                     │ Transform     │
  │ ├ Village      │                                     │ Position      │
  │ │ ├ House 01   │                                     │ Rotation      │
  │ │ └ Well       │                                     │ Scale         │
  │ └ Forest       │                                     │               │
  │   ├ Tree 013   │                                     │ Material      │
  │   └ Tree 014   │                                     │ Collision     │
  ├────────────────┴─────────────────────────────────────┴───────────────┤
  │ ASSETS   [All] [Images] [Materials] [Characters]       Search…       │
  │ [grass] [wood] [campfire] [tree] [castle]                            │
  ├──────────────────────────────────────────────────────────────────────┤
  │ ✓ Reloaded tree.glb   1 warning   Metal · High quality   60 fps      │
  └──────────────────────────────────────────────────────────────────────┘

  The viewport remains visually dominant. Everything else explains, finds, or modifies what is in it.

  The surrounding interface should be a quiet, dark-neutral production frame. The game supplies the color. Use compact
  typography, thin separators, one selection accent, restrained animation, and no decorative cards.

  ## Workspaces

  ### World

  For placement and scene composition:

  - Scene hierarchy
  - Runtime viewport
  - Selection inspector
  - Asset shelf
  - Move, rotate, scale, duplicate, group, hide, lock
  - Day/night and device-quality preview
  - Maximize viewport without destroying the layout

  ### Assets

  For intake and replacement:

  Asset browser | Large preview | Import settings and validation

  Dropping a file onto an existing asset should:

  1. Stage and inspect the file.
  2. Validate it.
  3. Replace the source only if usable.
  4. Hot reload immediately.
  5. Show Replaced tree.glb · Undo.

  Dropping onto empty viewport space imports a new asset. Dropping onto an object replaces or reassigns that object’s
  asset, depending on context.

  The browser should show thumbnails, type, validation state, usage count, and modified status. It should not expose
  filesystem complexity unless the artist asks to reveal a file.

  ### Materials

  For subjective visual tuning:

  - Large viewport or turntable
  - Selected material inspector
  - Texture slots
  - Roughness, tint, normal strength, tiling, transparency
  - Lighting-environment selector
  - Before/after comparison
  - Preview on asset, character, or current world

  No node graph initially. Sliders and presets cover the important last-mile decisions with far less complexity.

  ### Animation

  Eventually:

  - Character viewport
  - Clip browser
  - Timeline
  - Playback and speed controls
  - Body/outfit variants
  - Validation results linked to exact frames

  Clicking an intersection warning should jump directly to the affected animation and frame.

  ### Test

  ┌───────────────────────────────┬───────────────────────────────┐
  │ Player 1                      │ Player 2                      │
  ├───────────────────────────────┼───────────────────────────────┤
  │ Player 3                      │ Player 4                      │
  ├───────────────────────────────┴───────────────────────────────┤
  │ Sessions | State | Network | Logs | Performance               │
  └───────────────────────────────────────────────────────────────┘

  Each tile is an independent runtime session. Presets should include one, two, and four players, latency, packet loss,
  reconnect, and join-in-progress.

  Any client viewport can be popped into a separate native window for another monitor. That is a legitimate second
  window; a permanently floating material-properties palette is not.

  ## Context is what keeps this coherent

  Every editor should follow one of four shared contexts:

  - Active project
  - Active world
  - Current selection
  - Active runtime session

  Most areas follow global context automatically. An artist can pin an inspector or viewport when comparing two assets.

  This creates powerful behavior without extra UI:

  - Selecting an object in the scene tree highlights it in the viewport.
  - Selecting a material updates the inspector.
  - Clicking a validation warning selects the object and frames the problem.
  - Choosing Player 2 makes State, Network, and Logs follow Player 2.
  - A pinned viewport keeps showing the old asset for comparison.

  ## AI’s place

  AI should be available through Command-K, selection context menus, and a collapsible Actions editor—not occupy a
  permanent chat column.

  Examples:

  - “Make the selected stone warmer and less shiny.”
  - “Replace these six trees with three silhouette variants.”
  - “Find objects using this texture.”
  - “Fix the two mobile validation warnings.”

  AI changes should enter the same command, preview, undo, validation, and hot-reload system as manual changes. That
  gives Studio one editing model instead of separate “AI edits” and “human edits.”

  ## What the current code needs architecturally

  The present implementation tightly combines one Engine, one network client, one Window, one Renderer, and all input
  state inside StudioApp (src/main.rs:66). It creates one renderer directly from the entire native window in
  create_window (src/main.rs:159), and the event handler currently ignores WindowId because every event belongs to that
  single window in window_event (src/main.rs:893).

  The future structure should become:

  StudioApplication
  ├── ProjectSession
  │   ├── ProjectStore
  │   ├── AssetIndex
  │   ├── FileWatcher
  │   ├── Selection
  │   ├── UndoHistory
  │   └── ValidationService
  ├── WindowManager
  │   └── ProjectWindow(s)
  │       ├── Workspace
  │       └── Layout tree of Editors
  └── RuntimeManager
      └── RuntimeSession(s)
          ├── Engine
          ├── Network client
          ├── Input state
          └── Render target

  The most important renderer change is moving from “engine renderer owns the entire window surface” to “runtime session
  renders into an offscreen viewport texture that Studio composites into an area.” Any shared-engine API for this should
  be desktop-only and feature-gated so iOS, Android, and WASM remain untouched.

  For the shell, egui is a strong match with the existing Rust/winit/wgpu architecture. Specifically, egui-wgpu 0.35
  aligns with the project’s current wgpu 29 and winit 0.30 versions; I would pin that combination rather than upgrading
  the shared engine merely to follow the newest GUI release. Egui has official winit and wgpu integrations and supports
  additional native viewports. Official egui architecture (https://github.com/emilk/egui/blob/main/ARCHITECTURE.md)

  I would not introduce a third-party docking framework initially. Fixed workspace layouts with reusable editor
  interfaces are the safer foundation.

  ## The first practical release

  The repository currently imports package images into an atlas in load_image_atlas (src/main.rs:949); it does not yet
  have a general .glb asset pipeline. Therefore the first release should concentrate on functionality the engine
  genuinely supports:

  1. Introduce the application shell and embedded runtime viewport.
  2. Add World, Assets, and Test workspace presets.
  3. Add file watching with last-known-good hot reload.
  4. Support drag-and-drop replacement of image assets.
  5. Add palette, image-material, and block-transform inspection.
  6. Add atomic saves, undo, external-change detection, and validation feedback.
  7. Add two independent test sessions.
  8. Then add imported meshes, material parameters, and scene gizmos as engine capabilities mature.
  9. Add arbitrary docking and pop-out editors only after the editor set stabilizes.

  The first convincing product milestone is:

  > Open a project, drag a new texture over an existing asset, see every live viewport update, adjust its material and
  > placement, test it as two players, then undo—all without seeing JSON or Luau.

  That would already turn Studio from a game host into a real artist-facing production application.
