use cubacadabra_engine::native::Renderer as GameRenderer;
use egui::{
    Align, Align2, Color32, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Stroke,
    StrokeKind, TextStyle, Vec2,
};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor, wgpu};
use egui_winit::State as EguiState;
use std::time::Duration;
use winit::{event::WindowEvent, window::Window};

const TOP_BAR_HEIGHT: f32 = 48.0;
const STATUS_BAR_HEIGHT: f32 = 28.0;
const PANEL: Color32 = Color32::from_rgb(29, 31, 35);
const PANEL_RAISED: Color32 = Color32::from_rgb(35, 38, 43);
const SURFACE: Color32 = Color32::from_rgb(22, 24, 28);
const SURFACE_DEEP: Color32 = Color32::from_rgb(17, 19, 22);
const BORDER: Color32 = Color32::from_rgb(55, 59, 66);
const TEXT: Color32 = Color32::from_rgb(226, 229, 234);
const MUTED: Color32 = Color32::from_rgb(143, 150, 160);
const ACCENT: Color32 = Color32::from_rgb(91, 207, 183);
const WARNING: Color32 = Color32::from_rgb(231, 176, 83);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Workspace {
    #[default]
    World,
    Assets,
    Materials,
    Test,
}

impl Workspace {
    const ALL: [Self; 4] = [Self::World, Self::Assets, Self::Materials, Self::Test];

    fn label(self) -> &'static str {
        match self {
            Self::World => "World",
            Self::Assets => "Assets",
            Self::Materials => "Materials",
            Self::Test => "Test",
        }
    }
}

pub(crate) struct PreparedShell {
    paint_jobs: Vec<egui::ClippedPrimitive>,
    screen: ScreenDescriptor,
}

pub(crate) struct StudioShell {
    context: egui::Context,
    state: EguiState,
    renderer: EguiRenderer,
    workspace: Workspace,
    runtime_viewport: Rect,
    selected_scene: &'static str,
    selected_asset: &'static str,
    test_tool: &'static str,
    asset_filter: &'static str,
    playing: bool,
    notice: String,
    search_query: String,
    position: [f32; 3],
    rotation: f32,
    scale: f32,
    roughness: f32,
    pending_textures_delta: egui::TexturesDelta,
}

impl StudioShell {
    pub(crate) fn new(window: &Window, game_renderer: &GameRenderer) -> Self {
        let context = egui::Context::default();
        configure_style(&context);
        let state = EguiState::new(
            context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(4_096),
        );
        let renderer = EguiRenderer::new(
            game_renderer.device(),
            game_renderer.studio_overlay_format(),
            RendererOptions::default(),
        );
        Self {
            context,
            state,
            renderer,
            workspace: Workspace::World,
            runtime_viewport: Rect::NOTHING,
            selected_scene: "Tree 014",
            selected_asset: "forest-grass",
            test_tool: "Sessions",
            asset_filter: "All",
            playing: false,
            notice: "Ready".to_owned(),
            search_query: String::new(),
            position: [6.4, 0.0, -12.8],
            rotation: 18.0,
            scale: 1.0,
            roughness: 0.72,
            pending_textures_delta: egui::TexturesDelta::default(),
        }
    }

    pub(crate) fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    pub(crate) fn runtime_viewport(&self) -> Rect {
        self.runtime_viewport
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playing
    }

    pub(crate) fn prepare(&mut self, window: &Window, project_name: &str) -> PreparedShell {
        let input = self.state.take_egui_input(window);
        let context = self.context.clone();
        let output = context.run_ui(input, |ui| self.show(ui, project_name));
        self.state
            .handle_platform_output(window, output.platform_output);
        let pixels_per_point = context.pixels_per_point();
        let paint_jobs = context.tessellate(output.shapes, pixels_per_point);
        let size = window.inner_size();
        self.pending_textures_delta.append(output.textures_delta);
        PreparedShell {
            paint_jobs,
            screen: ScreenDescriptor {
                size_in_pixels: [size.width, size.height],
                pixels_per_point,
            },
        }
    }

    pub(crate) fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::TextureView,
        prepared: PreparedShell,
    ) {
        let textures_delta = std::mem::take(&mut self.pending_textures_delta);
        for (id, image_delta) in &textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }
        let command_buffers = self.renderer.update_buffers(
            device,
            queue,
            encoder,
            &prepared.paint_jobs,
            &prepared.screen,
        );
        debug_assert!(command_buffers.is_empty());
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Cubacadabra Studio shell"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.renderer.render(
            &mut pass.forget_lifetime(),
            &prepared.paint_jobs,
            &prepared.screen,
        );
        for id in &textures_delta.free {
            self.renderer.free_texture(id);
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, project_name: &str) {
        self.runtime_viewport = Rect::NOTHING;
        self.show_top_bar(ui, project_name);
        self.show_status_bar(ui);
        match self.workspace {
            Workspace::World => self.show_world(ui),
            Workspace::Assets => self.show_assets(ui),
            Workspace::Materials => self.show_materials(ui),
            Workspace::Test => self.show_test(ui),
        }
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    fn show_top_bar(&mut self, root: &mut egui::Ui, project_name: &str) {
        egui::Panel::top("studio_top_bar")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(panel_frame(PANEL_RAISED).inner_margin(Margin::symmetric(12, 0)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(TOP_BAR_HEIGHT);
                    ui.label(
                        RichText::new("C")
                            .strong()
                            .color(SURFACE_DEEP)
                            .background_color(ACCENT),
                    );
                    ui.label(RichText::new(project_name).strong().color(TEXT));
                    ui.add_space(8.0);

                    ui.menu_button("File", |ui| {
                        if ui.button("Open Project…").clicked() {
                            self.notice = "Open Project is a layout preview".to_owned();
                            ui.close();
                        }
                        if ui.button("Save").clicked() {
                            self.notice = "Nothing to save yet".to_owned();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Reveal Project").clicked() {
                            self.notice = "Reveal Project is not connected yet".to_owned();
                            ui.close();
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        ui.add_enabled(false, egui::Button::new("Undo"));
                        ui.add_enabled(false, egui::Button::new("Redo"));
                        ui.separator();
                        if ui.button("Preferences…").clicked() {
                            self.notice = "Preferences are coming later".to_owned();
                            ui.close();
                        }
                    });
                    ui.menu_button("Window", |ui| {
                        if ui.button("Maximize Viewport").clicked() {
                            self.notice = "Viewport maximize is coming later".to_owned();
                            ui.close();
                        }
                        if ui.button("Reset Layout").clicked() {
                            self.notice = "Layout reset".to_owned();
                            ui.close();
                        }
                    });

                    ui.add_space(10.0);
                    for workspace in Workspace::ALL {
                        if workspace_tab(ui, workspace.label(), self.workspace == workspace)
                            .clicked()
                        {
                            self.workspace = workspace;
                            self.notice = format!("{} workspace", workspace.label());
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("● Live").color(ACCENT));
                        let play_label = if self.playing { "■ Stop" } else { "▶ Play" };
                        if ui
                            .add(egui::Button::new(RichText::new(play_label).strong()).fill(ACCENT))
                            .clicked()
                        {
                            self.playing = !self.playing;
                            self.notice = if self.playing {
                                "Play session started".to_owned()
                            } else {
                                "Play session paused".to_owned()
                            };
                        }
                    });
                });
            });
    }

    fn show_status_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("studio_status_bar")
            .exact_size(STATUS_BAR_HEIGHT)
            .frame(panel_frame(PANEL_RAISED).inner_margin(Margin::symmetric(12, 0)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(STATUS_BAR_HEIGHT);
                    ui.label(RichText::new("✓").color(ACCENT));
                    ui.label(RichText::new(&self.notice).color(TEXT));
                    ui.add_space(12.0);
                    ui.label(RichText::new("1 warning").color(WARNING));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("60 fps").color(MUTED));
                        ui.separator();
                        ui.label(RichText::new("Metal · High quality").color(MUTED));
                    });
                });
            });
    }

    fn show_world(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("world_assets")
            .resizable(true)
            .default_size(178.0)
            .size_range(120.0..=280.0)
            .frame(panel_frame(PANEL))
            .show(root, |ui| self.asset_shelf(ui));

        egui::Panel::left("world_scene")
            .resizable(true)
            .default_size(222.0)
            .size_range(180.0..=340.0)
            .frame(panel_frame(PANEL))
            .show(root, |ui| self.scene_tree(ui));

        egui::Panel::right("world_inspector")
            .resizable(true)
            .default_size(286.0)
            .size_range(240.0..=380.0)
            .frame(panel_frame(PANEL))
            .show(root, |ui| self.inspector(ui));

        self.viewport_panel(root, "Perspective", "Viewport");
    }

    fn show_assets(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("asset_categories")
            .resizable(true)
            .default_size(220.0)
            .frame(panel_frame(PANEL))
            .show(root, |ui| {
                panel_title(ui, "Library");
                ui.add_space(8.0);
                for filter in ["All", "Images", "Materials", "Characters"] {
                    if ui
                        .selectable_label(self.asset_filter == filter, filter)
                        .clicked()
                    {
                        self.asset_filter = filter;
                        self.notice = format!("Showing {filter}");
                    }
                }
            });
        egui::Panel::right("asset_details")
            .resizable(true)
            .default_size(286.0)
            .frame(panel_frame(PANEL))
            .show(root, |ui| {
                panel_title(ui, "Asset details");
                ui.add_space(12.0);
                ui.label(RichText::new(self.selected_asset).strong().color(TEXT));
                property_row(ui, "Type", "Image");
                property_row(ui, "Status", "Valid");
                property_row(ui, "Used by", "3 materials");
                ui.add_space(12.0);
                ui.label(RichText::new("Drop a replacement here").color(MUTED));
            });
        egui::CentralPanel::default()
            .frame(panel_frame(SURFACE))
            .show(root, |ui| {
                panel_title(ui, "Assets");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [ui.available_width().max(120.0), 30.0],
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Search assets…"),
                    );
                });
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    for asset in ["forest-grass", "forest-wood", "campfire", "tree", "castle"] {
                        asset_tile(
                            ui,
                            asset,
                            self.selected_asset == asset,
                            &mut self.selected_asset,
                        );
                    }
                });
            });
    }

    fn show_materials(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("material_list")
            .resizable(true)
            .default_size(220.0)
            .frame(panel_frame(PANEL))
            .show(root, |ui| {
                panel_title(ui, "Materials");
                ui.add_space(8.0);
                for material in ["forest-grass", "forest-wood", "campfire"] {
                    if ui
                        .selectable_label(self.selected_asset == material, material)
                        .clicked()
                    {
                        self.selected_asset = material;
                        self.notice = format!("Selected {material}");
                    }
                }
            });
        egui::Panel::right("material_inspector")
            .resizable(true)
            .default_size(300.0)
            .frame(panel_frame(PANEL))
            .show(root, |ui| {
                panel_title(ui, "Material");
                ui.add_space(10.0);
                property_row(ui, "Image", "forest-grass");
                ui.add_space(8.0);
                ui.label(RichText::new("Roughness").color(MUTED));
                if ui
                    .add(egui::Slider::new(&mut self.roughness, 0.0..=1.0))
                    .changed()
                {
                    self.notice = "Material controls are preview only".to_owned();
                }
                property_row(ui, "Tile U", "8.0");
                property_row(ui, "Tile V", "8.0");
            });
        self.viewport_panel(root, "Daylight", "Material preview");
    }

    fn show_test(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("test_tools")
            .resizable(true)
            .default_size(138.0)
            .size_range(100.0..=260.0)
            .frame(panel_frame(PANEL))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    for tool in ["Sessions", "State", "Network", "Logs", "Performance"] {
                        if workspace_tab(ui, tool, self.test_tool == tool).clicked() {
                            self.test_tool = tool;
                            self.notice = format!("{tool} inspector preview");
                        }
                    }
                });
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("{} tools will appear here.", self.test_tool))
                        .color(MUTED),
                );
            });

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                let gap = 1.0;
                let half_width = (available.width() - gap) * 0.5;
                let half_height = (available.height() - gap) * 0.5;
                let rects = [
                    Rect::from_min_size(available.min, Vec2::new(half_width, half_height)),
                    Rect::from_min_size(
                        egui::pos2(available.min.x + half_width + gap, available.min.y),
                        Vec2::new(half_width, half_height),
                    ),
                    Rect::from_min_size(
                        egui::pos2(available.min.x, available.min.y + half_height + gap),
                        Vec2::new(half_width, half_height),
                    ),
                    Rect::from_min_size(
                        egui::pos2(
                            available.min.x + half_width + gap,
                            available.min.y + half_height + gap,
                        ),
                        Vec2::new(half_width, half_height),
                    ),
                ];
                for (index, rect) in rects.into_iter().enumerate() {
                    let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 32.0));
                    if index > 0 {
                        ui.painter().rect_filled(rect, 0.0, SURFACE);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Session preview",
                            FontId::proportional(12.0),
                            MUTED,
                        );
                    }
                    ui.painter().rect_filled(header, 0.0, PANEL_RAISED);
                    ui.painter().text(
                        header.left_center() + egui::vec2(12.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("Player {}", index + 1),
                        FontId::proportional(12.0),
                        TEXT,
                    );
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(1.0, BORDER),
                        StrokeKind::Inside,
                    );
                    if index == 0 {
                        self.runtime_viewport = Rect::from_min_max(
                            egui::pos2(rect.min.x + 1.0, header.max.y),
                            egui::pos2(rect.max.x - 1.0, rect.max.y - 1.0),
                        );
                    }
                }
            });
    }

    fn viewport_panel(&mut self, root: &mut egui::Ui, mode: &str, title: &str) {
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                let header = Rect::from_min_size(available.min, Vec2::new(available.width(), 34.0));
                ui.painter().rect_filled(header, 0.0, PANEL_RAISED);
                ui.painter().text(
                    header.left_center() + egui::vec2(12.0, 0.0),
                    Align2::LEFT_CENTER,
                    title.to_uppercase(),
                    FontId::proportional(11.0),
                    MUTED,
                );
                ui.painter().text(
                    header.right_center() - egui::vec2(12.0, 0.0),
                    Align2::RIGHT_CENTER,
                    mode,
                    FontId::proportional(11.0),
                    TEXT,
                );
                self.runtime_viewport = Rect::from_min_max(
                    egui::pos2(available.min.x + 1.0, header.max.y),
                    egui::pos2(available.max.x - 1.0, available.max.y - 1.0),
                );
                ui.painter().rect_stroke(
                    available,
                    0.0,
                    Stroke::new(1.0, BORDER),
                    StrokeKind::Inside,
                );
                ui.allocate_rect(self.runtime_viewport, Sense::hover());
            });
    }

    fn scene_tree(&mut self, ui: &mut egui::Ui) {
        panel_title(ui, "Scene");
        ui.add_space(8.0);
        scene_row(ui, 0, "▾", "World", &mut self.selected_scene);
        scene_row(ui, 1, "◈", "Environment", &mut self.selected_scene);
        scene_row(ui, 1, "▾", "Village", &mut self.selected_scene);
        scene_row(ui, 2, "◇", "House 01", &mut self.selected_scene);
        scene_row(ui, 2, "◇", "Well", &mut self.selected_scene);
        scene_row(ui, 1, "▾", "Forest", &mut self.selected_scene);
        scene_row(ui, 2, "◇", "Tree 013", &mut self.selected_scene);
        scene_row(ui, 2, "◇", "Tree 014", &mut self.selected_scene);
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        panel_title(ui, "Inspector");
        ui.add_space(8.0);
        ui.label(
            RichText::new(self.selected_scene)
                .strong()
                .size(15.0)
                .color(TEXT),
        );
        ui.add_space(12.0);
        section_label(ui, "Transform");
        ui.add_space(6.0);
        for (axis, value) in ["X", "Y", "Z"].into_iter().zip(&mut self.position) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(axis).color(MUTED));
                ui.add(egui::DragValue::new(value).speed(0.1));
            });
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Rotation").color(MUTED));
            ui.add(egui::DragValue::new(&mut self.rotation).suffix("°"));
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Scale").color(MUTED));
            ui.add(
                egui::DragValue::new(&mut self.scale)
                    .speed(0.01)
                    .range(0.01..=100.0),
            );
        });
        ui.add_space(14.0);
        section_label(ui, "Material");
        property_row(ui, "Surface", "forest-wood");
        ui.add_space(14.0);
        section_label(ui, "Collision");
        property_row(ui, "Mode", "Automatic");
    }

    fn asset_shelf(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            panel_title(ui, "Assets");
            ui.add_space(12.0);
            for filter in ["All", "Images", "Materials", "Characters"] {
                if ui
                    .selectable_label(self.asset_filter == filter, filter)
                    .clicked()
                {
                    self.asset_filter = filter;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_sized(
                    [170.0, 26.0],
                    egui::TextEdit::singleline(&mut self.search_query).hint_text("Search…"),
                );
            });
        });
        ui.separator();
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for asset in ["forest-grass", "forest-wood", "campfire", "tree", "castle"] {
                asset_tile(
                    ui,
                    asset,
                    self.selected_asset == asset,
                    &mut self.selected_asset,
                );
            }
        });
    }
}

fn configure_style(context: &egui::Context) {
    let mut style = (*context.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 5.0);
    style.spacing.interact_size.y = 28.0;
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL_RAISED;
    style.visuals.extreme_bg_color = SURFACE_DEEP;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.selection.bg_fill = Color32::from_rgb(45, 91, 84);
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(187, 192, 200));
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 53, 59);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(55, 61, 67);
    style.visuals.widgets.open.bg_fill = Color32::from_rgb(44, 48, 54);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    context.set_style_of(egui::Theme::Dark, style);
    context.set_theme(egui::ThemePreference::Dark);
}

fn panel_frame(fill: Color32) -> Frame {
    Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(Margin::same(12))
}

fn workspace_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let response = ui.add(
        egui::Button::new(RichText::new(label).color(if selected { TEXT } else { MUTED }))
            .frame(false),
    );
    if selected {
        let underline = Rect::from_min_max(
            egui::pos2(response.rect.min.x + 5.0, response.rect.max.y + 6.0),
            egui::pos2(response.rect.max.x - 5.0, response.rect.max.y + 8.0),
        );
        ui.painter().rect_filled(underline, 1.0, ACCENT);
    }
    response
}

fn panel_title(ui: &mut egui::Ui, title: &str) {
    ui.label(
        RichText::new(title.to_uppercase())
            .size(11.0)
            .strong()
            .color(MUTED),
    );
}

fn section_label(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).strong().color(TEXT));
    ui.separator();
}

fn property_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        ui.label(RichText::new(label).color(MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).color(TEXT));
        });
    });
}

fn scene_row(
    ui: &mut egui::Ui,
    depth: usize,
    icon: &str,
    name: &'static str,
    selected: &mut &'static str,
) {
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 15.0);
        ui.label(RichText::new(icon).color(MUTED));
        if ui.selectable_label(*selected == name, name).clicked() {
            *selected = name;
        }
    });
}

fn asset_tile(ui: &mut egui::Ui, name: &'static str, selected: bool, selection: &mut &'static str) {
    let fill = if selected {
        Color32::from_rgb(42, 71, 68)
    } else {
        PANEL_RAISED
    };
    let response = ui.add_sized(
        [116.0, 76.0],
        egui::Button::new(RichText::new(name).size(11.0).color(TEXT))
            .fill(fill)
            .stroke(Stroke::new(1.0, if selected { ACCENT } else { BORDER })),
    );
    if response.clicked() {
        *selection = name;
    }
}
