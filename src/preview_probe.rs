//! Opt-in native preview smoke test and app-owned framebuffer captures.
//! Set CUBA_STUDIO_PROBE_DIR in a debug build; release builds omit this module.
use super::*;
use egui_wgpu::wgpu;

pub(crate) struct PreviewProbe {
    directory: PathBuf,
    frame: u32,
    review: bool,
    multiplay: bool,
    expected_play_viewports: Option<Vec<egui::Rect>>,
    add_palette: bool,
    appearance: bool,
    room_capture: bool,
    capture_dataset: Option<PathBuf>,
    world: Option<String>,
    gameplay_camera: [f32; 3],
    projected: Option<[f32; 2]>,
    initial: Option<[f32; 2]>,
    reference: bool,
}

impl PreviewProbe {
    pub(crate) fn from_env() -> Option<Self> {
        let directory = PathBuf::from(env::var_os("CUBA_STUDIO_PROBE_DIR")?);
        fs::create_dir_all(&directory).expect("create preview probe directory");
        let reference = env::var_os("CUBA_STUDIO_PROBE_REFERENCE")
            .map(PathBuf::from)
            .map(|source| {
                assert!(source.is_file(), "probe reference {}", source.display());
                fs::copy(&source, directory.join("target.png")).unwrap_or_else(|error| {
                    panic!("copy probe reference {}: {error}", source.display())
                });
                true
            })
            .unwrap_or(false);
        Some(Self {
            directory,
            frame: 0,
            review: env::var_os("CUBA_STUDIO_PROBE_REVIEW").is_some(),
            multiplay: env::var_os("CUBA_STUDIO_PROBE_MULTIPLAY").is_some(),
            expected_play_viewports: None,
            add_palette: env::var_os("CUBA_STUDIO_PROBE_ADD").is_some(),
            appearance: env::var_os("CUBA_STUDIO_PROBE_APPEARANCE").is_some(),
            room_capture: env::var_os("CUBA_STUDIO_PROBE_ROOM_CAPTURE").is_some()
                || env::var_os("CUBA_STUDIO_PROBE_CAPTURE_DATASET").is_some(),
            capture_dataset: env::var_os("CUBA_STUDIO_PROBE_CAPTURE_DATASET").map(PathBuf::from),
            world: env::var("CUBA_STUDIO_PROBE_WORLD").ok(),
            gameplay_camera: [0.0; 3],
            projected: None,
            initial: None,
            reference,
        })
    }

    pub(crate) fn step(&mut self, app: &mut StudioApp) {
        if app
            .shell
            .as_ref()
            .is_none_or(StudioShell::is_project_loading)
        {
            return;
        }
        self.frame += 1;
        if self.frame == 2 && self.room_capture {
            #[cfg(target_os = "macos")]
            crate::macos::probe_room_capture_menu();
            #[cfg(not(target_os = "macos"))]
            app.shell
                .as_mut()
                .unwrap()
                .execute_command(crate::shell::StudioCommand::ImportRoomVideo);
            eprintln!("room video menu dispatch probe passed");
        }
        if self.frame == 3
            && let Some(dataset) = &self.capture_dataset
        {
            app.shell
                .as_mut()
                .unwrap()
                .probe_room_capture_review(dataset);
        }
        if self.frame == 80 && self.room_capture {
            app.shell
                .as_ref()
                .unwrap()
                .probe_room_capture_visible(self.capture_dataset.is_some());
            eprintln!("room video modal and capture review probe passed");
        }
        if self.frame == 2
            && let Some(world) = &self.world
        {
            assert!(
                app.client.engine_mut().start_world_by_id(world),
                "probe world {world}"
            );
            app.shell
                .as_mut()
                .unwrap()
                .select_scene_node(&format!("world/{world}"));
        }
        if self.frame == 2 && self.review {
            app.shell.as_mut().unwrap().set_playing(true);
        }
        if self.frame == 2 && self.multiplay {
            app.shell.as_mut().unwrap().start_play(9);
        }
        if self.multiplay {
            if let Some(expected) = &self.expected_play_viewports {
                assert_eq!(
                    app.shell.as_ref().unwrap().play_viewports(),
                    expected,
                    "player slots changed after redraw"
                );
            }
            // Exercise repeated takeovers, including the original player and
            // a player that has already occupied the main view.
            let target = match self.frame {
                80 => Some(1),
                90 => Some(4),
                100 => Some(0),
                110 => Some(4),
                _ => None,
            };
            if let Some(target) = target {
                let shell = app.shell.as_ref().unwrap();
                let previous = shell.controlled_player();
                let mut expected = shell.play_viewports().to_vec();
                let preview = expected[target].center();
                expected.swap(previous, target);
                let scale = app.window.as_ref().unwrap().scale_factor();
                app.handle_cursor_move(f64::from(preview.x) * scale, f64::from(preview.y) * scale);
                app.handle_mouse_button(ElementState::Pressed, MouseButton::Left);
                app.handle_mouse_button(ElementState::Released, MouseButton::Left);
                let shell = app.shell.as_ref().unwrap();
                assert_eq!(shell.controlled_player(), target);
                assert_eq!(shell.play_viewports(), expected);
                let expected_client = if target == 0 {
                    &app.client as *const ClientSession
                } else {
                    &app.preview_peers[target - 1].client as *const ClientSession
                };
                assert_eq!(
                    app.active_client_mut() as *const ClientSession,
                    expected_client
                );
                self.expected_play_viewports = Some(expected);
            }
            if self.frame == 180 {
                let snapshot = app
                    .active_client_mut()
                    .capture_snapshot()
                    .expect("controlled player snapshot");
                assert_eq!(snapshot.input.forward, 0.0);
                assert_eq!(snapshot.input.strafe, 0.0);
                assert!(!snapshot.input.jump && !snapshot.input.sprint);
                assert!(
                    !snapshot.player.moving,
                    "autopilot still moves the controlled player"
                );
                eprintln!(
                    "multiplayer probe passed: repeated player swaps, stable slots, input routing, controlled player idle"
                );
            }
        }
        if self.add_palette {
            if self.frame == 60 {
                let shell = app.shell.as_mut().unwrap();
                shell.set_playing(false);
                assert!(
                    shell.open_add_palette(None),
                    "probe could not open Add palette"
                );
            }
            if self.frame == 81 {
                eprintln!("add palette probe passed");
            }
            return;
        }
        if self.appearance {
            if self.frame == 60 {
                app.shell.as_mut().unwrap().set_playing(false);
                app.apply_scene_edit(crate::shell::SceneEditRequest::AddObject {
                    world_id: self.world.clone(),
                    kind: crate::shell::SceneObjectKind::Block,
                })
                .expect("appearance probe could not add a block");
            }
            if self.frame == 81 {
                eprintln!("appearance inspector probe passed");
            }
            return;
        }
        if !self.review {
            return;
        }
        match self.frame {
            60 => {
                app.clear_pointer_controls();
                let shell = app.shell.as_mut().unwrap();
                shell.set_playing(false);
                shell.set_review_camera(crate::shell::ReviewCameraPreset::Showcase);
                self.gameplay_camera = app.client.engine().camera();
            }
            81 => {
                self.initial = app
                    .renderer
                    .as_ref()
                    .unwrap()
                    .studio_project_world_point([12.0, 0.0, 20.0]);
                self.projected = self.initial;
                drag(app, MouseButton::Right, 70.0, 30.0);
            }
            91 => {
                self.check_changed(app, "orbit");
                drag(app, MouseButton::Middle, 60.0, -25.0);
            }
            101 => {
                self.check_changed(app, "pan");
                app.zoom_delta = 5.0;
            }
            111 => {
                self.check_changed(app, "zoom");
                app.shell
                    .as_mut()
                    .unwrap()
                    .set_review_camera(crate::shell::ReviewCameraPreset::Showcase);
            }
            121 => {
                assert_eq!(
                    app.client.engine().camera(),
                    self.gameplay_camera,
                    "review navigation changed gameplay camera"
                );
                let reset = app
                    .renderer
                    .as_ref()
                    .unwrap()
                    .studio_project_world_point([12.0, 0.0, 20.0]);
                assert_eq!(
                    reset, self.initial,
                    "reselecting preset must restore framing"
                );
                eprintln!(
                    "preview probe passed: stopped orbit, pan, zoom, reset; gameplay camera unchanged"
                );
            }
            _ => {}
        }
    }

    fn check_changed(&mut self, app: &StudioApp, action: &str) {
        let projected = app
            .renderer
            .as_ref()
            .unwrap()
            .studio_project_world_point([12.0, 0.0, 20.0]);
        assert!(
            projected.is_some() && projected != self.projected,
            "{action} did not change visible projection"
        );
        assert_eq!(app.client.engine().camera(), self.gameplay_camera);
        self.projected = projected;
    }

    pub(crate) fn capture_path(&self) -> Option<PathBuf> {
        let name = if self.multiplay && self.frame == 70 {
            "multiplayer"
        } else if self.multiplay && self.frame == 180 {
            "multiplayer-switched"
        } else if self.add_palette && self.frame == 80 {
            "add-palette"
        } else if self.appearance && self.frame == 80 {
            "appearance-inspector"
        } else if self.room_capture && self.frame == 80 {
            "room-video"
        } else {
            match (self.review, self.frame) {
                (false, 120) => {
                    if self.reference {
                        "current"
                    } else {
                        "gameplay"
                    }
                }
                (true, 50) => {
                    if self.reference {
                        "current"
                    } else {
                        "gameplay"
                    }
                }
                (true, 80) => "showcase",
                (true, 90) => "orbit",
                (true, 100) => "pan",
                (true, 110) => "zoom",
                (true, 120) => "reset",
                _ => return None,
            }
        };
        Some(self.directory.join(format!("{name}.png")))
    }

    pub(crate) fn finished(&self) -> bool {
        self.frame
            >= if self.add_palette || self.appearance || self.room_capture {
                81
            } else if self.multiplay {
                181
            } else {
                121
            }
    }
}

fn drag(app: &mut StudioApp, button: MouseButton, dx: f64, dy: f64) {
    let center = app.shell.as_ref().unwrap().runtime_viewport().center();
    let scale = app.window.as_ref().unwrap().scale_factor();
    app.handle_cursor_move(f64::from(center.x) * scale, f64::from(center.y) * scale);
    app.handle_mouse_button(ElementState::Pressed, button);
    app.handle_cursor_move(
        (f64::from(center.x) + dx) * scale,
        (f64::from(center.y) + dy) * scale,
    );
    app.handle_mouse_button(ElementState::Released, button);
}

pub(crate) struct Readback {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    stride: u32,
}

impl Readback {
    pub(crate) fn encode(
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> Self {
        let texture = view.texture();
        let (width, height) = (texture.width(), texture.height());
        let stride = (width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Studio preview probe"),
            size: u64::from(stride) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        Self {
            buffer,
            width,
            height,
            stride,
        }
    }

    pub(crate) fn save(self, device: &wgpu::Device, path: &Path) {
        let (send, receive) = mpsc::channel();
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receive.recv().unwrap().unwrap();
        let mapped = self.buffer.slice(..).get_mapped_range();
        let pixels = mapped
            .chunks(self.stride as usize)
            .flat_map(|row| row[..self.width as usize * 4].iter().copied())
            .collect();
        RgbaImage::from_raw(self.width, self.height, pixels)
            .unwrap()
            .save(path)
            .unwrap();
        if path.file_name().is_some_and(|name| name == "current.png") {
            write_reference_diff(path);
        }
        eprintln!("preview capture: {}", path.display());
        drop(mapped);
        self.buffer.unmap();
    }
}

fn write_reference_diff(current_path: &Path) {
    let target_path = current_path.with_file_name("target.png");
    let target = image::open(&target_path)
        .unwrap_or_else(|error| panic!("read reference {}: {error}", target_path.display()))
        .into_rgba8();
    let current = image::open(current_path)
        .unwrap_or_else(|error| panic!("read current {}: {error}", current_path.display()))
        .into_rgba8();
    let target = image::imageops::resize(
        &target,
        current.width(),
        current.height(),
        image::imageops::FilterType::Triangle,
    );
    let mut diff = RgbaImage::new(current.width(), current.height());
    for ((target, current), output) in target.pixels().zip(current.pixels()).zip(diff.pixels_mut())
    {
        let delta = [
            target[0].abs_diff(current[0]),
            target[1].abs_diff(current[1]),
            target[2].abs_diff(current[2]),
        ];
        // Amplify small shifts just enough for an at-a-glance visual review.
        *output = image::Rgba([
            delta[0].saturating_mul(2),
            delta[1].saturating_mul(2),
            delta[2].saturating_mul(2),
            255,
        ]);
    }
    let path = current_path.with_file_name("diff.png");
    diff.save(&path)
        .unwrap_or_else(|error| panic!("write reference diff {}: {error}", path.display()));
    eprintln!("preview reference diff: {}", path.display());
}
