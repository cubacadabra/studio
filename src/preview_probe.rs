//! Opt-in native preview smoke test and app-owned framebuffer captures.
//! Set CUBA_STUDIO_PROBE_DIR in a debug build; release builds omit this module.
use super::*;
use egui_wgpu::wgpu;

pub(crate) struct PreviewProbe {
    directory: PathBuf,
    frame: u32,
    review: bool,
    world: Option<String>,
    gameplay_camera: [f32; 3],
    projected: Option<[f32; 2]>,
    initial: Option<[f32; 2]>,
}

impl PreviewProbe {
    pub(crate) fn from_env() -> Option<Self> {
        let directory = PathBuf::from(env::var_os("CUBA_STUDIO_PROBE_DIR")?);
        fs::create_dir_all(&directory).expect("create preview probe directory");
        Some(Self {
            directory,
            frame: 0,
            review: env::var_os("CUBA_STUDIO_PROBE_REVIEW").is_some(),
            world: env::var("CUBA_STUDIO_PROBE_WORLD").ok(),
            gameplay_camera: [0.0; 3],
            projected: None,
            initial: None,
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
                drag(app, MouseButton::Left, 70.0, 30.0);
            }
            91 => {
                self.check_changed(app, "orbit");
                drag(app, MouseButton::Right, 60.0, -25.0);
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
        let name = match (self.review, self.frame) {
            (false, 120) => "gameplay",
            (true, 50) => "gameplay",
            (true, 80) => "showcase",
            (true, 90) => "orbit",
            (true, 100) => "pan",
            (true, 110) => "zoom",
            (true, 120) => "reset",
            _ => return None,
        };
        Some(self.directory.join(format!("{name}.png")))
    }

    pub(crate) fn finished(&self) -> bool {
        self.frame >= 121
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
        eprintln!("preview capture: {}", path.display());
        drop(mapped);
        self.buffer.unmap();
    }
}
