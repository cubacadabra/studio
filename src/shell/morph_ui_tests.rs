//! GPU-backed UI smoke checks, runnable even while the desktop is locked.
use super::*;
use winit::raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle};

struct HeadlessDisplay;
impl HasDisplayHandle for HeadlessDisplay {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

#[test]
#[ignore = "requires a GPU; optionally set CUBA_STUDIO_UI_CAPTURES to save PNGs"]
fn morph_library_layout_and_interactions() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let context = egui::Context::default();
    configure_context(&context);
    let state = EguiState::new(
        context.clone(),
        egui::ViewportId::ROOT,
        &HeadlessDisplay,
        Some(1.),
        None,
        Some(4096),
    );
    let renderer = EguiRenderer::new(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        RendererOptions::default(),
    );
    let mut shell = StudioShell::from_egui(context.clone(), state, renderer);
    shell.morph_catalog = crate::wardrobe_tests::catalog();
    shell.morph_catalog_ready = true;
    shell.active_loadout = shell.morph_catalog.presets[0].loadout();
    shell.selected_morph = shell.morph_catalog.presets[0].id.clone();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/starter-set/presets");
    for preset in shell.morph_catalog.presets.clone() {
        let url = preset.thumbnail.unwrap();
        shell.set_morph_thumbnail(url.clone(), &fs::read(root.join(url)).unwrap());
    }
    let mut time = 0.;
    for [width, height] in [[1280, 800], [1440, 900], [768, 1024], [390, 844]] {
        for _ in 0..2 {
            let output = frame(&mut shell, &mut time, [width, height], vec![]);
            shell.pending_textures_delta.append(output.textures_delta);
            capture(&mut shell, &device, &queue, output.shapes, [width, height]);
        }
        assert!(
            shell.runtime_viewport.width() >= 240.,
            "viewport too narrow at {width}: {:?}",
            shell.runtime_viewport
        );
        assert!(
            shell.runtime_viewport.height() >= 200.,
            "viewport too short at {height}"
        );
        assert!(shell.runtime_viewport.right() <= width as f32 + 1.);
        assert!(shell.runtime_viewport.bottom() <= height as f32 + 1.);
    }
    // Search for a starter and click its real image tile through egui input.
    shell.morph_query = "17".into();
    let mut output = frame(&mut shell, &mut time, [1280, 800], vec![]);
    shell.pending_textures_delta.append(output.textures_delta);
    output = frame(&mut shell, &mut time, [1280, 800], vec![]);
    shell.pending_textures_delta.append(output.textures_delta);
    let label = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == "Person 17" => Some(text.pos),
            _ => None,
        })
        .expect("search result label");
    let point = label + egui::vec2(35., -45.);
    for pressed in [true, false] {
        let output = frame(
            &mut shell,
            &mut time,
            [1280, 800],
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        shell.pending_textures_delta.append(output.textures_delta);
    }
    assert!(
        matches!(shell.take_morph_request(), Some(crate::wardrobe::Request::Preset(id)) if id.as_str() == "cuba:preset/person-17.v1")
    );
    shell.active_loadout = shell.morph_catalog.presets[16].loadout();
    shell.morph_starters = false;
    shell.morph_query = "glasses".into();
    let output = frame(&mut shell, &mut time, [1280, 800], vec![]);
    shell.pending_textures_delta.append(output.textures_delta);
    let output = frame(&mut shell, &mut time, [1280, 800], vec![]);
    let labels: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(labels.contains(&"Round Glasses"));
    assert!(labels.contains(&"None"));
    assert!(!labels.contains(&"Outfits"));
}

fn frame(
    shell: &mut StudioShell,
    time: &mut f64,
    size: [u32; 2],
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *time += 0.1;
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size[0] as f32, size[1] as f32),
        )),
        time: Some(*time),
        events,
        focused: true,
        ..Default::default()
    };
    shell
        .context
        .clone()
        .run_ui(input, |ui| shell.show(ui, "Morph Preview"))
}

fn capture(
    shell: &mut StudioShell,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shapes: Vec<egui::epaint::ClippedShape>,
    size: [u32; 2],
) {
    let [width, height] = size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let prepared = PreparedShell {
        paint_jobs: shell.context.tessellate(shapes, 1.),
        screen: ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: 1.,
        },
    };
    shell.paint(
        device,
        queue,
        &mut encoder,
        &texture.create_view(&Default::default()),
        prepared,
    );
    let stride = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (stride * height) as u64,
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
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    if let Ok(directory) = std::env::var("CUBA_STUDIO_UI_CAPTURES") {
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range();
        let pixels: Vec<u8> = mapped
            .chunks(stride as usize)
            .flat_map(|row| row[..(width * 4) as usize].iter().copied())
            .collect();
        fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            std::path::Path::new(&directory).join(format!("studio-{width}.png")),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
