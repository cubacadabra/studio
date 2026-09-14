use cubacadabra_client::{ClientAction, ClientSession, native::Renderer};
use image::{GenericImage, RgbaImage, imageops::FilterType};
use log::{debug, error, info, warn};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    env,
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
    sync::mpsc,
    thread,
    time::Instant,
};
mod codex;
mod game_creator;
#[cfg(target_os = "macos")]
mod macos;
mod morph_application;
mod morphs;
mod network;
mod shell;
mod wardrobe;
#[cfg(test)]
mod wardrobe_tests;
use codex::{CodexClient, CodexEvent};
use cubacadabra_morphs::decode_morph_pack;
use morphs::{
    MorphGlbPreviewMesh, compile_source_morph_pack, decode_source_glb_preview,
    decode_source_glb_preview_node, encode_morph_thumbnail_png, inspect_source_glb_structure,
    inspect_source_sidecar, is_morph_draft_json, parse_morph_draft_json, source_manifest_asset,
    source_manifest_geometry_file,
};
use network::{BackendClient, BackendEvent};
use shell::{PreparedShell, SceneEditRequest, StudioShell};
#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowAttributes},
};

const DEFAULT_WINDOW_WIDTH: f64 = 1280.0;
const DEFAULT_WINDOW_HEIGHT: f64 = 800.0;
const MAX_ATLAS_DIMENSION: u32 = 2048;
const MAX_ATLAS_IMAGE_DIMENSION: u32 = 1020;
const ATLAS_PADDING: u32 = 2;
const STANDALONE_PREVIEW_ROOT: &str = "Morph Preview";
const STANDALONE_PREVIEW_MANIFEST: &str = r#"{
  "id": "studio-morph-preview",
  "version": "0.0.0",
  "sdkVersion": "0.3.0",
  "package": {
    "formatVersion": 3,
    "entry": "game.luau"
  },
  "displayName": "Morph Preview",
  "lobby": false,
  "startWorld": "lobby",
  "launch": {
    "destinationWorld": "lobby",
    "authoritative": false
  },
  "world": {
    "groundSize": 12,
    "gridSize": 0,
    "gridDivisions": 0,
    "spawn": [0, 0, 0],
    "showSpawnPad": false
  }
}"#;
const STANDALONE_PREVIEW_SCRIPT: &str = "return {}";

#[derive(Debug)]
struct StudioError(String);

impl fmt::Display for StudioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for StudioError {}

#[derive(Debug)]
struct ImageAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    regions: std::collections::BTreeMap<String, [f32; 4]>,
}

struct GameSources {
    project_root: PathBuf,
    root: PathBuf,
    authored_manifest_source: String,
    manifest_source: String,
    script_source: String,
    standalone_preview: bool,
    temporary_package: Option<PathBuf>,
}

struct BackgroundProjectLoad {
    sources: GameSources,
    image_atlas: Option<ImageAtlas>,
    local_morph_catalog: Option<LocalMorphCatalog>,
}

struct PreparedProjectLoad {
    background: BackgroundProjectLoad,
    client: ClientSession,
    network: BackendClient,
}

enum ProjectLoadEvent {
    Progress(f32),
    Finished(Result<BackgroundProjectLoad, String>),
}

struct PendingProjectLoad {
    project: PathBuf,
    preserve_editor: bool,
    codex_rebuild: bool,
    receiver: mpsc::Receiver<ProjectLoadEvent>,
}

type ProjectFileSnapshot = BTreeMap<PathBuf, Vec<u8>>;

#[derive(Clone)]
struct CodexFileChange {
    relative_path: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

struct LocalMorphCatalog {
    catalog: cubacadabra_morphs::MorphCatalog,
    packs: BTreeMap<cubacadabra_morphs::MorphAssetId, Vec<u8>>,
    thumbnails: BTreeMap<String, Vec<u8>>,
    initial_preset: Option<cubacadabra_morphs::MorphAssetId>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalMorphCatalogFile {
    #[allow(dead_code)]
    schema_version: u16,
    #[serde(default)]
    assets: Vec<LocalMorphAsset>,
    #[serde(default)]
    builtins: Option<String>,
    #[serde(default)]
    exclude_builtin_kinds: Vec<cubacadabra_morphs::MorphAssetKind>,
    #[serde(default)]
    presets: Vec<LocalMorphPreset>,
}

#[derive(Debug, serde::Deserialize)]
struct LocalMorphAsset {
    id: cubacadabra_morphs::MorphAssetId,
    source: String,
}

#[derive(Debug, serde::Deserialize)]
struct LocalMorphPreset {
    source: String,
}

struct StudioApp {
    project_root: PathBuf,
    game_root: PathBuf,
    authored_manifest_source: String,
    manifest_source: String,
    standalone_preview: bool,
    temporary_package: Option<PathBuf>,
    codex: CodexClient,
    network: BackendClient,
    client: ClientSession,
    image_atlas: Option<ImageAtlas>,
    window: Option<Window>,
    renderer: Option<Renderer>,
    shell: Option<StudioShell>,
    pending_project_load: Option<PendingProjectLoad>,
    background_project_ready: Option<(PathBuf, bool, bool, Result<BackgroundProjectLoad, String>)>,
    prepared_project_ready: Option<(PathBuf, bool, bool, Result<PreparedProjectLoad, String>)>,
    renderer_uses_base_package_generation: bool,
    codex_checkpoint: Option<ProjectFileSnapshot>,
    codex_changes: Option<Vec<CodexFileChange>>,
    local_morph_catalog: Option<LocalMorphCatalog>,
    pressed_keys: HashSet<KeyCode>,
    jump_queued: bool,
    mobile_sprint: bool,
    morph_loadout: cubacadabra_morphs::MorphLoadout,
    morph_request_serial: u64,
    pending_morph: Option<(u64, cubacadabra_morphs::MorphLoadout)>,
    registered_morphs: HashSet<String>,
    climb: bool,
    joystick_input: (f32, f32),
    pointer_position: Option<(f32, f32)>,
    pointer_active: bool,
    camera_pointer_active: bool,
    movement_pointer_active: bool,
    movement_pointer_origin: Option<(f32, f32)>,
    ui_pointer_active: bool,
    look_delta: (f32, f32),
    zoom_delta: f32,
    last_frame: Instant,
}

impl StudioApp {
    fn load(
        game_root: Option<PathBuf>,
        morph_catalog_path: Option<PathBuf>,
    ) -> Result<Self, Box<dyn Error>> {
        let sources = load_game_sources(game_root)?;
        let authored_manifest_source = sources.authored_manifest_source;
        let manifest_source = sources.manifest_source;
        let script_source = sources.script_source;
        let game_root = sources.root;
        let project_root = sources.project_root;
        let standalone_preview = sources.standalone_preview;
        let temporary_package = sources.temporary_package;
        let morph_catalog_path =
            morph_catalog_path.or_else(|| discover_project_morph_catalog(&project_root));
        let local_morph_catalog = morph_catalog_path
            .as_deref()
            .map(load_local_morph_catalog)
            .transpose()?;
        let mut client = ClientSession::load(&manifest_source, &script_source)?;
        if standalone_preview {
            let position = client
                .engine()
                .snapshot()
                .get(..3)
                .and_then(|values| values.try_into().ok());
            if let Some(position) = position {
                client
                    .engine_mut()
                    .reconcile_player(position, std::f32::consts::PI);
            }
        }
        let network = BackendClient::new(client.game_id()).map_err(StudioError)?;
        let codex = CodexClient::new(&project_root).map_err(StudioError)?;
        info!(
            "studio loaded: game_id={} standalone_preview={} root={}",
            client.game_id(),
            standalone_preview,
            game_root.display()
        );
        if local_morph_catalog.is_none() {
            network.request_morph_catalog();
        }

        Ok(Self {
            project_root,
            image_atlas: load_image_atlas(&game_root, &manifest_source)?,
            authored_manifest_source,
            manifest_source,
            game_root,
            standalone_preview,
            temporary_package,
            codex,
            network,
            client,
            window: None,
            renderer: None,
            shell: None,
            pending_project_load: None,
            background_project_ready: None,
            prepared_project_ready: None,
            renderer_uses_base_package_generation: true,
            codex_checkpoint: None,
            codex_changes: None,
            local_morph_catalog,
            pressed_keys: HashSet::new(),
            jump_queued: false,
            mobile_sprint: false,
            morph_loadout: default_morph_loadout(),
            morph_request_serial: 0,
            pending_morph: None,
            registered_morphs: HashSet::new(),
            climb: false,
            joystick_input: (0.0, 0.0),
            pointer_position: None,
            pointer_active: false,
            camera_pointer_active: false,
            movement_pointer_active: false,
            movement_pointer_origin: None,
            ui_pointer_active: false,
            look_delta: (0.0, 0.0),
            zoom_delta: 0.0,
            last_frame: Instant::now(),
        })
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let window = event_loop.create_window(
            WindowAttributes::default()
                .with_title(format!(
                    "Cubacadabra Studio — {}",
                    game_name(&self.game_root)
                ))
                .with_inner_size(LogicalSize::new(
                    DEFAULT_WINDOW_WIDTH,
                    DEFAULT_WINDOW_HEIGHT,
                )),
        )?;
        let size = window.inner_size();
        let display_handle = window.display_handle()?.as_raw();
        let window_handle = window.window_handle()?.as_raw();
        let mut renderer = Renderer::new(
            display_handle,
            window_handle,
            size.width as f32,
            size.height as f32,
        )
        .ok_or_else(|| StudioError("the shared wgpu renderer could not start".to_owned()))?;

        if let Some(atlas) = &self.image_atlas {
            if !renderer.set_package_image_atlas(
                atlas.width,
                atlas.height,
                &atlas.pixels,
                atlas.regions.clone(),
            ) {
                return Err(Box::new(StudioError(
                    "the game's image atlas could not be uploaded".to_owned(),
                )));
            }
        }

        let mut shell = StudioShell::new(&window, &renderer, &self.manifest_source);
        shell.set_project_asset_available(!self.standalone_preview);
        shell.set_project_editable(
            !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
        );
        shell.set_source_manifest(&self.authored_manifest_source, false);
        shell.set_codex_project_root(self.project_root.clone());
        if let Ok(parent) = env::current_dir() {
            shell.set_new_project_parent(parent);
        }
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.shell = Some(shell);
        if let Some(local_catalog) = self.local_morph_catalog.take() {
            self.install_local_morphs(local_catalog)
                .map_err(|message| Box::new(StudioError(message)) as Box<dyn Error>)?;
        }
        self.update_viewport();
        self.request_redraw();
        Ok(())
    }

    fn update_viewport(&mut self) {
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let fallback = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size.width as f32 / scale, size.height as f32 / scale),
        );
        let viewport = self
            .shell
            .as_ref()
            .map(StudioShell::runtime_viewport)
            .filter(|rect| rect.is_positive())
            .unwrap_or(fallback);
        self.client.set_ui_viewport_values(
            viewport.width(),
            viewport.height(),
            scale,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        if let Some(renderer) = &mut self.renderer {
            renderer.set_studio_viewport(Some([
                viewport.min.x * scale,
                viewport.min.y * scale,
                viewport.width() * scale,
                viewport.height() * scale,
            ]));
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(size.width as f32, size.height as f32);
        }
        self.update_viewport();
    }

    fn render(&mut self) {
        self.drain_backend_events();
        self.drain_codex_events();
        self.commit_ready_project_load();
        self.prepare_ready_project_runtime();
        self.poll_project_load();
        #[cfg(target_os = "macos")]
        while let Some(command) = macos::take_menu_action() {
            if let Some(shell) = &mut self.shell {
                shell.execute_command(command);
            }
        }
        if self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_open_project_request)
        {
            self.choose_and_open_project();
        }
        #[cfg(target_os = "macos")]
        if let Some((title, parent, error)) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_native_new_project_dialog)
            && let Some((title, parent)) =
                macos::show_new_project_dialog(&title, &parent, error.as_deref())
        {
            if let Some(shell) = &mut self.shell {
                shell.set_new_project_draft(title.clone(), parent.clone());
            }
            self.create_new_project(&title, &parent);
        }
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;

        let project_name = game_name(&self.project_root);
        if let Some(shell) = &mut self.shell {
            shell.set_active_morph_loadout(&self.morph_loadout);
        }
        if let Some(request) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_morph_request)
        {
            if matches!(request, wardrobe::Request::RetryCatalog) {
                self.network.request_morph_catalog();
            } else if let Err(message) = self.request_morph_change(request) {
                if let Some(shell) = &mut self.shell {
                    shell.set_notice(message);
                }
            }
        }
        let import_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_import_request);
        if import_requested {
            self.import_morph_glb();
        }
        let auth_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_auth_request);
        if auth_requested {
            if let Some(shell) = &mut self.shell {
                shell.set_auth_pending(true);
                shell.set_notice("Opening browser for sign-in…".to_owned());
            }
            self.network.begin_browser_auth();
        }
        let chatgpt_auth_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_chatgpt_auth_request);
        if chatgpt_auth_requested {
            if let Some(shell) = &mut self.shell {
                shell.set_chatgpt_pending();
            }
            if let Err(message) = self.codex.begin_chatgpt_login()
                && let Some(shell) = &mut self.shell
            {
                shell.set_chatgpt_error(message);
            }
        }
        let codex_chat_open_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_codex_chat_open_request);
        if codex_chat_open_requested
            && let Err(message) = self.codex.open_chat()
            && let Some(shell) = &mut self.shell
        {
            shell.set_codex_chat_error(message);
        }
        if let Some(request) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_codex_chat_send_request)
        {
            self.codex_checkpoint = snapshot_project_files(&self.project_root).ok();
            self.codex_changes = None;
            if let Err(message) = self.codex.send_chat_message(
                request.message,
                request.model.to_owned(),
                request.reasoning_effort.to_owned(),
            ) {
                self.codex_checkpoint = None;
                if let Some(shell) = &mut self.shell {
                    shell.set_codex_chat_error(message);
                }
            }
        }
        let cancel_codex_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_codex_cancel_request);
        if cancel_codex_requested {
            if let Some(shell) = &mut self.shell {
                shell.set_codex_chat_cancelling();
            }
            if let Err(message) = self.codex.cancel_chat_message()
                && let Some(shell) = &mut self.shell
            {
                shell.set_codex_chat_error(message);
            }
        }
        let draft_export_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_draft_export_request);
        if draft_export_requested {
            self.export_morph_draft();
        }
        // Morph requests can turn the loading veil on or commit the first
        // native v2 appearance. Prepare the overlay after those transitions
        // so the old bundled character never reaches a visible frame.
        let prepared_shell: Option<PreparedShell> = match (&mut self.shell, &self.window) {
            (Some(shell), Some(window)) => Some(shell.prepare(window, &project_name)),
            _ => None,
        };
        let undo_codex_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_codex_undo_request);
        if undo_codex_requested {
            self.undo_codex_changes();
        }
        if let Some(edit) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_scene_edit_request)
        {
            if let Err(message) = self.apply_scene_edit(edit) {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(message);
                }
            }
        }
        let save_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_save_request);
        if save_requested {
            self.save_project_source();
        }
        let rebuild_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_rebuild_and_play_request);
        if rebuild_requested {
            self.save_project_source();
            self.start_project_reload();
        }
        let restart_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_restart_request);
        if restart_requested {
            self.start_project_reload();
        }
        let sidecar_export_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_sidecar_export_request);
        if sidecar_export_requested {
            self.export_morph_sidecar();
        }
        let sidecar_import_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_sidecar_import_request);
        if sidecar_import_requested {
            self.import_morph_sidecar();
        }
        #[cfg(not(target_os = "macos"))]
        let new_project_folder_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_new_project_folder_request);
        #[cfg(not(target_os = "macos"))]
        if new_project_folder_requested {
            if let Some(parent) = rfd::FileDialog::new()
                .set_title("Choose where to create the game")
                .pick_folder()
            {
                if let Some(shell) = &mut self.shell {
                    shell.set_new_project_parent(parent);
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let new_project_request = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_new_project_request);
        #[cfg(not(target_os = "macos"))]
        if let Some((title, parent)) = new_project_request {
            self.create_new_project(&title, &parent);
        }
        let project_add_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_project_add_request);
        if project_add_requested {
            self.add_morph_to_game();
        }
        let pack_import_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_pack_import_request);
        if pack_import_requested {
            self.import_morph_pack();
        }
        let publish_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_publish_request);
        if publish_requested {
            self.publish_morph_pack();
        }
        let thumbnail_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_thumbnail_request);
        if thumbnail_requested {
            self.generate_morph_thumbnail();
        }
        self.update_viewport();
        let project_loading = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::is_project_loading);
        let playing = !project_loading && self.shell.as_ref().is_none_or(StudioShell::is_playing);
        let morph_preview = !project_loading
            && self
                .shell
                .as_ref()
                .is_some_and(StudioShell::is_morphs_workspace);
        let controls_active = playing || morph_preview;

        let mut forward = if controls_active {
            axis(
                &self.pressed_keys,
                &[KeyCode::KeyW, KeyCode::ArrowUp],
                &[KeyCode::KeyS, KeyCode::ArrowDown],
            )
        } else {
            0.0
        };
        let mut strafe = if controls_active {
            axis(
                &self.pressed_keys,
                &[KeyCode::KeyD, KeyCode::ArrowRight],
                &[KeyCode::KeyA, KeyCode::ArrowLeft],
            )
        } else {
            0.0
        };
        if controls_active {
            let (joystick_forward, joystick_strafe) = joystick_movement(self.joystick_input);
            forward += joystick_forward;
            strafe += joystick_strafe;
        }
        let length = (forward * forward + strafe * strafe).sqrt();
        let (forward, strafe) = if length > 1.0 {
            (forward / length, strafe / length)
        } else {
            (forward, strafe)
        };
        let sprint = self.mobile_sprint
            || self.pressed_keys.contains(&KeyCode::ShiftLeft)
            || self.pressed_keys.contains(&KeyCode::ShiftRight);
        self.client.set_input_values(
            forward,
            strafe,
            controls_active && sprint,
            controls_active && self.jump_queued,
            controls_active && self.climb,
            self.look_delta.0,
            self.look_delta.1,
            self.zoom_delta,
        );
        self.jump_queued = false;
        self.look_delta = (0.0, 0.0);
        self.zoom_delta = 0.0;
        self.dispatch_client_actions();
        if playing {
            self.client.step(delta);
        }
        self.drain_ui_events();
        self.dispatch_client_actions();
        if !self.standalone_preview
            && let Some(movement) = self.client.local_movement(length > 0.01, playing && sprint)
        {
            self.network.send_move(
                movement.position[0],
                movement.position[1],
                movement.position[2],
                movement.yaw,
                movement.moving,
                movement.sprinting,
                movement.respawn_event_id,
            );
        }

        if let Some(renderer) = &mut self.renderer {
            renderer.set_avatar_preview_mode(
                self.shell
                    .as_ref()
                    .is_some_and(StudioShell::is_morphs_workspace),
            );
            renderer.sync(self.client.engine());
            match (&mut self.shell, prepared_shell) {
                (Some(shell), Some(prepared)) => {
                    renderer.draw_with_overlay(|device, queue, encoder, destination| {
                        shell.paint(device, queue, encoder, destination, prepared);
                    });
                }
                _ => renderer.draw(),
            }
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn import_morph_glb(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB model", &["glb"])
            .set_title("Import morph GLB")
            .pick_file()
        else {
            return;
        };
        let display_path = path.display().to_string();
        let result = fs::read(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))
            .and_then(|bytes| {
                let preview = decode_source_glb_preview(&bytes)
                    .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                let summary = inspect_source_glb_structure(&bytes)
                    .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                let lod_previews: [Option<MorphGlbPreviewMesh>; 3] = std::array::from_fn(|index| {
                    let level = ["near", "mid", "far"][index];
                    summary
                        .lod_candidates
                        .get(level)
                        .filter(|candidates| candidates.len() == 1)
                        .and_then(|candidates| candidates.first())
                        .and_then(|node| decode_source_glb_preview_node(&bytes, node).ok())
                });
                Ok((preview, summary, lod_previews))
            });
        if let Some(shell) = &mut self.shell {
            match result {
                Ok((preview, summary, lod_previews)) => {
                    shell.set_morph_preview(display_path, preview, summary);
                    for (level, preview) in lod_previews.into_iter().enumerate() {
                        if let Some(preview) = preview {
                            shell.set_morph_lod_preview(level, preview);
                        }
                    }
                }
                Err(message) => shell.set_morph_import_error(message),
            }
        }
        self.request_redraw();
    }

    fn export_morph_sidecar(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_sidecar_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, json)| {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph sidecar", &["json"])
                .set_file_name(&suggested_name)
                .set_title("Export morph sidecar")
                .save_file()
            else {
                return Err("Sidecar export cancelled.".to_owned());
            };
            fs::write(&path, json)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_sidecar_export_result(result);
        }
        self.request_redraw();
    }

    fn export_morph_draft(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_draft_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, json)| {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph draft", &["json"])
                .set_file_name(&suggested_name)
                .set_title("Save morph draft")
                .save_file()
            else {
                return Err("Morph draft save cancelled.".to_owned());
            };
            fs::write(&path, json)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_draft_export_result(result);
        }
        self.request_redraw();
    }

    fn import_morph_sidecar(&mut self) {
        let Some(sidecar_path) = rfd::FileDialog::new()
            .add_filter("Morph sidecar", &["json"])
            .set_title("Open morph sidecar")
            .pick_file()
        else {
            return;
        };
        let result = fs::read_to_string(&sidecar_path)
            .map_err(|error| format!("Could not read {}: {error}", sidecar_path.display()))
            .and_then(|manifest_source| {
                let draft = if is_morph_draft_json(&manifest_source) {
                    Some(parse_morph_draft_json(&manifest_source)?)
                } else {
                    None
                };
                let geometry_file = match &draft {
                    Some(draft) => draft.geometry_file.clone(),
                    None => source_manifest_geometry_file(&manifest_source)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?,
                };
                let glb_path = sidecar_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(geometry_file);
                let glb = fs::read(&glb_path).map_err(|error| {
                    format!(
                        "Could not read referenced GLB {}: {error}",
                        glb_path.display()
                    )
                })?;
                let (manifest, draft, preview, summary, lod_previews) = match draft {
                    Some(draft) => {
                        let preview = decode_source_glb_preview(&glb)
                            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                        let summary = inspect_source_glb_structure(&glb)
                            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                        let lod_previews: [Option<MorphGlbPreviewMesh>; 3] =
                            std::array::from_fn(|index| {
                                if draft.lod_nodes[index].is_empty() {
                                    None
                                } else {
                                    decode_source_glb_preview_node(&glb, &draft.lod_nodes[index])
                                        .ok()
                                }
                            });
                        (None, Some(draft), preview, summary, lod_previews)
                    }
                    None => {
                        let (manifest, preview, summary) =
                            inspect_source_sidecar(&manifest_source, &glb).map_err(
                                |diagnostics| Self::format_morph_diagnostics(&diagnostics),
                            )?;
                        let lod_previews: [Option<MorphGlbPreviewMesh>; 3] =
                            std::array::from_fn(|index| {
                                let level = ["near", "mid", "far"][index];
                                manifest.geometry.lod_nodes.get(level).and_then(|node| {
                                    decode_source_glb_preview_node(&glb, node).ok()
                                })
                            });
                        (Some(manifest), None, preview, summary, lod_previews)
                    }
                };
                Ok((
                    glb_path.display().to_string(),
                    manifest,
                    draft,
                    preview,
                    summary,
                    lod_previews,
                ))
            });
        if let Some(shell) = &mut self.shell {
            match result {
                Ok((glb_path, manifest, draft, preview, summary, lod_previews)) => {
                    if let Some(manifest) = manifest {
                        shell.set_morph_sidecar_preview(glb_path, manifest, preview, summary);
                    } else if let Some(draft) = draft {
                        shell.set_morph_draft_preview(glb_path, draft, preview, summary);
                    }
                    for (level, preview) in lod_previews.into_iter().enumerate() {
                        if let Some(preview) = preview {
                            shell.set_morph_lod_preview(level, preview);
                        }
                    }
                    shell.select_morph_preview_lod(Some(0));
                }
                Err(message) => shell.set_morph_import_error(message),
            }
        }
        self.request_redraw();
    }

    fn publish_morph_pack(&mut self) {
        let inputs = self
            .shell
            .as_ref()
            .map(StudioShell::morph_pack_inputs)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = inputs.and_then(|(glb_path, suggested_name, manifest_json)| {
            let glb = fs::read(&glb_path)
                .map_err(|error| format!("Could not read {}: {error}", glb_path))?;
            let (pack, summary) = compile_source_morph_pack(&manifest_json, &glb)
                .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph pack", &["morphpack"])
                .set_file_name(&suggested_name)
                .set_title("Publish morph pack")
                .save_file()
            else {
                return Err("Morph pack publish cancelled.".to_owned());
            };
            fs::write(&path, &pack)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
            Ok((summary.asset_id, summary.byte_len, pack))
        });
        let result = result.and_then(|(_, _, pack)| self.activate_morph_pack(&pack));
        if let Some(shell) = &mut self.shell {
            shell.set_morph_publish_result(result);
        }
        self.request_redraw();
    }

    fn add_morph_to_game(&mut self) {
        let result = if self.standalone_preview {
            Err("Open a game project before adding a character asset to it.".to_owned())
        } else {
            self.shell
                .as_ref()
                .map(StudioShell::morph_project_payload)
                .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()))
                .and_then(|(source_path, manifest_json, preview)| {
                    let glb = fs::read(&source_path)
                        .map_err(|error| format!("Could not read {}: {error}", source_path))?;
                    let manifest_json =
                        rewrite_manifest_geometry_file(&manifest_json, "source.glb")?;
                    let asset = source_manifest_asset(&manifest_json)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                    let asset_id = asset.id.to_string();
                    let slug = project_asset_slug(&asset_id)?;
                    let asset_directory = self.project_root.join("assets/characters").join(&slug);
                    fs::create_dir_all(&asset_directory).map_err(|error| {
                        format!(
                            "Could not create character asset directory {}: {error}",
                            asset_directory.display()
                        )
                    })?;
                    let (pack, _) = compile_source_morph_pack(&manifest_json, &glb)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                    let thumbnail = encode_morph_thumbnail_png(&preview)?;
                    write_atomic(&asset_directory.join("source.glb"), &glb)?;
                    write_atomic(
                        &asset_directory.join("source.morph.json"),
                        manifest_json.as_bytes(),
                    )?;
                    write_atomic(&asset_directory.join("runtime.morphpack"), &pack)?;
                    write_atomic(&asset_directory.join("thumbnail.png"), &thumbnail)?;

                    let catalog_path = self.project_root.join("assets/characters/catalog.json");
                    update_project_morph_catalog(
                        &catalog_path,
                        &asset_id,
                        &format!("{slug}/source.morph.json"),
                    )?;
                    let activated = self.activate_morph_pack(&pack)?;
                    Ok(activated)
                })
        };
        if let Some(shell) = &mut self.shell {
            shell.set_morph_project_result(result);
        }
        self.request_redraw();
    }

    fn create_new_project(&mut self, title: &str, parent: &Path) {
        let result = game_creator::create_game(title, parent);
        match result {
            Ok(created) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_new_project_created(&created.project);
                }
                self.start_project_load(created.project);
            }
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_new_project_error(error);
                }
            }
        }
        self.request_redraw();
    }

    fn save_project_source(&mut self) {
        let editable = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable);
        if !editable {
            return;
        }
        let manifest_path = self.project_root.join("manifest.json");
        match write_atomic(&manifest_path, self.authored_manifest_source.as_bytes()) {
            Ok(()) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_source_manifest(&self.authored_manifest_source, false);
                    shell.set_notice("Project saved".to_owned());
                }
            }
            Err(message) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(message);
                }
            }
        }
    }

    fn capture_codex_changes(&mut self) -> Vec<CodexFileChange> {
        let Some(before) = self.codex_checkpoint.take() else {
            self.codex_changes = None;
            return Vec::new();
        };
        let Ok(after) = snapshot_project_files(&self.project_root) else {
            self.codex_changes = None;
            return Vec::new();
        };
        let changes = diff_project_files(before, after);
        self.codex_changes = Some(changes.clone());
        changes
    }

    fn undo_codex_changes(&mut self) {
        let Some(changes) = self.codex_changes.take() else {
            if let Some(shell) = &mut self.shell {
                shell.set_notice("There are no Codex changes to undo".to_owned());
            }
            return;
        };
        let mut restored = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();
        for change in changes {
            let path = self.project_root.join(&change.relative_path);
            let current = fs::read(&path).ok();
            if current != change.after {
                skipped += 1;
                continue;
            }
            let result = match change.before {
                Some(bytes) => write_atomic(&path, &bytes),
                None => fs::remove_file(&path).map_err(|error| {
                    format!(
                        "could not remove {}: {error}",
                        change.relative_path.display()
                    )
                }),
            };
            match result {
                Ok(()) => restored += 1,
                Err(message) => errors.push(message),
            }
        }
        if let Some(shell) = &mut self.shell {
            shell.clear_codex_changes();
            if errors.is_empty() {
                let message = if skipped == 0 {
                    format!("Undid {restored} Codex change(s); rebuilding preview…")
                } else {
                    format!(
                        "Undid {restored} Codex change(s); kept {skipped} later edit(s); rebuilding preview…"
                    )
                };
                shell.set_notice(message);
            } else {
                shell.set_project_error(format!(
                    "Undo restored {restored} file(s), skipped {skipped}, and failed: {}",
                    errors.join("; ")
                ));
            }
        }
        self.refresh_authored_manifest_from_disk();
        self.start_project_reload();
    }

    fn refresh_authored_manifest_from_disk(&mut self) {
        let path = self.project_root.join("manifest.json");
        match fs::read_to_string(&path) {
            Ok(source) => {
                self.authored_manifest_source = source.clone();
                if let Some(shell) = &mut self.shell {
                    shell.set_source_manifest(&source, false);
                }
            }
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(format!(
                        "Could not read the updated manifest {}: {error}",
                        path.display()
                    ));
                }
            }
        }
    }

    fn apply_scene_edit(&mut self, request: SceneEditRequest) -> Result<(), String> {
        if !self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable)
        {
            return Err("Open a raw source project to edit scene objects.".to_owned());
        }
        let mut manifest: Value = serde_json::from_str(&self.authored_manifest_source)
            .map_err(|error| format!("manifest is no longer valid JSON: {error}"))?;
        if let SceneEditRequest::UpdateSignText {
            ref target,
            ref text,
        } = request
        {
            update_manifest_sign_text(&mut manifest, target, text.clone())?;
            let source = serde_json::to_string_pretty(&manifest)
                .map_err(|error| format!("could not serialize the scene manifest: {error}"))?
                + "\n";
            self.authored_manifest_source = source.clone();
            if let Some(shell) = &mut self.shell {
                shell.set_source_manifest(&source, true);
                shell.set_notice("Sign text changed — save to keep it".to_owned());
            }
            return Ok(());
        }
        let (target, operation) = match request {
            SceneEditRequest::UpdateBlock {
                target,
                position,
                size,
            } => (target, SceneEditOperation::Update { position, size }),
            SceneEditRequest::DuplicateBlock { target } => (target, SceneEditOperation::Duplicate),
            SceneEditRequest::DeleteBlock { target } => (target, SceneEditOperation::Delete),
            SceneEditRequest::UpdateSignText { .. } => {
                return Err("sign text edit was not handled".to_owned());
            }
        };
        let (world_id, index) = parse_block_target(&target)?;
        let world = if world_id == "lobby" {
            &mut manifest
        } else {
            manifest
                .get_mut("worlds")
                .and_then(Value::as_object_mut)
                .and_then(|worlds| worlds.get_mut(&world_id))
                .ok_or_else(|| format!("scene world `{world_id}` was not found"))?
        };
        let blocks = world
            .get_mut("blocks")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| format!("scene world `{world_id}` has no blocks"))?;
        let block = blocks
            .get_mut(index)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| format!("scene block `{target}` was not found"))?;
        match operation {
            SceneEditOperation::Update { position, size } => {
                block.insert("position".to_owned(), serde_json::json!(position));
                block.insert("size".to_owned(), serde_json::json!(size));
            }
            SceneEditOperation::Duplicate => {
                let mut copy = Value::Object(block.clone());
                let copy_object = copy
                    .as_object_mut()
                    .ok_or_else(|| "scene block could not be duplicated".to_owned())?;
                let base_id = copy_object
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("platform");
                copy_object.insert(
                    "id".to_owned(),
                    Value::String(format!("{base_id}-copy-{}", blocks.len() + 1)),
                );
                blocks.push(copy);
            }
            SceneEditOperation::Delete => {
                blocks.remove(index);
            }
        }
        let source = serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("could not serialize the scene manifest: {error}"))?
            + "\n";
        self.authored_manifest_source = source.clone();
        if let Some(shell) = &mut self.shell {
            shell.set_source_manifest(&source, true);
            shell.set_notice(match operation {
                SceneEditOperation::Update { .. } => {
                    "Platform changed — save to keep it".to_owned()
                }
                SceneEditOperation::Duplicate => "Platform duplicated — save to keep it".to_owned(),
                SceneEditOperation::Delete => "Platform deleted — save to keep it".to_owned(),
            });
        }
        Ok(())
    }

    fn choose_and_open_project(&mut self) {
        let starting_directory = if !self.standalone_preview && self.project_root.is_dir() {
            self.project_root
                .parent()
                .unwrap_or(&self.project_root)
                .to_path_buf()
        } else {
            env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };
        let mut dialog = rfd::FileDialog::new()
            .set_title("Open Project")
            .set_directory(starting_directory)
            .set_can_create_directories(false);
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window);
        }
        let Some(project) = dialog.pick_folder() else {
            return;
        };
        self.start_project_load(project);
    }

    fn start_project_load(&mut self, project: PathBuf) {
        self.start_project_load_with_mode(project, false, false);
    }

    fn start_project_reload(&mut self) {
        self.start_project_reload_with_origin(false);
    }

    fn start_codex_project_reload(&mut self) {
        self.start_project_reload_with_origin(true);
    }

    fn start_project_reload_with_origin(&mut self, codex_rebuild: bool) {
        if self.standalone_preview {
            if let Some(shell) = &mut self.shell {
                shell.set_project_error(
                    "The standalone morph preview cannot be rebuilt.".to_owned(),
                );
                if codex_rebuild {
                    shell.set_codex_preview_rebuild_failed(
                        "the standalone morph preview cannot be rebuilt",
                    );
                }
            }
            return;
        }
        self.start_project_load_with_mode(self.project_root.clone(), true, codex_rebuild);
    }

    fn start_project_load_with_mode(
        &mut self,
        project: PathBuf,
        preserve_editor: bool,
        codex_rebuild: bool,
    ) {
        if self.pending_project_load.is_some()
            || self.background_project_ready.is_some()
            || self.prepared_project_ready.is_some()
        {
            if codex_rebuild && let Some(shell) = &mut self.shell {
                shell.set_codex_preview_rebuild_failed("another project load is already running");
            }
            return;
        }
        if let Some(shell) = &mut self.shell {
            if preserve_editor {
                shell.begin_game_rebuild();
            } else {
                shell.begin_project_loading();
            }
            shell.set_project_loading_progress(0.01);
        }
        let (sender, receiver) = mpsc::channel();
        let worker_project = project.clone();
        let spawn = thread::Builder::new()
            .name("studio-project-loader".to_owned())
            .spawn(move || {
                let progress_sender = sender.clone();
                let result = load_project_in_background(&worker_project, move |progress| {
                    let _ = progress_sender.send(ProjectLoadEvent::Progress(progress));
                });
                let _ = sender.send(ProjectLoadEvent::Finished(result));
            });
        match spawn {
            Ok(_) => {
                self.pending_project_load = Some(PendingProjectLoad {
                    project,
                    preserve_editor,
                    codex_rebuild,
                    receiver,
                });
            }
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.cancel_project_loading();
                    if codex_rebuild {
                        shell.set_codex_preview_rebuild_failed(&format!(
                            "the project loader could not start: {error}"
                        ));
                    }
                }
                self.show_open_project_error(&format!(
                    "Could not start the project loader: {error}"
                ));
            }
        }
        self.request_redraw();
    }

    fn poll_project_load(&mut self) {
        let mut finished = None;
        loop {
            let event = self
                .pending_project_load
                .as_ref()
                .map(|load| load.receiver.try_recv());
            match event {
                Some(Ok(ProjectLoadEvent::Progress(progress))) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_project_loading_progress(progress);
                    }
                }
                Some(Ok(ProjectLoadEvent::Finished(result))) => {
                    finished = Some(result);
                    break;
                }
                Some(Err(mpsc::TryRecvError::Empty)) | None => break,
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    finished = Some(Err("The project loader stopped unexpectedly.".to_owned()));
                    break;
                }
            }
        }
        if let Some(result) = finished {
            let (project, preserve_editor, codex_rebuild) = self
                .pending_project_load
                .take()
                .map(|load| (load.project, load.preserve_editor, load.codex_rebuild))
                .unwrap_or_default();
            self.background_project_ready = Some((project, preserve_editor, codex_rebuild, result));
        }
    }

    fn prepare_ready_project_runtime(&mut self) {
        let Some((project, preserve_editor, codex_rebuild, result)) =
            self.background_project_ready.take()
        else {
            return;
        };
        let result = match result {
            Ok(background) => match ClientSession::load(
                &background.sources.manifest_source,
                &background.sources.script_source,
            ) {
                Ok(client) => match BackendClient::new(client.game_id()) {
                    Ok(network) => Ok(PreparedProjectLoad {
                        background,
                        client,
                        network,
                    }),
                    Err(error) => {
                        remove_temporary_package(&background.sources);
                        Err(error)
                    }
                },
                Err(error) => {
                    remove_temporary_package(&background.sources);
                    Err(error.to_string())
                }
            },
            Err(error) => Err(error),
        };
        if result.is_ok()
            && let Some(shell) = &mut self.shell
        {
            shell.set_project_loading_progress(0.93);
        }
        self.prepared_project_ready = Some((project, preserve_editor, codex_rebuild, result));
    }

    fn commit_ready_project_load(&mut self) {
        let Some((project, preserve_editor, codex_rebuild, result)) =
            self.prepared_project_ready.take()
        else {
            return;
        };
        let result =
            result.and_then(|prepared| self.commit_project_load(prepared, preserve_editor));
        match result {
            Ok(()) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_notice(if preserve_editor {
                        "Preview rebuilt and playing".to_owned()
                    } else {
                        format!("Opened {}", project.display())
                    });
                    if codex_rebuild {
                        shell.set_codex_preview_rebuilt();
                    }
                }
            }
            Err(message) => {
                if let Some(shell) = &mut self.shell {
                    shell.cancel_project_loading();
                }
                if preserve_editor {
                    if let Some(shell) = &mut self.shell {
                        shell.set_project_error(format!("Rebuild failed: {message}"));
                        if codex_rebuild {
                            shell.set_codex_preview_rebuild_failed(&message);
                        }
                    }
                } else {
                    self.show_open_project_error(&message);
                }
            }
        }
    }

    fn show_open_project_error(&mut self, message: &str) {
        if let Some(shell) = &mut self.shell {
            shell.set_notice(format!("Could not open project: {message}"));
        }
        let mut dialog = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Couldn’t Open Project")
            .set_description(message)
            .set_buttons(rfd::MessageButtons::Ok);
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window);
        }
        dialog.show();
    }

    fn commit_project_load(
        &mut self,
        prepared: PreparedProjectLoad,
        preserve_editor: bool,
    ) -> Result<(), String> {
        let PreparedProjectLoad {
            background:
                BackgroundProjectLoad {
                    sources:
                        GameSources {
                            project_root,
                            root,
                            authored_manifest_source,
                            manifest_source,
                            script_source,
                            standalone_preview,
                            temporary_package,
                        },
                    image_atlas,
                    local_morph_catalog,
                },
            mut client,
            network,
        } = prepared;

        // A newly-created engine starts its package generation at the same
        // value as the previous engine. The long-lived renderer therefore
        // cannot distinguish two consecutive ClientSession values by
        // generation alone. Alternate between the first and second package
        // generation using the engine's existing public loading API. This
        // keeps Studio compatible with released engine checkouts while still
        // forcing the renderer to consume the replacement scene.
        let client_uses_base_package_generation = !self.renderer_uses_base_package_generation;
        if !client_uses_base_package_generation {
            if !client.engine_mut().load_package_source(&manifest_source) {
                if let Some(package) = temporary_package {
                    let _ = fs::remove_dir_all(package);
                }
                return Err("the shared engine rejected the rebuilt scene".to_owned());
            }
            if !client.engine_mut().load_script_source(&script_source) {
                if let Some(package) = temporary_package {
                    let _ = fs::remove_dir_all(package);
                }
                return Err("the shared engine could not compile the rebuilt game logic".to_owned());
            }
        }

        if let (Some(renderer), Some(atlas)) = (&mut self.renderer, &image_atlas)
            && !renderer.set_package_image_atlas(
                atlas.width,
                atlas.height,
                &atlas.pixels,
                atlas.regions.clone(),
            )
        {
            if let Some(package) = temporary_package {
                let _ = fs::remove_dir_all(package);
            }
            return Err("the new game's image atlas could not be uploaded".to_owned());
        }
        let old_temporary_package = self.temporary_package.take();
        self.project_root = project_root;
        self.game_root = root;
        self.authored_manifest_source = authored_manifest_source;
        self.manifest_source = manifest_source;
        self.standalone_preview = standalone_preview;
        self.temporary_package = temporary_package;
        self.image_atlas = image_atlas;
        self.network = network;
        self.client = client;
        self.renderer_uses_base_package_generation = client_uses_base_package_generation;
        self.local_morph_catalog = local_morph_catalog;
        self.morph_loadout = default_morph_loadout();
        self.morph_request_serial = 0;
        self.pending_morph = None;
        self.registered_morphs.clear();
        self.pressed_keys.clear();
        if let Some(window) = &self.window {
            window.set_title(&format!(
                "Cubacadabra Studio — {}",
                game_name(&self.project_root)
            ));
        }
        if preserve_editor {
            if let Some(shell) = &mut self.shell {
                shell.set_project_editable(
                    !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
                );
                shell.set_source_manifest(&self.authored_manifest_source, false);
                shell.finish_project_loading();
                shell.set_notice("Preview rebuilt and playing".to_owned());
            }
            if let Some(local_catalog) = self.local_morph_catalog.take() {
                self.install_local_morphs(local_catalog)?;
            }
            if let Some(package) = old_temporary_package {
                let _ = fs::remove_dir_all(package);
            }
            self.update_viewport();
            return Ok(());
        }

        let mut shell = {
            let window = self
                .window
                .as_ref()
                .ok_or_else(|| "Studio window is not ready.".to_owned())?;
            let renderer = self
                .renderer
                .as_ref()
                .ok_or_else(|| "Studio renderer is not ready.".to_owned())?;
            StudioShell::new(window, renderer, &self.manifest_source)
        };
        shell.set_project_asset_available(true);
        shell.set_project_editable(
            !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
        );
        shell.set_source_manifest(&self.authored_manifest_source, false);
        shell.set_codex_project_root(self.project_root.clone());
        if let Err(message) = self.codex.set_project_root(&self.project_root) {
            shell.set_codex_chat_error(message);
        }
        if let Some(parent) = self.project_root.parent() {
            shell.set_new_project_parent(parent.to_path_buf());
        }
        self.shell = Some(shell);
        if let Some(local_catalog) = self.local_morph_catalog.take() {
            self.install_local_morphs(local_catalog)?;
        }
        if let Some(package) = old_temporary_package {
            let _ = fs::remove_dir_all(package);
        }
        self.update_viewport();
        Ok(())
    }

    fn import_morph_pack(&mut self) {
        let result = rfd::FileDialog::new()
            .add_filter("Morph pack", &["morphpack"])
            .set_title("Load morph pack")
            .pick_file()
            .ok_or_else(|| "Morph pack load cancelled.".to_owned())
            .and_then(|path| {
                let pack = fs::read(&path)
                    .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
                self.activate_morph_pack(&pack)
            });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_runtime_result(result);
        }
        self.request_redraw();
    }

    fn activate_morph_pack(&mut self, pack: &[u8]) -> Result<(String, usize), String> {
        debug!("decoding morph pack: bytes={}", pack.len());
        let decoded = decode_morph_pack(pack)
            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
        let asset_id = decoded.asset.id.to_string();
        debug!(
            "morph pack decoded: asset_id={} kind={:?} attachment_mode={:?}",
            asset_id, decoded.asset.kind, decoded.attachment.mode
        );
        self.renderer
            .as_mut()
            .ok_or_else(|| "The renderer is not ready for morph registration.".to_owned())?
            .register_morph_pack(pack)
            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
        debug!("morph pack registered with renderer: asset_id={}", asset_id);
        if let Some(shell) = &mut self.shell {
            shell.upsert_morph_asset(decoded.asset.clone());
        }
        self.registered_morphs.insert(asset_id.clone());
        self.request_morph_change(wardrobe::Request::Equip(decoded.asset.id))?;
        Ok((asset_id, pack.len()))
    }

    fn apply_morph_loadout(
        &mut self,
        mut loadout: cubacadabra_morphs::MorphLoadout,
    ) -> Result<(), String> {
        debug!(
            "resolving morph loadout: base={} parts={:?}",
            loadout.base, loadout.parts
        );
        let catalog = self
            .shell
            .as_ref()
            .ok_or_else(|| "Morph shell is not ready.".to_owned())?
            .morph_catalog()
            .clone();
        loadout.revision = self.client.engine().appearance_revision().saturating_add(1);
        let capabilities = morph_application::capabilities();
        cubacadabra_morphs::resolve_loadout(&catalog, &loadout, &capabilities).map_err(
            |diagnostics| {
                let message = Self::format_morph_diagnostics(&diagnostics);
                warn!("morph loadout resolution failed: {}", message);
                message
            },
        )?;
        let appearance = serde_json::to_string(&loadout)
            .map_err(|error| format!("Could not encode morph loadout: {error}"))?;
        debug!(
            "applying native morph loadout: base={} parts={:?}",
            loadout.base, loadout.parts
        );
        if self
            .client
            .engine_mut()
            .set_local_morph_loadout_json(&appearance)
            == 0
        {
            error!("engine rejected native morph loadout");
            return Err("The player appearance rejected that morph loadout.".to_owned());
        }
        self.morph_loadout = loadout;
        debug!(
            "morph loadout applied to engine: base={} parts={:?}",
            self.morph_loadout.base, self.morph_loadout.parts
        );
        Ok(())
    }

    fn generate_morph_thumbnail(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_thumbnail_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, preview)| {
            let png = encode_morph_thumbnail_png(&preview)?;
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name(&suggested_name)
                .set_title("Generate morph thumbnail")
                .save_file()
            else {
                return Err("Thumbnail generation cancelled.".to_owned());
            };
            fs::write(&path, png)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_thumbnail_result(result);
        }
        self.request_redraw();
    }

    fn format_morph_diagnostics(diagnostics: &[cubacadabra_morphs::MorphDiagnostic]) -> String {
        let summary = diagnostics
            .iter()
            .take(3)
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("; ");
        if diagnostics.len() > 3 {
            format!("{summary}; and {} more", diagnostics.len() - 3)
        } else {
            summary
        }
    }

    fn pointer_event(&mut self, phase: u8, x: f32, y: f32) -> bool {
        self.client.ui_pointer_event(1, phase, x, y)
    }

    fn drain_ui_events(&mut self) {
        while let Some(source) = self.client.poll_ui_event_json() {
            let Ok(event) = serde_json::from_slice::<Value>(&source) else {
                continue;
            };
            let action = event
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let phase = event
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match action {
                "player.move" => {
                    let x = event.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    let y = event.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    self.joystick_input = (x, y);
                }
                "player.jump" if phase == "activate" => self.jump_queued = true,
                "player.run" if phase == "activate" => self.mobile_sprint = !self.mobile_sprint,
                "player.climb" if phase == "activate" => self.climb = !self.climb,
                _ => {}
            }
        }
    }

    fn handle_key(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        match event.state {
            ElementState::Pressed => {
                if code == KeyCode::Escape {
                    event_loop.exit();
                    return;
                }
                if code == KeyCode::Space && !event.repeat {
                    self.jump_queued = true;
                }
                self.pressed_keys.insert(code);
            }
            ElementState::Released => {
                self.pressed_keys.remove(&code);
            }
        }
    }

    fn handle_cursor_move(&mut self, x: f64, y: f64) {
        let scale = self.window.as_ref().map_or(1.0, Window::scale_factor) as f32;
        let logical = (x as f32 / scale, y as f32 / scale);
        if let Some(previous) = self.pointer_position {
            if (self.pointer_active || self.camera_pointer_active) && !self.ui_pointer_active {
                self.look_delta.0 += logical.0 - previous.0;
                self.look_delta.1 += logical.1 - previous.1;
            }
        }
        if self.movement_pointer_active
            && let Some(origin) = self.movement_pointer_origin
        {
            const JOYSTICK_RADIUS: f32 = 72.0;
            self.joystick_input = (
                ((logical.0 - origin.0) / JOYSTICK_RADIUS).clamp(-1.0, 1.0),
                ((logical.1 - origin.1) / JOYSTICK_RADIUS).clamp(-1.0, 1.0),
            );
        }
        self.pointer_position = Some(logical);
        if self.ui_pointer_active {
            if let Some((local_x, local_y)) = self.runtime_pointer(logical.0, logical.1, false) {
                self.pointer_event(1, local_x, local_y);
            }
        }
    }

    fn runtime_pointer(&self, x: f32, y: f32, require_inside: bool) -> Option<(f32, f32)> {
        let viewport = self.shell.as_ref()?.runtime_viewport();
        if !viewport.is_positive() || (require_inside && !viewport.contains(egui::pos2(x, y))) {
            return None;
        }
        Some((x - viewport.min.x, y - viewport.min.y))
    }

    fn handle_mouse_button(&mut self, state: ElementState, button: MouseButton) {
        let Some((x, y)) = self.pointer_position else {
            return;
        };
        let morph_preview = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::is_morphs_workspace);
        match state {
            ElementState::Pressed => {
                let Some((local_x, local_y)) = self.runtime_pointer(x, y, true) else {
                    return;
                };
                let camera_side = morph_preview
                    && self
                        .shell
                        .as_ref()
                        .is_some_and(|shell| local_x >= shell.runtime_viewport().width() * 0.5);
                match button {
                    MouseButton::Left if morph_preview && camera_side => {
                        self.camera_pointer_active = true;
                        self.pointer_active = false;
                        self.movement_pointer_active = false;
                        self.movement_pointer_origin = None;
                        self.joystick_input = (0.0, 0.0);
                        self.ui_pointer_active = false;
                    }
                    MouseButton::Left if morph_preview => {
                        self.movement_pointer_active = true;
                        self.movement_pointer_origin = Some((x, y));
                        self.joystick_input = (0.0, 0.0);
                        self.pointer_active = false;
                        self.ui_pointer_active = false;
                    }
                    MouseButton::Left => {
                        self.ui_pointer_active = self.pointer_event(0, local_x, local_y);
                        self.pointer_active = !self.ui_pointer_active;
                    }
                    MouseButton::Right => {
                        self.camera_pointer_active = true;
                        self.pointer_active = false;
                    }
                    _ => {}
                }
            }
            ElementState::Released => match button {
                MouseButton::Left if self.movement_pointer_active => {
                    self.movement_pointer_active = false;
                    self.movement_pointer_origin = None;
                    self.joystick_input = (0.0, 0.0);
                }
                MouseButton::Left if morph_preview && self.camera_pointer_active => {
                    self.camera_pointer_active = false;
                }
                MouseButton::Left => {
                    if self.ui_pointer_active {
                        if let Some((local_x, local_y)) = self.runtime_pointer(x, y, false) {
                            self.pointer_event(2, local_x, local_y);
                        }
                    }
                    self.ui_pointer_active = false;
                    self.pointer_active = false;
                }
                MouseButton::Right => self.camera_pointer_active = false,
                _ => {}
            },
        }
    }

    fn drain_backend_events(&mut self) {
        while let Some(event) = self.network.try_recv() {
            match event {
                BackendEvent::Connected => self.client.transport_connected(),
                BackendEvent::Disconnected => self.client.transport_disconnected(),
                BackendEvent::Message(source) => {
                    let _ = self.client.receive_text(&source);
                }
                BackendEvent::MorphCatalog(source) => self.install_published_morphs(&source),
                BackendEvent::MorphCatalogError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_catalog_error(message);
                        shell.set_notice("Morph catalog unavailable".into());
                    }
                }
                BackendEvent::MorphPacks { request_id, packs } => {
                    if let Err(message) = self.finish_morph_change(request_id, packs) {
                        if let Some(shell) = &mut self.shell {
                            shell.set_notice(message);
                        }
                    }
                }
                BackendEvent::MorphPacksError {
                    request_id,
                    message,
                } => {
                    if self
                        .pending_morph
                        .as_ref()
                        .is_some_and(|(serial, _)| *serial == request_id)
                    {
                        self.pending_morph = None;
                        if let Some(shell) = &mut self.shell {
                            shell.set_morph_loading(false);
                            shell.set_notice(format!("Appearance unchanged: {message}"));
                        }
                    }
                }
                BackendEvent::MorphThumbnail { url, bytes } => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_morph_thumbnail(url, &bytes);
                    }
                }
                BackendEvent::AuthStarted => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_notice(
                            "Finish signing in in your browser. Studio will continue automatically."
                                .to_owned(),
                        );
                    }
                }
                BackendEvent::AuthCompleted { user } => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_auth_completed(user);
                    }
                }
                BackendEvent::AuthError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_auth_error(message);
                    }
                }
            }
        }
    }

    fn drain_codex_events(&mut self) {
        while let Some(event) = self.codex.try_recv() {
            match event {
                CodexEvent::AccountStatus(account) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_account(account);
                    }
                }
                CodexEvent::BrowserOpened => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_browser_opened();
                    }
                }
                CodexEvent::LoginCompleted(account) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_connected(account);
                    }
                }
                CodexEvent::ChatReady => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_ready();
                    }
                }
                CodexEvent::WorkStatus(status) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_work_status(status);
                    }
                }
                CodexEvent::AssistantDelta(delta) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_delta(delta);
                    }
                }
                CodexEvent::AssistantMessage(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_message(message);
                    }
                }
                CodexEvent::ChatTurnCompleted => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_completed();
                        shell.set_notice("Change received — rebuilding preview…".to_owned());
                    }
                    let changes = self.capture_codex_changes();
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_changes(
                            changes
                                .iter()
                                .map(|change| change.relative_path.display().to_string())
                                .collect(),
                        );
                    }
                    if self
                        .shell
                        .as_ref()
                        .is_some_and(StudioShell::project_is_dirty)
                    {
                        self.save_project_source();
                    }
                    self.refresh_authored_manifest_from_disk();
                    self.start_codex_project_reload();
                }
                CodexEvent::ChatTurnCancelled => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_cancelled();
                        shell.set_notice(
                            "Codex stopped. The preview was not rebuilt; review or undo the changes."
                                .to_owned(),
                        );
                    }
                    let changes = self.capture_codex_changes();
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_changes(
                            changes
                                .iter()
                                .map(|change| change.relative_path.display().to_string())
                                .collect(),
                        );
                    }
                }
                CodexEvent::ChatError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_error(message);
                    }
                }
                CodexEvent::Error(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_error(message);
                    }
                }
                CodexEvent::Unavailable(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_unavailable(message);
                    }
                }
            }
        }
    }

    fn dispatch_client_actions(&mut self) {
        for action in self.client.poll_actions() {
            if self.standalone_preview {
                continue;
            }
            match action {
                ClientAction::SetWorld(world_id) => self.network.set_world(world_id),
                ClientAction::SendText(source) => self.network.send(source),
            }
        }
    }
}

impl Drop for StudioApp {
    fn drop(&mut self) {
        if let Some(package) = self.temporary_package.take() {
            let _ = fs::remove_dir_all(package);
        }
    }
}

fn load_project_in_background(
    project: &Path,
    mut progress: impl FnMut(f32),
) -> Result<BackgroundProjectLoad, String> {
    project_manifest(project)?;
    progress(0.04);
    let sources =
        load_game_sources(Some(project.to_path_buf())).map_err(|error| error.to_string())?;
    progress(0.44);

    let prepared = (|| {
        let image_atlas = load_image_atlas_with_progress(
            &sources.root,
            &sources.manifest_source,
            |image_progress| progress(0.44 + image_progress * 0.30),
        )
        .map_err(|error| error.to_string())?;
        progress(0.76);
        let morph_catalog_path = discover_project_morph_catalog(&sources.project_root);
        let local_morph_catalog = morph_catalog_path
            .as_deref()
            .map(load_local_morph_catalog)
            .transpose()
            .map_err(|error| error.to_string())?;
        progress(0.88);
        Ok::<_, String>((image_atlas, local_morph_catalog))
    })();

    match prepared {
        Ok((image_atlas, local_morph_catalog)) => Ok(BackgroundProjectLoad {
            sources,
            image_atlas,
            local_morph_catalog,
        }),
        Err(error) => {
            if let Some(package) = &sources.temporary_package {
                let _ = fs::remove_dir_all(package);
            }
            Err(error)
        }
    }
}

fn remove_temporary_package(sources: &GameSources) {
    if let Some(package) = &sources.temporary_package {
        let _ = fs::remove_dir_all(package);
    }
}

fn load_game_sources(game_root: Option<PathBuf>) -> Result<GameSources, Box<dyn Error>> {
    let Some(game_root) = game_root else {
        return Ok(GameSources {
            project_root: PathBuf::from(STANDALONE_PREVIEW_ROOT),
            root: PathBuf::from(STANDALONE_PREVIEW_ROOT),
            authored_manifest_source: STANDALONE_PREVIEW_MANIFEST.to_owned(),
            manifest_source: STANDALONE_PREVIEW_MANIFEST.to_owned(),
            script_source: STANDALONE_PREVIEW_SCRIPT.to_owned(),
            standalone_preview: true,
            temporary_package: None,
        });
    };
    let authored_manifest_source = read_utf8_file(&game_root.join("manifest.json"), "manifest")?;
    let (package_root, temporary_package) = if game_root.join("game.luau").is_file() {
        (game_root.clone(), None)
    } else if game_root.join("src/main.luau").is_file() {
        let package = build_raw_game_package(&game_root)?;
        (package.clone(), Some(package))
    } else {
        return Err(Box::new(StudioError(format!(
            "{} is neither a built package nor a raw game project (expected game.luau or src/main.luau)",
            game_root.display()
        ))));
    };

    Ok(GameSources {
        project_root: game_root,
        root: package_root.clone(),
        authored_manifest_source,
        manifest_source: read_utf8_file(&package_root.join("manifest.json"), "manifest")?,
        script_source: read_utf8_file(&package_root.join("game.luau"), "script")?,
        standalone_preview: false,
        temporary_package,
    })
}

fn build_raw_game_package(game_root: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let package = std::env::temp_dir().join(format!(
        "cubacadabra-studio-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let mut command = std::process::Command::new("cubacadabra");
    command.args([
        "build-game",
        "--source",
        game_root.to_str().ok_or_else(|| {
            Box::new(StudioError("game path is not valid UTF-8".to_owned())) as Box<dyn Error>
        })?,
        "--output",
        package.to_str().ok_or_else(|| {
            Box::new(StudioError(
                "temporary package path is not valid UTF-8".to_owned(),
            )) as Box<dyn Error>
        })?,
    ]);
    let result = command.output().or_else(|_| {
        let tools_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/src");
        std::process::Command::new("python3")
            .env("PYTHONPATH", tools_source)
            .args([
                "-m",
                "cubacadabra",
                "build-game",
                "--source",
                game_root.to_str().unwrap_or_default(),
                "--output",
                package.to_str().unwrap_or_default(),
            ])
            .output()
    })?;
    if !result.status.success() {
        let details = String::from_utf8_lossy(&result.stderr);
        return Err(Box::new(StudioError(format!(
            "could not build raw game project with cubacadabra: {}",
            details.trim()
        ))));
    }
    Ok(package)
}

fn read_utf8_file(path: &Path, kind: &str) -> Result<String, Box<dyn Error>> {
    fs::read_to_string(path).map_err(|error| {
        Box::new(StudioError(format!(
            "could not read {kind} file {}: {error}",
            path.display()
        ))) as Box<dyn Error>
    })
}

fn project_manifest(project: &Path) -> Result<PathBuf, String> {
    if !project.is_dir() {
        return Err(format!("{} is not a directory.", project.display()));
    }
    let manifest = project.join("manifest.json");
    if !manifest.is_file() {
        return Err(format!(
            "Choose a project folder containing manifest.json. No manifest was found in {}.",
            project.display()
        ));
    }
    Ok(manifest)
}

fn discover_project_morph_catalog(project_root: &Path) -> Option<PathBuf> {
    let path = project_root.join("assets/characters/catalog.json");
    path.is_file().then_some(path)
}

fn rewrite_manifest_geometry_file(source: &str, geometry_file: &str) -> Result<String, String> {
    let mut root: Value = serde_json::from_str(source)
        .map_err(|error| format!("the generated morph sidecar is invalid JSON: {error}"))?;
    root.get_mut("geometry")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "the generated morph sidecar has no geometry object".to_owned())?
        .insert("file".to_owned(), Value::String(geometry_file.to_owned()));
    root.get_mut("asset")
        .and_then(Value::as_object_mut)
        .and_then(|asset| asset.get_mut("source"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "the generated morph sidecar has no asset source object".to_owned())?
        .insert(
            "geometry".to_owned(),
            Value::String(geometry_file.to_owned()),
        );
    serde_json::to_string_pretty(&root)
        .map_err(|error| format!("could not serialize the morph sidecar: {error}"))
}

fn project_asset_slug(asset_id: &str) -> Result<String, String> {
    let mut slug = String::new();
    for byte in asset_id.bytes() {
        if byte.is_ascii_alphanumeric() {
            slug.push(char::from(byte).to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() || slug.len() > 96 {
        return Err("the character asset ID cannot become a safe project folder name".to_owned());
    }
    Ok(slug)
}

enum SceneEditOperation {
    Update { position: [f32; 3], size: [f32; 3] },
    Duplicate,
    Delete,
}

fn parse_block_target(target: &str) -> Result<(String, usize), String> {
    let mut parts = target.split('/');
    let kind = parts.next();
    let world = parts.next();
    let collection = parts.next();
    let index = parts.next();
    if kind != Some("world") || collection != Some("blocks") || parts.next().is_some() {
        return Err(format!("`{target}` is not an editable platform"));
    }
    let world = world
        .filter(|world| !world.is_empty())
        .ok_or_else(|| format!("`{target}` has no world"))?;
    let index = index
        .ok_or_else(|| format!("`{target}` has no block index"))?
        .parse::<usize>()
        .map_err(|_| format!("`{target}` has an invalid block index"))?;
    Ok((world.to_owned(), index))
}

fn update_manifest_sign_text(
    manifest: &mut Value,
    target: &str,
    text: String,
) -> Result<(), String> {
    let mut parts = target.split('/');
    let kind = parts.next();
    let world = parts.next();
    let collection = parts.next();
    let index = parts.next();
    if kind != Some("world") || collection != Some("signs") || parts.next().is_some() {
        return Err(format!("`{target}` is not an editable sign"));
    }
    let world = world
        .filter(|world| !world.is_empty())
        .ok_or_else(|| format!("`{target}` has no world"))?;
    let index = index
        .ok_or_else(|| format!("`{target}` has no sign index"))?
        .parse::<usize>()
        .map_err(|_| format!("`{target}` has an invalid sign index"))?;
    let world_definition = if world == "lobby" {
        manifest
    } else {
        manifest
            .get_mut("worlds")
            .and_then(Value::as_object_mut)
            .and_then(|worlds| worlds.get_mut(world))
            .ok_or_else(|| format!("scene world `{world}` was not found"))?
    };
    let signs = world_definition
        .get_mut("signs")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("scene world `{world}` has no signs"))?;
    let sign = signs
        .get_mut(index)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("scene sign `{target}` was not found"))?;
    sign.insert("text".to_owned(), Value::String(text));
    Ok(())
}

fn snapshot_project_files(root: &Path) -> Result<ProjectFileSnapshot, String> {
    fn visit(root: &Path, directory: &Path, files: &mut ProjectFileSnapshot) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?
        {
            let entry =
                entry.map_err(|error| format!("could not inspect project file: {error}"))?;
            let path = entry.path();
            let file_name = entry.file_name();
            if matches!(file_name.to_str(), Some(".git" | "target")) {
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
            if file_type.is_dir() {
                visit(root, &path, files)?;
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| format!("could not relativize {}: {error}", path.display()))?
                    .to_path_buf();
                let bytes = fs::read(&path)
                    .map_err(|error| format!("could not read {}: {error}", path.display()))?;
                files.insert(relative, bytes);
            }
        }
        Ok(())
    }

    let mut files = ProjectFileSnapshot::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn diff_project_files(
    before: ProjectFileSnapshot,
    after: ProjectFileSnapshot,
) -> Vec<CodexFileChange> {
    let paths = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    paths
        .into_iter()
        .filter_map(|relative_path| {
            let before_bytes = before.get(&relative_path).cloned();
            let after_bytes = after.get(&relative_path).cloned();
            (before_bytes != after_bytes).then_some(CodexFileChange {
                relative_path,
                before: before_bytes,
                after: after_bytes,
            })
        })
        .collect()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("file has no parent directory: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("file has no safe name: {}", path.display()))?;
    let temporary = parent.join(format!(".{file_name}.studio-{}", std::process::id()));
    fs::write(&temporary, bytes)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(path).map_err(|remove_error| {
                format!("could not replace {}: {remove_error}", path.display())
            })?;
            fs::rename(&temporary, path).map_err(|rename_error| {
                format!("could not replace {}: {rename_error}", path.display())
            })
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(format!("could not finalize {}: {error}", path.display()))
        }
    }
}

fn update_project_morph_catalog(path: &Path, asset_id: &str, source: &str) -> Result<(), String> {
    let mut catalog: Value = if path.is_file() {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        serde_json::from_str(&source)
            .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))?
    } else {
        serde_json::json!({ "schemaVersion": 1, "assets": [] })
    };
    if catalog.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err(format!(
            "{} must use morph catalog schema 1",
            path.display()
        ));
    }
    let assets = catalog
        .get_mut("assets")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("{} must contain an assets array", path.display()))?;
    assets.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(asset_id));
    assets.push(serde_json::json!({ "id": asset_id, "source": source }));
    let serialized = serde_json::to_string_pretty(&catalog)
        .map_err(|error| format!("could not serialize {}: {error}", path.display()))?;
    write_atomic(path, serialized.as_bytes())
}

fn load_local_morph_catalog(path: &Path) -> Result<LocalMorphCatalog, Box<dyn Error>> {
    let catalog_source = read_utf8_file(path, "morph catalog")?;
    let definition: LocalMorphCatalogFile =
        serde_json::from_str(&catalog_source).map_err(|error| {
            Box::new(StudioError(format!(
                "local morph catalog {} is not valid JSON: {error}",
                path.display()
            ))) as Box<dyn Error>
        })?;
    if definition.schema_version != 1 {
        return Err(Box::new(StudioError(format!(
            "unsupported local morph catalog schema {}; expected 1",
            definition.schema_version
        ))));
    }

    let catalog_root = path.parent().ok_or_else(|| {
        Box::new(StudioError(format!(
            "local morph catalog has no parent directory: {}",
            path.display()
        ))) as Box<dyn Error>
    })?;
    let mut assets = Vec::new();
    let builtins_source = if let Some(builtins) = definition.builtins.as_deref() {
        let builtins_path = local_catalog_file(catalog_root, builtins, "built-in catalog")?;
        read_utf8_file(&builtins_path, "built-in morph catalog")?
    } else {
        include_str!("../../rust/assets/characters/morph_catalog.json").to_owned()
    };
    let builtins = cubacadabra_morphs::parse_catalog(&builtins_source).map_err(|diagnostics| {
        Box::new(StudioError(format!(
            "built-in morph catalog is invalid: {}",
            StudioApp::format_morph_diagnostics(&diagnostics)
        ))) as Box<dyn Error>
    })?;
    assets.extend(
        builtins
            .assets
            .into_iter()
            .filter(|asset| !definition.exclude_builtin_kinds.contains(&asset.kind)),
    );

    let mut packs = BTreeMap::new();
    for local_asset in definition.assets {
        let sidecar_path = local_catalog_file(catalog_root, &local_asset.source, "asset sidecar")?;
        let sidecar_source = read_utf8_file(&sidecar_path, "morph sidecar")?;
        let asset = source_manifest_asset(&sidecar_source).map_err(|diagnostics| {
            Box::new(StudioError(format!(
                "morph sidecar {} is invalid: {}",
                sidecar_path.display(),
                StudioApp::format_morph_diagnostics(&diagnostics)
            ))) as Box<dyn Error>
        })?;
        if asset.id != local_asset.id {
            return Err(Box::new(StudioError(format!(
                "local morph catalog asset {} points to {}, which declares {}",
                local_asset.id,
                sidecar_path.display(),
                asset.id
            ))));
        }
        let geometry_file =
            source_manifest_geometry_file(&sidecar_source).map_err(|diagnostics| {
                Box::new(StudioError(format!(
                    "morph sidecar {} has invalid geometry: {}",
                    sidecar_path.display(),
                    StudioApp::format_morph_diagnostics(&diagnostics)
                ))) as Box<dyn Error>
            })?;
        let geometry_path = local_catalog_file(
            sidecar_path.parent().ok_or_else(|| {
                Box::new(StudioError(format!(
                    "morph sidecar has no parent directory: {}",
                    sidecar_path.display()
                ))) as Box<dyn Error>
            })?,
            &geometry_file,
            "morph geometry",
        )?;
        let geometry = fs::read(&geometry_path).map_err(|error| {
            Box::new(StudioError(format!(
                "could not read morph geometry {}: {error}",
                geometry_path.display()
            ))) as Box<dyn Error>
        })?;
        let (pack, summary) =
            compile_source_morph_pack(&sidecar_source, &geometry).map_err(|diagnostics| {
                Box::new(StudioError(format!(
                    "could not compile local morph {}: {}",
                    local_asset.id,
                    StudioApp::format_morph_diagnostics(&diagnostics)
                ))) as Box<dyn Error>
            })?;
        if summary.asset_id != local_asset.id.as_str() {
            return Err(Box::new(StudioError(format!(
                "compiled local morph has unexpected ID {} (expected {})",
                summary.asset_id, local_asset.id
            ))));
        }
        assets.retain(|existing| existing.id != asset.id);
        assets.push(asset);
        packs.insert(local_asset.id, pack);
    }

    let mut presets = Vec::new();
    let mut thumbnails = BTreeMap::new();
    for local_preset in definition.presets {
        let preset_path = local_catalog_file(catalog_root, &local_preset.source, "preset")?;
        let preset_source = read_utf8_file(&preset_path, "morph preset")?;
        let preset: cubacadabra_morphs::MorphPreset = serde_json::from_str(&preset_source)
            .map_err(|error| {
                Box::new(StudioError(format!(
                    "morph preset {} is not valid: {error}",
                    preset_path.display()
                ))) as Box<dyn Error>
            })?;
        if let Some(thumbnail) = preset.thumbnail.as_deref() {
            let thumbnail_path = preset_path
                .parent()
                .ok_or_else(|| {
                    Box::new(StudioError(
                        "local preset has no parent directory".to_owned(),
                    )) as Box<dyn Error>
                })?
                .join(thumbnail);
            if Path::new(thumbnail).is_absolute() || !thumbnail_path.is_file() {
                return Err(Box::new(StudioError(format!(
                    "local morph thumbnail does not exist: {}",
                    thumbnail_path.display()
                ))));
            }
            thumbnails.insert(
                thumbnail.to_owned(),
                fs::read(&thumbnail_path).map_err(|error| {
                    Box::new(StudioError(format!(
                        "could not read local morph thumbnail {}: {error}",
                        thumbnail_path.display()
                    ))) as Box<dyn Error>
                })?,
            );
        }
        presets.push(preset);
    }

    let catalog = cubacadabra_morphs::MorphCatalog {
        schema_version: cubacadabra_morphs::MORPH_CATALOG_SCHEMA_VERSION,
        content_version: "local-development".to_owned(),
        assets,
        presets,
    };
    let diagnostics = catalog.validate();
    if !diagnostics.is_empty() {
        return Err(Box::new(StudioError(format!(
            "local morph catalog {} is invalid: {}",
            path.display(),
            StudioApp::format_morph_diagnostics(&diagnostics)
        ))));
    }
    let initial_preset = catalog.presets.first().map(|preset| preset.id.clone());
    log::info!(
        "loaded local morph catalog: path={} assets={} packs={} presets={}",
        path.display(),
        catalog.assets.len(),
        packs.len(),
        catalog.presets.len()
    );
    Ok(LocalMorphCatalog {
        catalog,
        packs,
        thumbnails,
        initial_preset,
    })
}

fn local_catalog_file(root: &Path, reference: &str, kind: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = root.join(reference);
    if reference.is_empty() || Path::new(reference).is_absolute() || !path.is_file() {
        return Err(Box::new(StudioError(format!(
            "local {kind} does not exist: {}",
            path.display()
        ))));
    }
    path.canonicalize().map_err(|error| {
        Box::new(StudioError(format!(
            "could not resolve local {kind} {}: {error}",
            path.display()
        ))) as Box<dyn Error>
    })
}

impl ApplicationHandler for StudioApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[cfg(target_os = "macos")]
        macos::install_native_menu();
        if let Err(error) = self.create_window(event_loop) {
            eprintln!("Cubacadabra Studio: {error}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let shell_consumed = match (&mut self.shell, &self.window) {
            (Some(shell), Some(window)) => shell.on_window_event(window, &event),
            _ => false,
        };
        // Once Play is active, the game owns its keyboard controls even if
        // egui still reports that it wants keyboard input. This can happen
        // after the editor's search field or another shell control had focus;
        // letting that stale focus consume W/A/S/D, arrows, Shift, or Space
        // makes the running game appear completely unresponsive.
        let playing = self
            .shell
            .as_ref()
            .is_some_and(|shell| shell.is_playing() && !shell.is_project_loading());
        let runtime_hovered = self
            .pointer_position
            .is_some_and(|(x, y)| self.runtime_pointer(x, y, true).is_some());
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => self.resize(
                self.window
                    .as_ref()
                    .map_or(PhysicalSize::new(0, 0), Window::inner_size),
            ),
            WindowEvent::RedrawRequested => {
                self.render();
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. }
                if should_forward_gameplay_keyboard(playing, shell_consumed) =>
            {
                self.handle_key(&event, event_loop)
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_move(position.x, position.y)
            }
            WindowEvent::MouseInput { state, button, .. }
                if playing
                    || runtime_hovered
                    || !shell_consumed
                    || state == ElementState::Released =>
            {
                self.handle_mouse_button(state, button)
            }
            WindowEvent::MouseWheel { delta, .. } if runtime_hovered => {
                self.zoom_delta += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 0.9,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 100.0,
                };
            }
            WindowEvent::Focused(false) => {
                self.pressed_keys.clear();
                self.pointer_active = false;
                self.camera_pointer_active = false;
                self.movement_pointer_active = false;
                self.movement_pointer_origin = None;
                self.joystick_input = (0.0, 0.0);
                if self.ui_pointer_active {
                    self.pointer_event(3, 0.0, 0.0);
                }
                self.ui_pointer_active = false;
            }
            _ => {}
        }
    }
}

fn axis(keys: &HashSet<KeyCode>, positive: &[KeyCode], negative: &[KeyCode]) -> f32 {
    f32::from(positive.iter().any(|key| keys.contains(key)))
        - f32::from(negative.iter().any(|key| keys.contains(key)))
}

fn joystick_movement((x, y): (f32, f32)) -> (f32, f32) {
    (-y, x)
}

fn should_forward_gameplay_keyboard(playing: bool, shell_consumed: bool) -> bool {
    playing || !shell_consumed
}

fn game_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("game")
        .to_owned()
}

fn load_image_atlas(
    root: &Path,
    manifest_source: &str,
) -> Result<Option<ImageAtlas>, Box<dyn Error>> {
    load_image_atlas_with_progress(root, manifest_source, |_| {})
}

fn load_image_atlas_with_progress(
    root: &Path,
    manifest_source: &str,
    mut progress: impl FnMut(f32),
) -> Result<Option<ImageAtlas>, Box<dyn Error>> {
    let manifest: Value = serde_json::from_str(manifest_source)?;
    let Some(images) = manifest
        .get("assets")
        .and_then(|assets| assets.get("images"))
        .and_then(Value::as_object)
    else {
        progress(1.0);
        return Ok(None);
    };
    if images.is_empty() {
        progress(1.0);
        return Ok(None);
    }

    let mut loaded = Vec::with_capacity(images.len());
    for (index, (id, definition)) in images.iter().enumerate() {
        let relative = definition
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| StudioError(format!("image asset {id:?} has no path")))?;
        let asset_path = safe_asset_path(root, relative)?;
        let mut image = image::ImageReader::open(&asset_path)?.decode()?.to_rgba8();
        let longest = image.width().max(image.height());
        if longest > MAX_ATLAS_IMAGE_DIMENSION {
            let scale = MAX_ATLAS_IMAGE_DIMENSION as f32 / longest as f32;
            image = image::imageops::resize(
                &image,
                (image.width() as f32 * scale).round().max(1.0) as u32,
                (image.height() as f32 * scale).round().max(1.0) as u32,
                FilterType::Lanczos3,
            );
        }
        loaded.push((id.clone(), image));
        progress((index + 1) as f32 / images.len() as f32 * 0.72);
    }

    let mut placements = Vec::with_capacity(loaded.len());
    let mut x = ATLAS_PADDING;
    let mut y = ATLAS_PADDING;
    let mut row_height = 0;
    for (id, image) in &loaded {
        if image.width() + ATLAS_PADDING * 2 > MAX_ATLAS_DIMENSION
            || image.height() + ATLAS_PADDING * 2 > MAX_ATLAS_DIMENSION
        {
            return Err(Box::new(StudioError(format!(
                "image asset {id:?} is too large for the world atlas"
            ))));
        }
        if x + image.width() + ATLAS_PADDING > MAX_ATLAS_DIMENSION {
            x = ATLAS_PADDING;
            y += row_height + ATLAS_PADDING;
            row_height = 0;
        }
        if y + image.height() + ATLAS_PADDING > MAX_ATLAS_DIMENSION {
            return Err(Box::new(StudioError(
                "the game's images do not fit in a 2048px world atlas".to_owned(),
            )));
        }
        placements.push((id, x, y));
        x += image.width() + ATLAS_PADDING;
        row_height = row_height.max(image.height());
    }

    let height = next_power_of_two((y + row_height + ATLAS_PADDING).max(1));
    let mut atlas = RgbaImage::new(MAX_ATLAS_DIMENSION, height.min(MAX_ATLAS_DIMENSION));
    let mut regions = std::collections::BTreeMap::new();
    for (index, ((id, image), (_, left, top))) in loaded.iter().zip(&placements).enumerate() {
        atlas.copy_from(image, *left, *top)?;
        regions.insert(
            id.clone(),
            [
                (*left as f32 + 0.5) / atlas.width() as f32,
                (*top as f32 + 0.5) / atlas.height() as f32,
                (image.width().saturating_sub(1).max(1)) as f32 / atlas.width() as f32,
                (image.height().saturating_sub(1).max(1)) as f32 / atlas.height() as f32,
            ],
        );
        progress(0.72 + (index + 1) as f32 / loaded.len() as f32 * 0.28);
    }
    Ok(Some(ImageAtlas {
        width: atlas.width(),
        height: atlas.height(),
        pixels: atlas.into_raw(),
        regions,
    }))
}

fn safe_asset_path(root: &Path, relative: &str) -> Result<PathBuf, Box<dyn Error>> {
    let relative_path = Path::new(relative);
    let is_safe = relative_path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
        && relative_path.starts_with("assets");
    if !is_safe {
        return Err(Box::new(StudioError(format!(
            "asset path is outside assets/: {relative}"
        ))));
    }
    let path = root.join(relative_path);
    if !path.is_file() {
        return Err(Box::new(StudioError(format!(
            "asset file does not exist: {}",
            path.display()
        ))));
    }
    Ok(path)
}

fn default_morph_loadout() -> cubacadabra_morphs::MorphLoadout {
    cubacadabra_morphs::MorphLoadout {
        version: cubacadabra_morphs::MORPH_LOADOUT_VERSION,
        base: cubacadabra_morphs::MorphAssetId::parse("cuba:base/person.v1")
            .expect("built-in morph base ID must be valid"),
        parts: vec![
            cubacadabra_morphs::MorphAssetId::parse("cuba:hair/swept.v1")
                .expect("built-in hair ID must be valid"),
            cubacadabra_morphs::MorphAssetId::parse("cuba:everyday-hoodie.v1")
                .expect("built-in outfit ID must be valid"),
        ],
        face: Some(
            cubacadabra_morphs::MorphAssetId::parse("cuba:face/happy.v1")
                .expect("built-in face ID must be valid"),
        ),
        parameters: BTreeMap::new(),
        revision: 0,
    }
}

fn next_power_of_two(value: u32) -> u32 {
    value.next_power_of_two().min(MAX_ATLAS_DIMENSION)
}

struct StudioOptions {
    game_path: Option<PathBuf>,
    validate_project_path: Option<PathBuf>,
    morph_catalog_path: Option<PathBuf>,
}

fn parse_options() -> Result<StudioOptions, Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let mut game_path = None;
    let mut validate_project_path = None;
    let mut morph_catalog_path = None;
    while let Some(argument) = args.next() {
        if argument == "--help" || argument == "-h" {
            println!(
                "Usage: studio [--path <game-directory>] [--validate-project <game-directory>] [--morph-catalog <catalog.json>]"
            );
            println!();
            println!(
                "Open a local Cubacadabra game package, or launch the standalone morph preview."
            );
            std::process::exit(0);
        }
        if argument == "--path" {
            game_path = Some(
                args.next()
                    .ok_or_else(|| StudioError("--path expects a game directory".to_owned()))?,
            );
        } else if argument == "--validate-project" {
            validate_project_path = Some(args.next().ok_or_else(|| {
                StudioError("--validate-project expects a game directory".to_owned())
            })?);
        } else if argument == "--morph-catalog" {
            morph_catalog_path = Some(args.next().ok_or_else(|| {
                StudioError("--morph-catalog expects a catalog JSON file".to_owned())
            })?);
        } else {
            return Err(Box::new(StudioError(format!(
                "unknown argument: {}",
                argument.to_string_lossy()
            ))));
        }
    }
    let game_path = game_path
        .map(PathBuf::from)
        .map(|path| path.canonicalize())
        .transpose()?;
    if let Some(path) = &game_path
        && !path.is_dir()
    {
        return Err(Box::new(StudioError(format!(
            "game path is not a directory: {}",
            path.display()
        ))));
    }
    let validate_project_path = validate_project_path
        .map(PathBuf::from)
        .map(|path| path.canonicalize())
        .transpose()?;
    if let Some(path) = &validate_project_path
        && !path.is_dir()
    {
        return Err(Box::new(StudioError(format!(
            "project path is not a directory: {}",
            path.display()
        ))));
    }
    let morph_catalog_path = morph_catalog_path
        .map(PathBuf::from)
        .map(|path| path.canonicalize())
        .transpose()?;
    if let Some(path) = &morph_catalog_path
        && !path.is_file()
    {
        return Err(Box::new(StudioError(format!(
            "morph catalog is not a file: {}",
            path.display()
        ))));
    }
    Ok(StudioOptions {
        game_path,
        validate_project_path,
        morph_catalog_path,
    })
}

fn validate_project(game_path: PathBuf) -> Result<(), Box<dyn Error>> {
    let sources = load_game_sources(Some(game_path.clone()))?;
    let client = ClientSession::load(&sources.manifest_source, &sources.script_source)?;
    println!(
        "Validated {} ({}) through the shared game builder.",
        game_path.display(),
        client.game_id()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ProjectFileSnapshot, STANDALONE_PREVIEW_MANIFEST, diff_project_files, joystick_movement,
        load_game_sources, load_local_morph_catalog, load_project_in_background,
        project_asset_slug, project_manifest, should_forward_gameplay_keyboard,
        update_manifest_sign_text, update_project_morph_catalog,
    };
    use crate::game_creator;
    use std::{fs, path::Path};

    #[test]
    fn standalone_sources_load_as_a_default_person_preview() {
        let sources = load_game_sources(None).expect("standalone sources");
        assert!(sources.standalone_preview);
        let client = cubacadabra_client::ClientSession::load(
            &sources.manifest_source,
            &sources.script_source,
        )
        .expect("standalone client");
        assert_eq!(client.game_id(), "studio-morph-preview");
        assert_eq!(client.engine().active_world_id(), Some("lobby"));
    }

    #[test]
    fn raw_game_projects_load_through_the_shared_builder() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../first-game");
        let sources = load_game_sources(Some(path)).expect("raw game project");
        let client = cubacadabra_client::ClientSession::load(
            &sources.manifest_source,
            &sources.script_source,
        )
        .expect("built raw project");
        assert_eq!(client.game_id(), "first-game");
        assert!(sources.script_source.contains("begin module: round.luau"));
        assert!(!sources.script_source.contains("@include"));
    }

    #[test]
    fn generated_starter_builds_through_the_shared_builder() {
        let root = std::env::temp_dir().join(format!(
            "cubacadabra-studio-starter-build-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let created = game_creator::create_game("Jump Course", &root).expect("starter project");
        let sources = load_game_sources(Some(created.project.clone())).expect("built starter");
        let client = cubacadabra_client::ClientSession::load(
            &sources.manifest_source,
            &sources.script_source,
        )
        .expect("starter package should compile");
        assert_eq!(client.game_id(), "jump-course");
        if let Some(package) = sources.temporary_package {
            let _ = fs::remove_dir_all(package);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sign_text_edit_preserves_the_authored_sign_shape() {
        let mut manifest = serde_json::json!({
            "worlds": {
                "course": {
                    "signs": [{
                        "text": "Before",
                        "position": [1, 2, 3],
                        "yaw": 0.5,
                        "maxWidth": 4.0,
                        "color": "paper"
                    }]
                }
            }
        });
        update_manifest_sign_text(&mut manifest, "world/course/signs/0", "After".to_owned())
            .expect("sign should be editable");
        let sign = &manifest["worlds"]["course"]["signs"][0];
        assert_eq!(sign["text"], "After");
        assert_eq!(sign["position"], serde_json::json!([1, 2, 3]));
        assert_eq!(sign["yaw"], 0.5);
        assert_eq!(sign["maxWidth"], 4.0);
        assert_eq!(sign["color"], "paper");
    }

    #[test]
    fn codex_change_diff_tracks_created_deleted_and_updated_files() {
        let mut before = ProjectFileSnapshot::new();
        before.insert("src/main.luau".into(), b"old".to_vec());
        before.insert("manifest.json".into(), b"same".to_vec());
        before.insert("src/old-ui.luau".into(), b"deleted".to_vec());
        let mut after = ProjectFileSnapshot::new();
        after.insert("src/main.luau".into(), b"new".to_vec());
        after.insert("manifest.json".into(), b"same".to_vec());
        after.insert("src/ui.luau".into(), b"created".to_vec());
        let changes = diff_project_files(before, after);
        assert_eq!(
            changes
                .iter()
                .map(|change| change.relative_path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["src/main.luau", "src/old-ui.luau", "src/ui.luau"]
        );
    }

    #[test]
    fn project_folders_require_a_manifest() {
        let valid = Path::new(env!("CARGO_MANIFEST_DIR")).join("../first-game");
        assert_eq!(
            project_manifest(&valid).unwrap(),
            valid.join("manifest.json")
        );

        let missing = std::env::temp_dir().join(format!(
            "cubacadabra-studio-no-manifest-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&missing);
        fs::create_dir(&missing).unwrap();
        let error = project_manifest(&missing).unwrap_err();
        assert!(error.contains("containing manifest.json"));
        let _ = fs::remove_dir(missing);
    }

    #[test]
    fn background_project_loader_reports_monotonic_real_phases() {
        let project = std::env::temp_dir().join(format!(
            "cubacadabra-studio-progress-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("manifest.json"), STANDALONE_PREVIEW_MANIFEST).unwrap();
        fs::write(project.join("game.luau"), "return {}").unwrap();

        let mut progress = Vec::new();
        let result = load_project_in_background(&project, |value| progress.push(value));
        assert!(result.is_ok());
        assert!(progress.windows(2).all(|values| values[0] <= values[1]));
        assert_eq!(progress.first().copied(), Some(0.04));
        assert_eq!(progress.last().copied(), Some(0.88));
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn local_study_catalog_compiles_current_source_assets() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/starter-set/studies/mockup-person/catalog.json");
        let local = load_local_morph_catalog(&path).expect("mockup-person local catalog");
        assert_eq!(
            local.initial_preset.as_ref().unwrap().as_str(),
            "cuba:preset/mockup-person.v1"
        );
        assert_eq!(local.packs.len(), 5);
        assert_eq!(
            local
                .catalog
                .asset(
                    &cubacadabra_morphs::MorphAssetId::parse("cuba:base/study-person.v1").unwrap()
                )
                .unwrap()
                .display_name,
            "Studio study base"
        );
    }

    #[test]
    fn local_starter_catalog_loads_runtime_thumbnails() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/starter-set/catalog.json");
        let local = load_local_morph_catalog(&path).expect("starter catalog");
        assert_eq!(local.catalog.presets.len(), 24);
        assert_eq!(local.thumbnails.len(), 24);
        assert!(
            local
                .thumbnails
                .get("thumbnails/person-05.png")
                .is_some_and(|bytes| bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
        );
    }

    #[test]
    fn project_catalog_upserts_a_stable_source_reference() {
        let root = std::env::temp_dir().join(format!(
            "cubacadabra-studio-catalog-test-{}",
            std::process::id()
        ));
        let path = root.join("assets/characters/catalog.json");
        let _ = fs::remove_dir_all(&root);
        update_project_morph_catalog(
            &path,
            "cuba:headwear/test-hat.v1",
            "test-hat/source.morph.json",
        )
        .expect("project catalog should be written");
        update_project_morph_catalog(
            &path,
            "cuba:headwear/test-hat.v1",
            "test-hat/source.morph.json",
        )
        .expect("project catalog should upsert");
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["assets"].as_array().unwrap().len(), 1);
        assert_eq!(value["assets"][0]["source"], "test-hat/source.morph.json");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_asset_slugs_are_safe_and_deterministic() {
        assert_eq!(
            project_asset_slug("cuba:headwear/test-hat.v1").unwrap(),
            "cuba-headwear-test-hat-v1"
        );
    }

    #[test]
    fn play_mode_bypasses_stale_shell_keyboard_capture() {
        assert!(should_forward_gameplay_keyboard(true, true));
    }

    #[test]
    fn paused_editor_still_honors_shell_keyboard_capture() {
        assert!(!should_forward_gameplay_keyboard(false, true));
        assert!(should_forward_gameplay_keyboard(false, false));
    }

    #[test]
    fn preview_drag_preserves_screen_space_movement_directions() {
        assert_eq!(joystick_movement((1.0, 0.0)), (0.0, 1.0));
        assert_eq!(joystick_movement((-1.0, 0.0)), (0.0, -1.0));
        assert_eq!(joystick_movement((0.0, -1.0)), (1.0, 0.0));
        assert_eq!(joystick_movement((0.0, 1.0)), (-1.0, 0.0));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    debug!("Studio debug logging initialized");
    let options = parse_options()?;
    if let Some(path) = options.validate_project_path {
        return validate_project(path);
    }
    let mut app = StudioApp::load(options.game_path, options.morph_catalog_path)?;
    let mut event_loop_builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    event_loop_builder.with_activation_policy(ActivationPolicy::Regular);
    let event_loop = event_loop_builder.build()?;
    #[cfg(target_os = "macos")]
    macos::set_application_icon();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app)?;
    Ok(())
}
