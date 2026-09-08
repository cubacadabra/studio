use cubacadabra_engine::{Engine, native::Renderer};
use image::{GenericImage, RgbaImage, imageops::FilterType};
use serde_json::Value;
use std::{
    collections::HashSet,
    env,
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
    time::Instant,
};
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

struct StudioApp {
    game_root: PathBuf,
    engine: Engine,
    image_atlas: Option<ImageAtlas>,
    window: Option<Window>,
    renderer: Option<Renderer>,
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
        let manifest_path = game_root.join("manifest.json");
        let script_path = game_root.join("game.luau");
        let manifest_source = fs::read_to_string(&manifest_path).map_err(|error| {
            StudioError(format!(
                "could not read {}: {error}",
                manifest_path.display()
            ))
        })?;
        let script_source = fs::read_to_string(&script_path).map_err(|error| {
            StudioError(format!("could not read {}: {error}", script_path.display()))
        })?;

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

        Ok(Self {
            image_atlas: load_image_atlas(&game_root, &manifest_source)?,
            game_root,
            engine,
            window: None,
            renderer: None,
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

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.update_viewport();
        self.request_redraw();
        Ok(())
    }

    fn update_viewport(&mut self) {
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        self.engine.set_ui_viewport_values(
            size.width as f32 / scale,
            size.height as f32 / scale,
            scale,
            0.0,
            0.0,
            0.0,
            0.0,
        );
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
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;

        let mut forward = axis(&self.pressed_keys, KeyCode::KeyW, KeyCode::KeyS);
        let mut strafe = axis(&self.pressed_keys, KeyCode::KeyD, KeyCode::KeyA);
        forward -= self.joystick_input.1;
        strafe += self.joystick_input.0;
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
            sprint,
            self.jump_queued,
            self.climb,
            self.look_delta.0,
            self.look_delta.1,
            self.zoom_delta,
        );
        self.jump_queued = false;
        self.look_delta = (0.0, 0.0);
        self.zoom_delta = 0.0;
        self.engine.step(delta);
        self.drain_ui_events();

        if let Some(renderer) = &mut self.renderer {
            renderer.sync(&self.engine);
            renderer.draw();
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
            self.pointer_event(1, logical.0, logical.1);
        }
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
                self.ui_pointer_active = self.pointer_event(0, x, y);
                self.pointer_active = !self.ui_pointer_active;
            }
            ElementState::Released => {
                if self.ui_pointer_active {
                    self.pointer_event(2, x, y);
                }
                self.ui_pointer_active = false;
                self.pointer_active = false;
            }
        }
    }
}

impl ApplicationHandler for StudioApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
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
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(&event, event_loop),
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_move(position.x, position.y)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_button(state, button)
            }
            WindowEvent::MouseWheel { delta, .. } => {
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

fn axis(keys: &HashSet<KeyCode>, positive: KeyCode, negative: KeyCode) -> f32 {
    f32::from(keys.contains(&positive)) - f32::from(keys.contains(&negative))
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
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app)?;
    Ok(())
}
