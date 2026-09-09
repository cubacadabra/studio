use cubacadabra_engine::{Engine, native::Renderer};
use image::{GenericImage, RgbaImage, imageops::FilterType};
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
mod network;
mod shell;
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
}

#[derive(Clone, Debug)]
struct RemotePlayer {
    username: String,
    generation: u32,
    position: [f32; 3],
    yaw: f32,
    moving: bool,
    sprinting: bool,
    appearance: Option<Value>,
}

struct StudioApp {
    game_root: PathBuf,
    network: BackendClient,
    engine: Engine,
    remote_players: BTreeMap<String, RemotePlayer>,
    remote_roster_dirty: bool,
    remote_sequence: u64,
    player_id: Option<String>,
    connected_world_id: Option<String>,
    pending_session_world_id: Option<String>,
    launch_world_id: Option<String>,
    image_atlas: Option<ImageAtlas>,
    window: Option<Window>,
    renderer: Option<Renderer>,
    shell: Option<StudioShell>,
    pressed_keys: HashSet<KeyCode>,
    jump_queued: bool,
    mobile_sprint: bool,
    climb: bool,
    joystick_input: (f32, f32),
    pointer_position: Option<(f32, f32)>,
    pointer_active: bool,
    ui_pointer_active: bool,
    look_delta: (f32, f32),
    zoom_delta: f32,
    last_frame: Instant,
}

impl StudioApp {
    fn load(game_root: PathBuf) -> Result<Self, Box<dyn Error>> {
        let sources = load_game_sources(game_root)?;
        let manifest_source = sources.manifest_source;
        let script_source = sources.script_source;
        let game_root = sources.root;
        let manifest = serde_json::from_str::<Value>(&manifest_source)
            .map_err(|error| StudioError(format!("manifest is not valid JSON: {error}")))?;
        let game_id = manifest
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| StudioError("manifest.json is missing a string id".to_owned()))?;
        let launch_world_id = manifest
            .get("launch")
            .and_then(Value::as_object)
            .and_then(|launch| launch.get("destinationWorld"))
            .and_then(Value::as_str)
            .map(str::to_owned);

        let mut engine = Engine::new();
        if !engine.load_package_source(&manifest_source) {
            return Err(Box::new(StudioError(
                "the shared engine rejected manifest.json".to_owned(),
            )));
        }
        if !engine.load_script_source(&script_source) {
            return Err(Box::new(StudioError(
                "the shared engine could not compile game.luau".to_owned(),
            )));
        }
        if engine.active_world_id().is_none() {
            return Err(Box::new(StudioError(
                "the game manifest did not define a start world".to_owned(),
            )));
        }
        let network = BackendClient::new(&game_id).map_err(StudioError)?;

        Ok(Self {
            image_atlas: load_image_atlas(&game_root, &manifest_source)?,
            game_root,
            network,
            engine,
            remote_players: BTreeMap::new(),
            remote_roster_dirty: true,
            remote_sequence: 0,
            player_id: None,
            connected_world_id: None,
            pending_session_world_id: None,
            launch_world_id,
            window: None,
            renderer: None,
            shell: None,
            pressed_keys: HashSet::new(),
            jump_queued: false,
            mobile_sprint: false,
            climb: false,
            joystick_input: (0.0, 0.0),
            pointer_position: None,
            pointer_active: false,
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
        self.engine.set_ui_viewport_values(
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
        let prepared_shell: Option<PreparedShell> = match (&mut self.shell, &self.window) {
            (Some(shell), Some(window)) => Some(shell.prepare(window, &project_name)),
            _ => None,
        };
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
        self.engine.set_input_values(
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
        self.sync_backend_world();
        self.sync_remote_players();
        self.engine.step(delta);
        self.drain_ui_events();
        self.sync_backend_world();
        self.flush_network_messages();
        if self.engine.active_world_id() != Some("settings") {
            let snapshot = self.engine.snapshot();
            self.network.send_move(
                snapshot[0],
                snapshot[1],
                snapshot[2],
                self.engine.player_facing_yaw(),
                length > 0.01,
                playing && sprint,
                self.engine.studio_player_respawn_event_id(),
            );
        }

        if let Some(renderer) = &mut self.renderer {
            renderer.sync(&self.engine);
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

    fn pointer_event(&mut self, phase: u8, x: f32, y: f32) -> bool {
        self.engine.ui_pointer_event(1, phase, x, y)
    }

    fn drain_ui_events(&mut self) {
        while let Some(source) = self.engine.poll_ui_event_json() {
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
            if self.pointer_active && !self.ui_pointer_active {
                self.look_delta.0 += logical.0 - previous.0;
                self.look_delta.1 += logical.1 - previous.1;
            }
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
        if button != MouseButton::Left {
            return;
        }
        let Some((x, y)) = self.pointer_position else {
            return;
        };
        match state {
            ElementState::Pressed => {
                let Some((local_x, local_y)) = self.runtime_pointer(x, y, true) else {
                    return;
                };
                self.ui_pointer_active = self.pointer_event(0, local_x, local_y);
                self.pointer_active = !self.ui_pointer_active;
            }
            ElementState::Released => {
                if self.ui_pointer_active {
                    if let Some((local_x, local_y)) = self.runtime_pointer(x, y, false) {
                        self.pointer_event(2, local_x, local_y);
                    }
                }
                self.ui_pointer_active = false;
                self.pointer_active = false;
            }
        }
    }

    fn drain_backend_events(&mut self) {
        while let Some(event) = self.network.try_recv() {
            match event {
                BackendEvent::Connected => self.reset_remote_session(),
                BackendEvent::Disconnected => self.reset_remote_session(),
                BackendEvent::Message(source) => self.handle_backend_message(&source),
            }
        }
    }

    fn handle_backend_message(&mut self, source: &str) {
        let Ok(event) = serde_json::from_str::<Value>(source) else {
            return;
        };
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match event_type {
            "session_identity" => {
                self.player_id = event.get("id").and_then(Value::as_str).map(str::to_owned);
            }
            "player_join" => {
                let Some(id) = event.get("id").and_then(Value::as_str) else {
                    return;
                };
                if self.player_id.as_deref() == Some(id) {
                    return;
                }
                self.remote_players.insert(
                    id.to_owned(),
                    RemotePlayer {
                        username: event
                            .get("username")
                            .and_then(Value::as_str)
                            .unwrap_or(id)
                            .to_owned(),
                        generation: event.get("generation").and_then(Value::as_u64).unwrap_or(0)
                            as u32,
                        position: [0.0; 3],
                        yaw: 0.0,
                        moving: false,
                        sprinting: false,
                        appearance: event.get("appearance").cloned(),
                    },
                );
                self.remote_roster_dirty = true;
            }
            "player_leave" => {
                if let Some(id) = event.get("id").and_then(Value::as_str) {
                    self.remote_players.remove(id);
                    self.remote_roster_dirty = true;
                }
            }
            "appearance" => {
                if let Some(id) = event.get("id").and_then(Value::as_str) {
                    if let Some(player) = self.remote_players.get_mut(id) {
                        player.appearance = event.get("appearance").cloned();
                        self.remote_roster_dirty = true;
                    }
                }
            }
            "player_name" => {
                if let (Some(id), Some(username)) = (
                    event.get("id").and_then(Value::as_str),
                    event.get("username").and_then(Value::as_str),
                ) {
                    if let Some(player) = self.remote_players.get_mut(id) {
                        player.username = username.to_owned();
                        self.remote_roster_dirty = true;
                    }
                }
            }
            "move" => self.handle_remote_move(&event),
            "experience_launch" => {
                let Some(player_id) = self.player_id.as_deref() else {
                    return;
                };
                let in_launch_group = event
                    .get("playerIds")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(player_id)));
                if in_launch_group {
                    self.pending_session_world_id = event
                        .get("sessionWorldId")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let Some(launch_world_id) = self.launch_world_id.as_deref() else {
                        eprintln!(
                            "Cubacadabra Studio: backend requested a launch without launch.destinationWorld"
                        );
                        return;
                    };
                    if !self.engine.start_world_by_id(launch_world_id) {
                        eprintln!(
                            "Cubacadabra Studio: backend requested unavailable world {launch_world_id}"
                        );
                    }
                }
            }
            "game_state" | "game_message" | "player_state" => {
                let _ = self.engine.studio_receive_network_message_json(source);
            }
            _ => {}
        }
    }

    fn handle_remote_move(&mut self, event: &Value) {
        let Some(id) = event.get("id").and_then(Value::as_str) else {
            return;
        };
        let coordinates = ["x", "y", "z"].map(|key| {
            event
                .get(key)
                .and_then(Value::as_f64)
                .map(|value| value as f32)
        });
        let Some([Some(x), Some(y), Some(z)]) = Some(coordinates) else {
            return;
        };
        let Some(yaw) = event
            .get("yaw")
            .and_then(Value::as_f64)
            .map(|value| value as f32)
        else {
            return;
        };
        if ![x, y, z, yaw].iter().all(|value| value.is_finite()) {
            return;
        }
        if self.player_id.as_deref() == Some(id) {
            if event.get("corrected").and_then(Value::as_bool) == Some(true) {
                self.engine.reconcile_player([x, y, z], yaw);
            }
            return;
        }
        let player = self
            .remote_players
            .entry(id.to_owned())
            .or_insert(RemotePlayer {
                username: id.to_owned(),
                generation: 0,
                position: [0.0; 3],
                yaw: 0.0,
                moving: false,
                sprinting: false,
                appearance: None,
            });
        player.position = [x, y, z];
        player.yaw = yaw as f32;
        player.moving = event
            .get("moving")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        player.sprinting = event
            .get("sprinting")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(generation) = event.get("generation").and_then(Value::as_u64) {
            player.generation = generation as u32;
        }
        self.remote_roster_dirty = true;
    }

    fn reset_remote_session(&mut self) {
        self.remote_players.clear();
        self.remote_roster_dirty = true;
        self.remote_sequence = 0;
        self.player_id = None;
        self.engine.studio_reset_remote_session();
    }

    fn sync_remote_players(&mut self) {
        if !self.remote_roster_dirty {
            return;
        }
        let Some(world_id) = self.engine.active_world_id() else {
            return;
        };
        self.remote_sequence = self.remote_sequence.saturating_add(1);
        self.remote_roster_dirty = false;
        let players = self
            .remote_players
            .iter()
            .map(|(id, player)| {
                let mut value = serde_json::json!({
                    "id": id,
                    "username": player.username,
                    "generation": player.generation,
                    "position": player.position,
                    "yaw": player.yaw,
                    "moving": player.moving,
                    "sprinting": player.sprinting,
                });
                if let Some(appearance) = &player.appearance {
                    value["appearance"] = appearance.clone();
                }
                value
            })
            .collect::<Vec<_>>();
        let message = serde_json::json!({
            "version": 1,
            "sequence": self.remote_sequence,
            "worldId": world_id,
            "players": players,
        });
        let _ = self
            .engine
            .studio_apply_remote_update_json(&message.to_string());
    }

    fn sync_backend_world(&mut self) {
        let Some(world_id) = self.engine.active_world_id() else {
            return;
        };
        let network_world_id = if world_id == "settings" {
            "lobby".to_owned()
        } else if self.launch_world_id.as_deref() == Some(world_id)
            && self.pending_session_world_id.is_some()
        {
            self.pending_session_world_id.clone().unwrap_or_default()
        } else {
            world_id.to_owned()
        };
        if self.connected_world_id.as_deref() == Some(network_world_id.as_str()) {
            return;
        }
        self.connected_world_id = Some(network_world_id.clone());
        self.reset_remote_session();
        self.network.set_world(network_world_id);
    }

    fn flush_network_messages(&mut self) {
        while let Some(source) = self.engine.studio_poll_network_message() {
            let Ok(message) = serde_json::from_str::<Value>(&source) else {
                continue;
            };
            let Some(channel) = message.get("channel").and_then(Value::as_str) else {
                continue;
            };
            let expected_sequence = message
                .get("expectedSequence")
                .and_then(Value::as_u64)
                .filter(|sequence| *sequence <= u32::MAX as u64);
            let event_type = if expected_sequence.is_some() {
                "game_state_compare_set"
            } else if message.get("retained").and_then(Value::as_bool) == Some(true) {
                "game_state_set"
            } else {
                "game_message"
            };
            let mut event = serde_json::json!({
                "type": event_type,
                "channel": channel,
                "payload": message.get("payload").cloned().unwrap_or(Value::Null),
            });
            if let Some(sequence) = expected_sequence {
                event["expectedSequence"] = sequence.into();
            }
            self.network.send(event.to_string());
        }
    }
}

fn load_game_sources(game_root: PathBuf) -> Result<GameSources, Box<dyn Error>> {
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
            WindowEvent::KeyboardInput { event, .. } if !shell_consumed => {
                self.handle_key(&event, event_loop)
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_move(position.x, position.y)
            }
            WindowEvent::MouseInput { state, button, .. }
                if runtime_hovered || !shell_consumed || state == ElementState::Released =>
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

fn next_power_of_two(value: u32) -> u32 {
    value.next_power_of_two().min(MAX_ATLAS_DIMENSION)
}

fn parse_game_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let mut path = None;
    while let Some(argument) = args.next() {
        if argument == "--help" || argument == "-h" {
            println!("Usage: studio --path <game-directory>");
            println!();
            println!("Open a local Cubacadabra game package in a desktop window.");
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
    let path = path.ok_or_else(|| StudioError("missing required --path".to_owned()))?;
    let path = PathBuf::from(path).canonicalize()?;
    if !path.is_dir() {
        return Err(Box::new(StudioError(format!(
            "game path is not a directory: {}",
            path.display()
        ))));
    }
    Ok(path)
}

fn main() -> Result<(), Box<dyn Error>> {
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
