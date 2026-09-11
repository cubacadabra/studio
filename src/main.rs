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
    time::Instant,
};
#[cfg(target_os = "macos")]
mod macos;
mod morphs;
mod network;
mod shell;
use cubacadabra_morphs::decode_morph_pack;
use morphs::{
    MorphGlbPreviewMesh, compile_source_morph_pack, decode_source_glb_preview,
    decode_source_glb_preview_node, encode_morph_thumbnail_png, inspect_source_glb_structure,
    inspect_source_sidecar, is_morph_draft_json, parse_morph_draft_json,
    source_manifest_geometry_file,
};
use network::{BackendClient, BackendEvent};
use shell::{PreparedShell, StudioShell};
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
    root: PathBuf,
    manifest_source: String,
    script_source: String,
    standalone_preview: bool,
}

struct StudioApp {
    game_root: PathBuf,
    standalone_preview: bool,
    network: BackendClient,
    client: ClientSession,
    image_atlas: Option<ImageAtlas>,
    window: Option<Window>,
    renderer: Option<Renderer>,
    shell: Option<StudioShell>,
    pressed_keys: HashSet<KeyCode>,
    jump_queued: bool,
    mobile_sprint: bool,
    morph_equipment: BTreeMap<String, String>,
    morph_loadout: cubacadabra_morphs::MorphLoadout,
    morph_base_asset: Option<cubacadabra_morphs::MorphAssetId>,
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
    fn load(game_root: Option<PathBuf>) -> Result<Self, Box<dyn Error>> {
        let sources = load_game_sources(game_root)?;
        let manifest_source = sources.manifest_source;
        let script_source = sources.script_source;
        let game_root = sources.root;
        let standalone_preview = sources.standalone_preview;
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
        info!(
            "studio loaded: game_id={} standalone_preview={} root={}",
            client.game_id(),
            standalone_preview,
            game_root.display()
        );
        network.request_morph_catalog();

        Ok(Self {
            image_atlas: load_image_atlas(&game_root, &manifest_source)?,
            game_root,
            standalone_preview,
            network,
            client,
            window: None,
            renderer: None,
            shell: None,
            pressed_keys: HashSet::new(),
            jump_queued: false,
            mobile_sprint: false,
            morph_equipment: BTreeMap::new(),
            morph_loadout: default_morph_loadout(),
            morph_base_asset: None,
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

        let shell = StudioShell::new(&window, &renderer);
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.shell = Some(shell);
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
        #[cfg(target_os = "macos")]
        while let Some(command) = macos::take_menu_action() {
            if let Some(shell) = &mut self.shell {
                shell.execute_command(command);
            }
        }
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;

        let project_name = game_name(&self.game_root);
        if let Some(shell) = &mut self.shell {
            shell.set_active_morphs(self.morph_equipment.values().cloned());
        }
        let prepared_shell: Option<PreparedShell> = match (&mut self.shell, &self.window) {
            (Some(shell), Some(window)) => Some(shell.prepare(window, &project_name)),
            _ => None,
        };
        let toggle_request = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_morph_toggle_request);
        let remote_pack_request = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_remote_morph_pack_request);
        let asset_request = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_morph_asset_request);
        if let Some(asset_id) = asset_request {
            debug!(
                "morph asset selection request received: asset_id={}",
                asset_id
            );
            let result = self.select_morph_asset(&asset_id);
            if let Some(shell) = &mut self.shell {
                match result {
                    Ok(()) => shell.set_notice(format!("Applied {}", asset_id.as_str())),
                    Err(message) => shell.set_notice(message),
                }
            }
        }
        let mut skip_remote_pack = false;
        if let Some(asset_id) = toggle_request {
            debug!("morph toggle request received: asset_id={}", asset_id);
            let was_active = self
                .morph_loadout
                .parts
                .iter()
                .any(|active_id| active_id == &asset_id);
            skip_remote_pack = was_active;
            if was_active {
                debug!("morph asset is active; removing it: asset_id={}", asset_id);
                let result = self.remove_morph(&asset_id);
                if let Some(shell) = &mut self.shell {
                    match result {
                        Ok(()) => shell.set_notice(format!("Removed {}", asset_id.as_str())),
                        Err(message) => shell.set_notice(message),
                    }
                }
            }
        }
        if let Some((asset_id, url)) = remote_pack_request
            && !skip_remote_pack
        {
            debug!(
                "morph pack download request received: asset_id={} url={}",
                asset_id, url
            );
            self.network.request_morph_pack(asset_id, url);
        }
        let import_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_import_request);
        if import_requested {
            self.import_morph_glb();
        }
        let draft_export_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_draft_export_request);
        if draft_export_requested {
            self.export_morph_draft();
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
        let playing = self.shell.as_ref().is_none_or(StudioShell::is_playing);

        let mut forward = if playing {
            axis(
                &self.pressed_keys,
                &[KeyCode::KeyW, KeyCode::ArrowUp],
                &[KeyCode::KeyS, KeyCode::ArrowDown],
            )
        } else {
            0.0
        };
        let mut strafe = if playing {
            axis(
                &self.pressed_keys,
                &[KeyCode::KeyD, KeyCode::ArrowRight],
                &[KeyCode::KeyA, KeyCode::ArrowLeft],
            )
        } else {
            0.0
        };
        if playing {
            forward -= self.joystick_input.1;
            strafe += self.joystick_input.0;
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
            playing && sprint,
            playing && self.jump_queued,
            playing && self.climb,
            self.look_delta.0,
            self.look_delta.1,
            self.zoom_delta,
        );
        self.jump_queued = false;
        self.look_delta = (0.0, 0.0);
        self.zoom_delta = 0.0;
        self.dispatch_client_actions();
        self.client.step(delta);
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
            let mut definition = decoded.asset.clone();
            if definition.id.as_str() == "cuba:base/person.v1" {
                definition.display_name = "Person".to_owned();
            }
            shell.upsert_morph_asset(definition);
        }
        let mut loadout = self.morph_loadout.clone();
        if decoded.asset.kind == cubacadabra_morphs::MorphAssetKind::Base {
            self.morph_base_asset = Some(decoded.asset.id.clone());
            loadout.base = decoded.asset.id.clone();
        } else {
            let is_clothing = matches!(
                decoded.asset.kind,
                cubacadabra_morphs::MorphAssetKind::Top
                    | cubacadabra_morphs::MorphAssetKind::Outerwear
                    | cubacadabra_morphs::MorphAssetKind::Bottom
                    | cubacadabra_morphs::MorphAssetKind::Footwear
            );
            if is_clothing {
                loadout.parts.retain(|part_id| {
                    self.shell
                        .as_ref()
                        .and_then(|shell| shell.morph_asset(part_id))
                        .is_none_or(|part| part.kind != cubacadabra_morphs::MorphAssetKind::Outfit)
                });
            }
            if !loadout.parts.iter().any(|part| part == &decoded.asset.id) {
                loadout.parts.push(decoded.asset.id.clone());
            }
        }
        debug!(
            "applying loadout after morph pack activation: base={} parts={:?}",
            loadout.base, loadout.parts
        );
        self.apply_morph_loadout(loadout)?;
        Ok((asset_id, pack.len()))
    }

    fn remove_morph(&mut self, asset_id: &cubacadabra_morphs::MorphAssetId) -> Result<(), String> {
        let mut loadout = self.morph_loadout.clone();
        let before = loadout.parts.len();
        loadout.parts.retain(|active_id| active_id != asset_id);
        if loadout.parts.len() == before {
            return Err(format!("{} is not currently equipped", asset_id.as_str()));
        }
        debug!(
            "applying morph removal: asset_id={} parts={:?}",
            asset_id, loadout.parts
        );
        self.apply_morph_loadout(loadout)
    }

    fn select_morph_asset(
        &mut self,
        asset_id: &cubacadabra_morphs::MorphAssetId,
    ) -> Result<(), String> {
        let definition = self
            .shell
            .as_ref()
            .and_then(|shell| shell.morph_asset(asset_id))
            .cloned()
            .ok_or_else(|| format!("Morph {} is not in the catalog", asset_id.as_str()))?;
        debug!(
            "morph asset selected: asset_id={} kind={:?} name={}",
            definition.id, definition.kind, definition.display_name
        );
        let mut loadout = self.morph_loadout.clone();
        match definition.kind {
            cubacadabra_morphs::MorphAssetKind::Base => {
                loadout.base = definition.id.clone();
                loadout.parts.retain(|part_id| {
                    self.shell
                        .as_ref()
                        .and_then(|shell| shell.morph_asset(part_id))
                        .is_some_and(|part| {
                            part.kind != cubacadabra_morphs::MorphAssetKind::Base
                                && part.supported_bases.contains(&loadout.base)
                        })
                });
            }
            cubacadabra_morphs::MorphAssetKind::Face => loadout.face = Some(definition.id.clone()),
            cubacadabra_morphs::MorphAssetKind::Hair
            | cubacadabra_morphs::MorphAssetKind::Outfit => {
                loadout.parts.retain(|part_id| {
                    self.shell
                        .as_ref()
                        .and_then(|shell| shell.morph_asset(part_id))
                        .is_none_or(|part| {
                            part.kind != definition.kind
                                && !(definition.kind == cubacadabra_morphs::MorphAssetKind::Outfit
                                    && matches!(
                                        part.kind,
                                        cubacadabra_morphs::MorphAssetKind::Top
                                            | cubacadabra_morphs::MorphAssetKind::Outerwear
                                            | cubacadabra_morphs::MorphAssetKind::Bottom
                                            | cubacadabra_morphs::MorphAssetKind::Footwear
                                    ))
                        })
                });
                loadout.parts.push(definition.id.clone());
            }
            cubacadabra_morphs::MorphAssetKind::Top
            | cubacadabra_morphs::MorphAssetKind::Outerwear
            | cubacadabra_morphs::MorphAssetKind::Bottom
            | cubacadabra_morphs::MorphAssetKind::Footwear => {
                loadout.parts.retain(|part_id| {
                    self.shell
                        .as_ref()
                        .and_then(|shell| shell.morph_asset(part_id))
                        .is_none_or(|part| {
                            part.kind != cubacadabra_morphs::MorphAssetKind::Outfit
                                && part
                                    .occupied_slots
                                    .iter()
                                    .all(|slot| !definition.occupied_slots.contains(slot))
                        })
                });
                loadout.parts.push(definition.id.clone());
            }
            _ => {
                loadout.parts.retain(|part_id| {
                    self.shell
                        .as_ref()
                        .and_then(|shell| shell.morph_asset(part_id))
                        .is_none_or(|part| {
                            part.occupied_slots
                                .iter()
                                .all(|slot| !definition.occupied_slots.contains(slot))
                        })
                });
                loadout.parts.push(definition.id.clone());
            }
        }
        debug!(
            "applying loadout after morph selection: base={} parts={:?}",
            loadout.base, loadout.parts
        );
        let result = self.apply_morph_loadout(loadout);
        match &result {
            Ok(()) => debug!("morph asset applied: asset_id={}", asset_id),
            Err(error) => warn!(
                "morph asset application failed: asset_id={} error={}",
                asset_id, error
            ),
        }
        result
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
        let capabilities = cubacadabra_morphs::CapabilitySet::new([
            cubacadabra_morphs::CapabilityId::parse("mesh.rigid.v1")
                .expect("built-in morph capability must be valid"),
            cubacadabra_morphs::CapabilityId::parse("skin.biped15-linear.v1")
                .expect("built-in morph capability must be valid"),
            cubacadabra_morphs::CapabilityId::parse("material.cuba-pbr.v1")
                .expect("built-in morph capability must be valid"),
            cubacadabra_morphs::CapabilityId::parse("face.analytic.v1")
                .expect("built-in morph capability must be valid"),
            cubacadabra_morphs::CapabilityId::parse("secondary.chain.v1")
                .expect("built-in morph capability must be valid"),
            cubacadabra_morphs::CapabilityId::parse("material.emissive.v1")
                .expect("built-in morph capability must be valid"),
        ]);
        cubacadabra_morphs::resolve_loadout(&catalog, &loadout, &capabilities).map_err(
            |diagnostics| {
                let message = Self::format_morph_diagnostics(&diagnostics);
                warn!("morph loadout resolution failed: {}", message);
                message
            },
        )?;
        let mut legacy =
            cubacadabra_morphs::project_v2_to_v1(&catalog, &loadout).map_err(|diagnostics| {
                let message = Self::format_morph_diagnostics(&diagnostics);
                warn!("morph loadout projection failed: {}", message);
                message
            })?;
        if self.morph_base_asset.as_ref() == Some(&loadout.base) {
            legacy
                .equipment
                .insert("base".to_owned(), loadout.base.to_string());
        }
        let appearance = serde_json::to_string(&legacy)
            .map_err(|error| format!("Could not encode morph loadout: {error}"))?;
        debug!(
            "projected morph loadout to engine appearance: base={} parts={:?} equipment={:?}",
            loadout.base, loadout.parts, legacy.equipment
        );
        if self
            .client
            .engine_mut()
            .set_local_appearance_json(&appearance)
            == 0
        {
            error!("engine rejected projected morph loadout");
            return Err("The player appearance rejected that morph loadout.".to_owned());
        }
        self.morph_equipment = legacy.equipment;
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
                BackendEvent::MorphCatalog(source) => {
                    debug!("morph catalog event received: bytes={}", source.len());
                    if let Some(shell) = &mut self.shell {
                        match shell.set_remote_morph_catalog(&source) {
                            Ok(count) => {
                                debug!("morph catalog installed: count={}", count);
                                shell.set_notice(format!(
                                    "Loaded {count} published morphs from D1/R2"
                                ));
                            }
                            Err(message) => {
                                warn!("morph catalog rejected: {}", message);
                                shell.set_notice(message);
                            }
                        }
                    }
                }
                BackendEvent::MorphCatalogError(message) => {
                    warn!("morph catalog request failed: {}", message);
                    if let Some(shell) = &mut self.shell {
                        shell.set_notice(format!("Morph catalog unavailable: {message}"));
                    }
                }
                BackendEvent::MorphPack { asset_id, bytes } => {
                    debug!(
                        "morph pack event received: asset_id={} bytes={}",
                        asset_id,
                        bytes.len()
                    );
                    let result = self.activate_morph_pack(&bytes);
                    match &result {
                        Ok((activated_id, size)) => {
                            debug!(
                                "morph pack activated: asset_id={} bytes={}",
                                activated_id, size
                            );
                        }
                        Err(error) => {
                            warn!(
                                "morph pack activation failed: asset_id={} error={}",
                                asset_id, error
                            );
                        }
                    }
                    if let Some(shell) = &mut self.shell {
                        shell.set_morph_runtime_result(result);
                    }
                }
                BackendEvent::MorphPackError { asset_id, message } => {
                    warn!(
                        "morph pack request failed: asset_id={} error={}",
                        asset_id, message
                    );
                    if let Some(shell) = &mut self.shell {
                        shell.set_notice(format!("Could not load {asset_id}: {message}"));
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

fn load_game_sources(game_root: Option<PathBuf>) -> Result<GameSources, Box<dyn Error>> {
    let Some(game_root) = game_root else {
        return Ok(GameSources {
            root: PathBuf::from(STANDALONE_PREVIEW_ROOT),
            manifest_source: STANDALONE_PREVIEW_MANIFEST.to_owned(),
            script_source: STANDALONE_PREVIEW_SCRIPT.to_owned(),
            standalone_preview: true,
        });
    };
    let manifest_path = game_root.join("manifest.json");
    let manifest_source = read_utf8_file(&manifest_path, "manifest")?;

    let (script_source, source_kind) = if game_root.join("game.luau").is_file() {
        (
            read_utf8_file(&game_root.join("game.luau"), "script")?,
            "built",
        )
    } else if game_root.join("src/main.luau").is_file() {
        (
            expand_raw_script(&game_root.join("src"), &game_root.join("src/main.luau"))?,
            "raw",
        )
    } else {
        return Err(Box::new(StudioError(format!(
            "{} is neither a built package nor a raw game project (expected game.luau or src/main.luau)",
            game_root.display()
        ))));
    };

    let manifest_source = if source_kind == "raw" {
        resolve_effects_source(&game_root, &manifest_source)?
    } else {
        manifest_source
    };

    Ok(GameSources {
        root: game_root,
        manifest_source,
        script_source,
        standalone_preview: false,
    })
}

fn read_utf8_file(path: &Path, kind: &str) -> Result<String, Box<dyn Error>> {
    fs::read_to_string(path).map_err(|error| {
        Box::new(StudioError(format!(
            "could not read {kind} file {}: {error}",
            path.display()
        ))) as Box<dyn Error>
    })
}

fn expand_raw_script(source_root: &Path, entry: &Path) -> Result<String, Box<dyn Error>> {
    let mut stack = Vec::new();
    expand_source_file(source_root, entry, &mut stack)
}

fn expand_source_file(
    source_root: &Path,
    path: &Path,
    stack: &mut Vec<PathBuf>,
) -> Result<String, Box<dyn Error>> {
    let relative = path
        .strip_prefix(source_root)
        .unwrap_or(path)
        .display()
        .to_string();
    let canonical = path.canonicalize().map_err(|error| {
        Box::new(StudioError(format!(
            "could not resolve source file {relative}: {error}"
        ))) as Box<dyn Error>
    })?;
    if stack.contains(&canonical) {
        let chain = stack
            .iter()
            .map(|item| {
                item.strip_prefix(source_root)
                    .unwrap_or(item)
                    .display()
                    .to_string()
            })
            .chain(std::iter::once(relative.clone()))
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(Box::new(StudioError(format!(
            "cyclic Luau include: {chain}"
        ))));
    }

    let source = read_utf8_file(path, "source")?;
    stack.push(canonical);
    let mut lines = Vec::new();
    for (line_number, line) in source.lines().enumerate() {
        if !stack.is_empty() && stack.len() > 1 && is_top_level_return(line) {
            return Err(Box::new(StudioError(format!(
                "{relative}:{}: included files cannot contain a top-level return",
                line_number + 1
            ))));
        }

        let Some(include_value) = parse_include(line) else {
            lines.push(line.to_owned());
            continue;
        };

        if let Some(sdk_source) = sdk_include(include_value)? {
            lines.push(format!("-- begin SDK include: {include_value}"));
            lines.push(sdk_source);
            lines.push(format!("-- end SDK include: {include_value}"));
            continue;
        }

        let include_path = safe_source_include(source_root, include_value).map_err(|error| {
            Box::new(StudioError(format!(
                "{relative}:{}: {error}",
                line_number + 1
            ))) as Box<dyn Error>
        })?;
        lines.push(format!("-- begin include: {include_value}"));
        lines.push(expand_source_file(source_root, &include_path, stack)?);
        lines.push(format!("-- end include: {include_value}"));
    }
    stack.pop();
    Ok(lines.join("\n"))
}

fn parse_include(line: &str) -> Option<&str> {
    let line = line.trim();
    let line = line.strip_prefix("--")?.trim_start();
    let line = line.strip_prefix("@include")?.trim_start();
    let line = line.strip_prefix('"')?;
    let (value, rest) = line.split_once('"')?;
    rest.trim().is_empty().then_some(value)
}

fn is_top_level_return(line: &str) -> bool {
    line.strip_prefix("return")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

fn sdk_include(include_value: &str) -> Result<Option<String>, Box<dyn Error>> {
    let Some(file_name) = include_value.strip_prefix("@cubacadabra/") else {
        return Ok(None);
    };
    let file_name = match file_name {
        "disclosure-v1.luau" => "disclosure.luau",
        "obby-v1.luau" => "obby.luau",
        "survival-v1.luau" => "survival.luau",
        "cycle-v1.luau" => "cycle.luau",
        "shared-state-v1.luau" => "shared-state.luau",
        _ => {
            return Err(Box::new(StudioError(format!(
                "unknown Cubacadabra SDK include: {include_value}"
            ))));
        }
    };
    let source = match file_name {
        "disclosure.luau" => include_str!("../../tools/src/cubacadabra/sdk/disclosure.luau"),
        "obby.luau" => include_str!("../../tools/src/cubacadabra/sdk/obby.luau"),
        "survival.luau" => include_str!("../../tools/src/cubacadabra/sdk/survival.luau"),
        "cycle.luau" => include_str!("../../tools/src/cubacadabra/sdk/cycle.luau"),
        "shared-state.luau" => {
            include_str!("../../tools/src/cubacadabra/sdk/shared-state.luau")
        }
        _ => unreachable!(),
    };
    Ok(Some(source.to_owned()))
}

fn safe_source_include(source_root: &Path, include_value: &str) -> Result<PathBuf, String> {
    let include_path = Path::new(include_value);
    if include_value.is_empty()
        || include_path.is_absolute()
        || include_path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("include must stay inside src/".to_owned());
    }
    let path = source_root.join(include_path);
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("included file could not be read: {error}"))?;
    let root = source_root
        .canonicalize()
        .map_err(|error| format!("source root could not be read: {error}"))?;
    if !canonical.starts_with(&root) {
        return Err("include must stay inside src/".to_owned());
    }
    if !canonical.is_file() {
        return Err(format!("included file not found: {include_value}"));
    }
    Ok(canonical)
}

fn resolve_effects_source(root: &Path, manifest_source: &str) -> Result<String, Box<dyn Error>> {
    let mut manifest: Value = serde_json::from_str(manifest_source).map_err(|error| {
        Box::new(StudioError(format!("manifest is not valid JSON: {error}"))) as Box<dyn Error>
    })?;
    let Some(effects) = manifest.get("effects").and_then(Value::as_object) else {
        return Ok(manifest_source.to_owned());
    };
    let Some(source_value) = effects.get("source") else {
        return Ok(manifest_source.to_owned());
    };
    let Some(relative) = source_value.as_str() else {
        return Err(Box::new(StudioError(
            "manifest.effects.source must be a relative JSON path".to_owned(),
        )));
    };
    if effects.len() != 1 || !is_safe_project_path(relative) {
        return Err(Box::new(StudioError(
            "manifest.effects.source must be the only effects field and stay inside the game project"
                .to_owned(),
        )));
    }
    let effects_path = root.join(relative);
    let effects_source = read_utf8_file(&effects_path, "effects")?;
    let resolved_effects: Value = serde_json::from_str(&effects_source).map_err(|error| {
        Box::new(StudioError(format!(
            "manifest.effects.source is not valid JSON: {error}"
        ))) as Box<dyn Error>
    })?;
    if !resolved_effects.is_object() {
        return Err(Box::new(StudioError(
            "manifest.effects.source must contain a JSON object".to_owned(),
        )));
    }
    manifest["effects"] = resolved_effects;
    Ok(serde_json::to_string_pretty(&manifest)? + "\n")
}

fn is_safe_project_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
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
        let playing = self.shell.as_ref().is_some_and(StudioShell::is_playing);
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
    let manifest: Value = serde_json::from_str(manifest_source)?;
    let Some(images) = manifest
        .get("assets")
        .and_then(|assets| assets.get("images"))
        .and_then(Value::as_object)
    else {
        return Ok(None);
    };
    if images.is_empty() {
        return Ok(None);
    }

    let mut loaded = Vec::with_capacity(images.len());
    for (id, definition) in images {
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
    for ((id, image), (_, left, top)) in loaded.iter().zip(&placements) {
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

fn parse_game_path() -> Result<Option<PathBuf>, Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let mut path = None;
    while let Some(argument) = args.next() {
        if argument == "--help" || argument == "-h" {
            println!("Usage: studio [--path <game-directory>]");
            println!();
            println!(
                "Open a local Cubacadabra game package, or launch the standalone morph preview."
            );
            std::process::exit(0);
        }
        if argument == "--path" {
            path = Some(
                args.next()
                    .ok_or_else(|| StudioError("--path expects a game directory".to_owned()))?,
            );
        } else {
            return Err(Box::new(StudioError(format!(
                "unknown argument: {}",
                argument.to_string_lossy()
            ))));
        }
    }
    let Some(path) = path else {
        return Ok(None);
    };
    let path = PathBuf::from(path).canonicalize()?;
    if !path.is_dir() {
        return Err(Box::new(StudioError(format!(
            "game path is not a directory: {}",
            path.display()
        ))));
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::{load_game_sources, should_forward_gameplay_keyboard};

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
    fn play_mode_bypasses_stale_shell_keyboard_capture() {
        assert!(should_forward_gameplay_keyboard(true, true));
    }

    #[test]
    fn paused_editor_still_honors_shell_keyboard_capture() {
        assert!(!should_forward_gameplay_keyboard(false, true));
        assert!(should_forward_gameplay_keyboard(false, false));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    debug!("Studio debug logging initialized");
    let game_path = parse_game_path()?;
    let mut app = StudioApp::load(game_path)?;
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
