use cubacadabra_engine::native::Renderer as GameRenderer;
use egui::{
    Align, Align2, Color32, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Stroke,
    StrokeKind, TextStyle, Vec2,
};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor, wgpu};
use egui_winit::State as EguiState;
use std::time::Duration;
use winit::{event::WindowEvent, window::Window};

#[cfg(test)]
mod tests;

const TOP_BAR_HEIGHT: f32 = 30.0;
const STATUS_BAR_HEIGHT: f32 = 20.0;
const EDITOR_HEADER_HEIGHT: f32 = 24.0;
const CONTROL_HEIGHT: f32 = 20.0;
const LABEL_PADDING: f32 = 10.0;
const PANEL: Color32 = Color32::from_rgb(49, 49, 49);
const PANEL_RAISED: Color32 = Color32::from_rgb(58, 58, 58);
const PANEL_HEADER: Color32 = Color32::from_rgb(55, 55, 55);
const SURFACE: Color32 = Color32::from_rgb(40, 40, 40);
const SURFACE_DEEP: Color32 = Color32::from_rgb(29, 29, 29);
const FIELD: Color32 = Color32::from_rgb(70, 70, 70);
const BORDER: Color32 = Color32::from_rgb(29, 29, 29);
const BORDER_STRONG: Color32 = Color32::from_rgb(87, 87, 87);
const TEXT: Color32 = Color32::from_rgb(224, 224, 224);
const MUTED: Color32 = Color32::from_rgb(185, 185, 185);
const FAINT: Color32 = Color32::from_rgb(143, 143, 143);
const ACCENT: Color32 = Color32::from_rgb(137, 177, 218);
const ACCENT_DARK: Color32 = Color32::from_rgb(65, 88, 115);
const LIVE: Color32 = Color32::from_rgb(133, 186, 153);

#[derive(Clone, Copy, Debug)]
enum Icon {
    Project,
    World,
    Assets,
    Material,
    Test,
    Folder,
    Object,
    Image,
    Character,
    Search,
    Filter,
    Grid,
    Camera,
    Sliders,
    More,
    Plus,
    Play,
    Stop,
    Check,
    ChevronDown,
    ChevronRight,
    Eye,
    Lock,
    Save,
    Open,
    Undo,
    Redo,
    Settings,
    Network,
    Logs,
    Gauge,
}

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
            .frame(editor_frame(SURFACE).inner_margin(Margin::symmetric(6, 0)))
            .show(root, |ui| {
                egui::MenuBar::new().style(menu_bar_style).ui(ui, |ui| {
                    let logo = ui.allocate_response(egui::vec2(22.0, 24.0), Sense::hover());
                    paint_icon(
                        ui.painter(),
                        logo.rect.shrink2(egui::vec2(4.0, 5.0)),
                        Icon::Project,
                        TEXT,
                    );

                    ui.menu_button("File", |ui| {
                        ui.set_min_width(220.0);
                        if menu_entry(ui, Icon::Open, "Open Project…", "⌘O", true).clicked() {
                            self.notice = "Open Project is a layout preview".to_owned();
                            ui.close();
                        }
                        if menu_entry(ui, Icon::Save, "Save", "⌘S", true).clicked() {
                            self.notice = "Nothing to save yet".to_owned();
                            ui.close();
                        }
                        ui.separator();
                        if menu_entry(ui, Icon::Folder, "Reveal Project", "", true).clicked() {
                            self.notice = "Reveal Project is not connected yet".to_owned();
                            ui.close();
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        ui.set_min_width(220.0);
                        menu_entry(ui, Icon::Undo, "Undo", "⌘Z", false);
                        menu_entry(ui, Icon::Redo, "Redo", "⇧⌘Z", false);
                        ui.separator();
                        if menu_entry(ui, Icon::Settings, "Preferences…", "⌘,", true).clicked()
                        {
                            self.notice = "Preferences are coming later".to_owned();
                            ui.close();
                        }
                    });
                    ui.menu_button("Window", |ui| {
                        ui.set_min_width(220.0);
                        if menu_entry(ui, Icon::Grid, "Maximize Viewport", "Space", true).clicked()
                        {
                            self.notice = "Viewport maximize is coming later".to_owned();
                            ui.close();
                        }
                        if menu_entry(ui, Icon::Sliders, "Reset Layout", "", true).clicked() {
                            self.notice = "Layout reset".to_owned();
                            ui.close();
                        }
                    });

                    ui.add_space(12.0);
                    for workspace in Workspace::ALL {
                        if workspace_tab(ui, workspace.label(), self.workspace == workspace)
                            .clicked()
                        {
                            self.workspace = workspace;
                            self.notice = format!("{} workspace", workspace.label());
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let live =
                            ui.allocate_response(egui::vec2(43.0, CONTROL_HEIGHT), Sense::hover());
                        paint_status_label(ui, live.rect, LIVE, "Live");
                        let play_icon = if self.playing { Icon::Stop } else { Icon::Play };
                        let play_label = if self.playing { "Stop" } else { "Play" };
                        if toolbar_button(ui, play_icon, play_label, self.playing).clicked() {
                            self.playing = !self.playing;
                            self.notice = if self.playing {
                                "Play session started".to_owned()
                            } else {
                                "Play session paused".to_owned()
                            };
                        }
                        if ui.available_width() > 180.0 {
                            vertical_separator(ui, 14.0);
                            ui.label(RichText::new(project_name).size(11.0).color(FAINT));
                        }
                    });
                });
            });
    }

    fn show_status_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("studio_status_bar")
            .exact_size(STATUS_BAR_HEIGHT)
            .frame(editor_frame(PANEL_RAISED).inner_margin(Margin::symmetric(8, 0)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(STATUS_BAR_HEIGHT);
                    ui.spacing_mut().interact_size.y = 16.0;
                    inline_icon(ui, Icon::Check, MUTED);
                    ui.label(RichText::new(&self.notice).size(10.5).color(MUTED));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("Layout preview").size(10.0).color(FAINT));
                        vertical_separator(ui, 12.0);
                        ui.label(RichText::new("Metal").size(10.0).color(FAINT));
                    });
                });
            });
    }

    fn show_world(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("world_assets")
            .resizable(true)
            .default_size(136.0)
            .size_range(120.0..=300.0)
            .frame(editor_frame(PANEL))
            .show(root, |ui| self.asset_shelf(ui));

        egui::Panel::left("world_scene")
            .resizable(true)
            .default_size(208.0)
            .size_range(180.0..=340.0)
            .frame(editor_frame(PANEL))
            .show(root, |ui| self.scene_tree(ui));

        egui::Panel::right("world_inspector")
            .resizable(true)
            .default_size(256.0)
            .size_range(224.0..=340.0)
            .frame(editor_frame(PANEL_RAISED))
            .show(root, |ui| self.inspector(ui));

        self.viewport_panel(root, "Perspective", "Viewport");
    }

    fn show_assets(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("asset_categories")
            .resizable(true)
            .default_size(220.0)
            .frame(editor_frame(PANEL))
            .show(root, |ui| {
                panel_header(ui, Icon::Folder, "Library", |ui| {
                    icon_button(ui, Icon::Plus, "Add source", false);
                });
                content_frame().show(ui, |ui| {
                    for (filter, icon) in [
                        ("All", Icon::Grid),
                        ("Images", Icon::Image),
                        ("Materials", Icon::Material),
                        ("Characters", Icon::Character),
                    ] {
                        if navigation_row(ui, icon, filter, self.asset_filter == filter).clicked() {
                            self.asset_filter = filter;
                            self.notice = format!("Showing {filter}");
                        }
                    }
                });
            });
        egui::Panel::right("asset_details")
            .resizable(true)
            .default_size(256.0)
            .size_range(224.0..=340.0)
            .frame(editor_frame(PANEL_RAISED))
            .show(root, |ui| {
                panel_header(ui, Icon::Sliders, "Asset details", |ui| {
                    icon_button(ui, Icon::More, "Asset options", false);
                });
                content_frame().show(ui, |ui| {
                    selected_object_header(ui, self.selected_asset, "Asset preview");
                    ui.add_space(4.0);
                    property_section(ui, "File", |ui| {
                        property_row(ui, "Type", asset_kind(self.selected_asset).1);
                        property_row(ui, "Status", "Preview");
                    });
                    ui.add_space(4.0);
                    drop_target(ui, "Drop a replacement file");
                });
            });
        egui::CentralPanel::default()
            .frame(editor_frame(SURFACE))
            .show(root, |ui| {
                panel_header(ui, Icon::Assets, "Assets", |ui| {
                    icon_button(ui, Icon::Grid, "Grid view", true);
                    icon_button(ui, Icon::More, "Asset options", false);
                });
                content_frame().show(ui, |ui| {
                    search_field(ui, &mut self.search_query, ui.available_width());
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
            });
    }

    fn show_materials(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("material_list")
            .resizable(true)
            .default_size(220.0)
            .frame(editor_frame(PANEL))
            .show(root, |ui| {
                panel_header(ui, Icon::Material, "Materials", |ui| {
                    icon_button(ui, Icon::Plus, "New material", false);
                });
                content_frame().show(ui, |ui| {
                    for material in ["forest-grass", "forest-wood", "campfire"] {
                        if navigation_row(
                            ui,
                            Icon::Material,
                            material,
                            self.selected_asset == material,
                        )
                        .clicked()
                        {
                            self.selected_asset = material;
                            self.notice = format!("Selected {material}");
                        }
                    }
                });
            });
        egui::Panel::right("material_inspector")
            .resizable(true)
            .default_size(256.0)
            .size_range(224.0..=340.0)
            .frame(editor_frame(PANEL_RAISED))
            .show(root, |ui| {
                panel_header(ui, Icon::Sliders, "Material", |ui| {
                    icon_button(ui, Icon::More, "Material options", false);
                });
                content_frame().show(ui, |ui| {
                    selected_object_header(ui, self.selected_asset, "Surface material");
                    ui.add_space(4.0);
                    property_section(ui, "Texture", |ui| {
                        property_row(ui, "Image", self.selected_asset);
                    });
                    property_section(ui, "Surface", |ui| {
                        ui.label(RichText::new("Roughness").size(11.0).color(MUTED));
                        if ui
                            .add(egui::Slider::new(&mut self.roughness, 0.0..=1.0))
                            .changed()
                        {
                            self.notice = "Material controls are preview only".to_owned();
                        }
                        property_row(ui, "Tile U", "8.0");
                        property_row(ui, "Tile V", "8.0");
                    });
                });
            });
        self.viewport_panel(root, "Daylight", "Material preview");
    }

    fn show_test(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("test_tools")
            .resizable(true)
            .default_size(112.0)
            .size_range(80.0..=260.0)
            .frame(editor_frame(PANEL))
            .show(root, |ui| {
                editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Test, MUTED);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for tool in ["Sessions", "State", "Network", "Logs", "Performance"] {
                        if compact_tab(ui, tool, self.test_tool == tool).clicked() {
                            self.test_tool = tool;
                            self.notice = format!("{tool} inspector preview");
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, Icon::More, "Test options", false);
                    });
                });
                content_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        inline_icon(ui, tool_icon(self.test_tool), FAINT);
                        ui.label(
                            RichText::new(format!("{} tools will appear here.", self.test_tool))
                                .size(11.5)
                                .color(MUTED),
                        );
                    });
                });
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
                    let header = Rect::from_min_size(
                        rect.min,
                        Vec2::new(rect.width(), EDITOR_HEADER_HEIGHT),
                    );
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
                    ui.painter().rect_filled(header, 0.0, PANEL_HEADER);
                    let icon_rect = Rect::from_center_size(
                        header.left_center() + egui::vec2(14.0, 0.0),
                        Vec2::splat(14.0),
                    );
                    paint_icon(ui.painter(), icon_rect, Icon::Camera, FAINT);
                    ui.painter().text(
                        header.left_center() + egui::vec2(27.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("Player {}", index + 1),
                        FontId::proportional(11.5),
                        TEXT,
                    );
                    let status_center = header.right_center() - egui::vec2(13.0, 0.0);
                    ui.painter().circle_filled(
                        status_center,
                        3.0,
                        if index == 0 { ACCENT } else { FAINT },
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
                let header = editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Camera, MUTED);
                    ui.label(RichText::new(title).size(11.0).color(TEXT));
                    vertical_separator(ui, 12.0);
                    ui.label(RichText::new(mode).size(11.0).color(TEXT));
                    paint_down_chevron(ui);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, Icon::More, "Viewport options", false);
                        icon_button(ui, Icon::Camera, "Camera view", false);
                        icon_button(ui, Icon::Sliders, "Viewport shading", false);
                        icon_button(ui, Icon::Grid, "Toggle grid", true);
                    });
                });
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
        panel_header(ui, Icon::World, "Scene", |ui| {
            icon_button(ui, Icon::Filter, "Filter scene", false);
            icon_button(ui, Icon::Plus, "Add object", false);
        });
        Frame::NONE
            .inner_margin(Margin::symmetric(0, 4))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                scene_row(ui, 0, Icon::World, true, "World", &mut self.selected_scene);
                scene_row(
                    ui,
                    1,
                    Icon::Folder,
                    false,
                    "Environment",
                    &mut self.selected_scene,
                );
                scene_row(
                    ui,
                    1,
                    Icon::Folder,
                    true,
                    "Village",
                    &mut self.selected_scene,
                );
                scene_row(
                    ui,
                    2,
                    Icon::Object,
                    false,
                    "House 01",
                    &mut self.selected_scene,
                );
                scene_row(ui, 2, Icon::Object, false, "Well", &mut self.selected_scene);
                scene_row(
                    ui,
                    1,
                    Icon::Folder,
                    true,
                    "Forest",
                    &mut self.selected_scene,
                );
                scene_row(
                    ui,
                    2,
                    Icon::Object,
                    false,
                    "Tree 013",
                    &mut self.selected_scene,
                );
                scene_row(
                    ui,
                    2,
                    Icon::Object,
                    false,
                    "Tree 014",
                    &mut self.selected_scene,
                );
            });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        panel_header(ui, Icon::Sliders, "Inspector", |ui| {
            icon_button(ui, Icon::Lock, "Lock inspector", false);
            icon_button(ui, Icon::More, "Inspector options", false);
        });
        content_frame().show(ui, |ui| {
            selected_object_header(ui, self.selected_scene, "Mesh instance");
            ui.add_space(4.0);
            property_section(ui, "Transform", |ui| {
                for (index, (axis, value)) in ["X", "Y", "Z"]
                    .into_iter()
                    .zip(&mut self.position)
                    .enumerate()
                {
                    drag_property_row(
                        ui,
                        if index == 0 { "Position" } else { "" },
                        axis,
                        value,
                        0.1,
                        "",
                    );
                }
                ui.add_space(4.0);
                drag_property_row(ui, "Rotation", "", &mut self.rotation, 0.5, "°");
                drag_property_row(ui, "Scale", "", &mut self.scale, 0.01, "");
            });
            property_section(ui, "Material", |ui| {
                property_row(ui, "Surface", "forest-wood");
            });
            property_section(ui, "Collision", |ui| {
                property_row(ui, "Mode", "Automatic");
            });
        });
    }

    fn asset_shelf(&mut self, ui: &mut egui::Ui) {
        editor_header(ui, |ui| {
            inline_icon(ui, Icon::Assets, MUTED);
            ui.label(RichText::new("Assets").size(11.0).color(TEXT));
            vertical_separator(ui, 12.0);
            ui.spacing_mut().item_spacing.x = 0.0;
            for filter in ["All", "Images", "Materials", "Characters"] {
                if compact_tab(ui, filter, self.asset_filter == filter).clicked() {
                    self.asset_filter = filter;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                icon_button(ui, Icon::Grid, "Grid view", true);
                search_field(ui, &mut self.search_query, 190.0);
            });
        });
        Frame::NONE
            .inner_margin(Margin::symmetric(8, 8))
            .show(ui, |ui| {
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
}

fn configure_style(context: &egui::Context) {
    let mut style = (*context.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 2.0);
    style.spacing.button_padding = egui::vec2(8.0, 2.0);
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.spacing.indent = 12.0;
    style.spacing.menu_margin = Margin::same(4);
    style.animation_time = 0.15;
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL_RAISED;
    style.visuals.window_stroke = Stroke::new(1.0, BORDER_STRONG);
    style.visuals.window_corner_radius = egui::CornerRadius::same(4);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(3);
    style.visuals.extreme_bg_color = SURFACE_DEEP;
    style.visuals.text_edit_bg_color = Some(FIELD);
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.indent_has_left_vline = false;
    style.visuals.selection.bg_fill = ACCENT_DARK;
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(2);
    style.visuals.widgets.inactive.bg_fill = FIELD;
    style.visuals.widgets.inactive.weak_bg_fill = FIELD;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(2);
    style.visuals.widgets.hovered.bg_fill = BORDER_STRONG;
    style.visuals.widgets.hovered.weak_bg_fill = BORDER_STRONG;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(2);
    style.visuals.widgets.active.bg_fill = ACCENT_DARK;
    style.visuals.widgets.active.weak_bg_fill = ACCENT_DARK;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(2);
    style.visuals.widgets.open.bg_fill = FIELD;
    style.visuals.widgets.open.weak_bg_fill = FIELD;
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(2);
    // Egui removes text padding for frameless buttons, including menu labels.
    // Keep the frame geometry and make inactive menu frames transparent instead.
    style.visuals.button_frame = true;
    style.visuals.slider_trailing_fill = true;
    style.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(11.5));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(10.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(11.0));
    context.set_style_of(egui::Theme::Dark, style);
    context.set_theme(egui::ThemePreference::Dark);
}

fn editor_frame(fill: Color32) -> Frame {
    Frame::NONE.fill(fill).inner_margin(Margin::same(0))
}

fn menu_bar_style(style: &mut egui::Style) {
    style.spacing.item_spacing.x = 0.0;
    style.spacing.button_padding = egui::vec2(LABEL_PADDING, 4.0);
    style.spacing.interact_size.y = TOP_BAR_HEIGHT;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.open.bg_stroke = Stroke::NONE;
}

fn content_frame() -> Frame {
    Frame::NONE.inner_margin(Margin::symmetric(8, 6))
}

fn workspace_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(11.5),
        if selected { TEXT } else { MUTED },
    );
    let width = galley.size().x.ceil() + LABEL_PADDING * 2.0;
    let (slot, response) =
        ui.allocate_exact_size(egui::vec2(width, TOP_BAR_HEIGHT), Sense::click());
    let rect = Rect::from_min_max(slot.min + egui::vec2(0.0, 5.0), slot.max);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if response.hovered() || response.has_focus() || selected {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius {
                nw: 3,
                ne: 3,
                sw: 0,
                se: 0,
            },
            if selected { PANEL_HEADER } else { PANEL_RAISED },
        );
    }
    ui.painter().galley(
        egui::pos2(
            rect.min.x + LABEL_PADDING,
            slot.center().y - galley.size().y * 0.5,
        ),
        galley,
        TEXT,
    );
    paint_focus(ui, &response);
    response
}

// Allocate headers once: frame strokes/margins must never change their height.
fn editor_header(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) -> Rect {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), EDITOR_HEADER_HEIGHT),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, PANEL_HEADER);
    ui.painter()
        .hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, BORDER));
    let mut header = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    header.set_clip_rect(ui.clip_rect().intersect(rect));
    header.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
    content(&mut header);
    rect
}

fn panel_header(ui: &mut egui::Ui, icon: Icon, title: &str, actions: impl FnOnce(&mut egui::Ui)) {
    editor_header(ui, |ui| {
        inline_icon(ui, icon, MUTED);
        ui.label(RichText::new(title).size(11.0).color(TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), actions);
    });
}

fn selected_object_header(ui: &mut egui::Ui, name: &str, kind: &str) {
    ui.horizontal(|ui| {
        inline_icon(ui, Icon::Object, MUTED);
        ui.add(egui::Label::new(RichText::new(name).size(11.5).color(TEXT)).truncate())
            .on_hover_text(kind);
    });
}

fn property_section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).size(11.5).color(TEXT))
        .default_open(true)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            content(ui);
            ui.add_space(6.0);
        });
}

fn property_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let field = property_field(ui, label);
    ui.painter().rect_filled(field, 2.0, SURFACE);
    ui.put(
        field.shrink2(egui::vec2(6.0, 0.0)),
        egui::Label::new(RichText::new(value).size(11.0).color(TEXT)).truncate(),
    );
}

fn property_field(ui: &mut egui::Ui, label: &str) -> Rect {
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CONTROL_HEIGHT),
        Sense::hover(),
    );
    let label_width = 64.0;
    ui.painter().text(
        egui::pos2(row.min.x + label_width - 8.0, row.center().y),
        Align2::RIGHT_CENTER,
        label,
        FontId::proportional(11.0),
        MUTED,
    );
    Rect::from_min_max(row.min + egui::vec2(label_width, 0.0), row.max)
}

fn drag_property_row(
    ui: &mut egui::Ui,
    label: &str,
    axis: &str,
    value: &mut f32,
    speed: f64,
    suffix: &str,
) {
    let field = property_field(ui, label);
    ui.painter().rect_filled(field, 2.0, FIELD);
    let mut value_rect = field;
    if !axis.is_empty() {
        value_rect.min.x += 20.0;
        ui.painter().text(
            field.left_center() + egui::vec2(10.0, 0.0),
            Align2::CENTER_CENTER,
            axis,
            FontId::proportional(10.5),
            axis_color(axis),
        );
    }
    ui.push_id((label, axis), |ui| {
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
        ui.put(
            value_rect,
            egui::DragValue::new(value)
                .speed(speed)
                .suffix(suffix)
                .min_decimals(2)
                .update_while_editing(false),
        )
        .on_hover_text(format!(
            "{} {axis} · drag to adjust; double-click to type (preview)",
            if label.is_empty() { "Position" } else { label }
        ));
    });
}

fn scene_row(
    ui: &mut egui::Ui,
    depth: usize,
    icon: Icon,
    expanded: bool,
    name: &'static str,
    selected: &mut &'static str,
) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CONTROL_HEIGHT),
        Sense::click(),
    );
    let is_selected = *selected == name;
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, is_selected, name)
    });
    if is_selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0.0,
            if is_selected {
                ACCENT_DARK
            } else {
                PANEL_RAISED
            },
        );
    }
    paint_focus(ui, &response);
    let x = rect.min.x + 6.0 + depth as f32 * 12.0;
    if matches!(icon, Icon::Folder | Icon::World) {
        paint_icon(
            ui.painter(),
            Rect::from_center_size(egui::pos2(x + 5.0, rect.center().y), Vec2::splat(10.0)),
            if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            FAINT,
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(egui::pos2(x + 19.0, rect.center().y), Vec2::splat(13.0)),
        icon,
        if is_selected { TEXT } else { MUTED },
    );
    ui.painter().text(
        egui::pos2(x + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(11.5),
        TEXT,
    );
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.right_center() - egui::vec2(9.0, 0.0),
            Vec2::splat(12.0),
        ),
        Icon::Eye,
        FAINT,
    );
    if response.clicked() {
        *selected = name;
    }
}

fn asset_tile(ui: &mut egui::Ui, name: &'static str, selected: bool, selection: &mut &'static str) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(104.0, 88.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, name)
    });
    let border = if selected {
        ACCENT
    } else if response.hovered() {
        BORDER_STRONG
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, 2.0, if selected { ACCENT_DARK } else { PANEL });
    let preview = Rect::from_min_max(
        rect.min + egui::vec2(3.0, 3.0),
        egui::pos2(rect.max.x - 3.0, rect.max.y - 23.0),
    );
    ui.painter().rect_filled(preview, 2.0, SURFACE);
    let (asset_icon, kind) = asset_kind(name);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(preview.center(), Vec2::splat(26.0)),
        asset_icon,
        MUTED,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y - 11.0),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional(10.5),
        TEXT,
    );
    ui.painter()
        .rect_stroke(rect, 2.0, Stroke::new(1.0, border), StrokeKind::Inside);
    paint_focus(ui, &response);
    if response.clicked() {
        *selection = name;
    }
    response.on_hover_text(format!("{name} · {} preview", kind.to_lowercase()));
}

fn compact_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(11.0),
        if selected { TEXT } else { MUTED },
    );
    let width = galley.size().x.ceil() + 16.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, if selected { FIELD } else { PANEL_RAISED });
    }
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, TEXT);
    paint_focus(ui, &response);
    response
}

fn navigation_row(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, if selected { ACCENT_DARK } else { PANEL_RAISED });
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(11.0, 0.0),
            Vec2::splat(14.0),
        ),
        icon,
        if selected { ACCENT } else { MUTED },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(24.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.5),
        if selected { TEXT } else { MUTED },
    );
    paint_focus(ui, &response);
    response
}

fn search_field(ui: &mut egui::Ui, query: &mut String, width: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.min(ui.available_width()).max(80.0), CONTROL_HEIGHT),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, 3.0, SURFACE_DEEP);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(11.0, 0.0),
            Vec2::splat(12.0),
        ),
        Icon::Search,
        FAINT,
    );
    let text_rect = Rect::from_min_max(
        rect.min + egui::vec2(23.0, 1.0),
        rect.max - egui::vec2(4.0, 1.0),
    );
    let response = ui.put(
        text_rect,
        egui::TextEdit::singleline(query)
            .hint_text("Search assets…")
            .font(FontId::proportional(11.0))
            .margin(Margin::ZERO)
            .frame(Frame::NONE),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Search assets")
    });
}

fn drop_target(ui: &mut egui::Ui, label: &str) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 62.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        3.0,
        if response.hovered() {
            PANEL_RAISED
        } else {
            SURFACE
        },
    );
    ui.painter().rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, if response.hovered() { ACCENT } else { BORDER }),
        StrokeKind::Inside,
    );
    let icon_rect = Rect::from_center_size(rect.center() - egui::vec2(0.0, 9.0), Vec2::splat(16.0));
    paint_icon(ui.painter(), icon_rect, Icon::Open, MUTED);
    ui.painter().text(
        rect.center() + egui::vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.5),
        MUTED,
    );
}

fn menu_entry(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(220.0, 24.0), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if enabled && (response.hovered() || response.has_focus()) {
        ui.painter().rect_filled(rect, 2.0, ACCENT_DARK);
    }
    let color = if enabled { TEXT } else { FAINT };
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(13.0, 0.0),
            Vec2::splat(14.0),
        ),
        icon,
        if enabled { MUTED } else { FAINT },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(28.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.5),
        color,
    );
    if !shortcut.is_empty() {
        ui.painter().text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::proportional(10.0),
            FAINT,
        );
    }
    response
}

fn toolbar_button(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(58.0, CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    ui.painter().rect_filled(
        rect,
        2.0,
        if active {
            ACCENT_DARK
        } else if response.hovered() {
            PANEL_HEADER
        } else {
            PANEL_RAISED
        },
    );
    ui.painter().rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, if active { ACCENT } else { BORDER_STRONG }),
        StrokeKind::Inside,
    );
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(14.0, 0.0),
            Vec2::splat(13.0),
        ),
        icon,
        if active { ACCENT } else { TEXT },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(25.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.0),
        TEXT,
    );
    paint_focus(ui, &response);
    response
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str, active: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    if active || response.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, if active { FIELD } else { BORDER_STRONG });
    }
    paint_icon(
        ui.painter(),
        rect.shrink(3.0),
        icon,
        if active { TEXT } else { MUTED },
    );
    paint_focus(ui, &response);
    response.on_hover_text(tooltip)
}

fn paint_focus(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            2.0,
            Stroke::new(1.0, ACCENT),
            StrokeKind::Inside,
        );
    }
}

fn inline_icon(ui: &mut egui::Ui, icon: Icon, color: Color32) {
    let response = ui.allocate_response(Vec2::splat(14.0), Sense::hover());
    paint_icon(ui.painter(), response.rect, icon, color);
}

fn paint_status_label(ui: &egui::Ui, rect: Rect, color: Color32, label: &str) {
    ui.painter()
        .circle_filled(rect.left_center() + egui::vec2(9.0, 0.0), 3.0, color);
    ui.painter().text(
        rect.left_center() + egui::vec2(18.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(10.5),
        color,
    );
}

fn vertical_separator(ui: &mut egui::Ui, height: f32) {
    let response = ui.allocate_response(egui::vec2(1.0, height), Sense::hover());
    ui.painter().line_segment(
        [response.rect.center_top(), response.rect.center_bottom()],
        Stroke::new(1.0, BORDER),
    );
}

fn paint_down_chevron(ui: &mut egui::Ui) {
    let response = ui.allocate_response(Vec2::splat(12.0), Sense::hover());
    paint_icon(ui.painter(), response.rect, Icon::ChevronDown, FAINT);
}

fn axis_color(axis: &str) -> Color32 {
    match axis {
        "X" => Color32::from_rgb(218, 105, 105),
        "Y" => Color32::from_rgb(112, 193, 126),
        "Z" => Color32::from_rgb(103, 151, 218),
        _ => MUTED,
    }
}

fn tool_icon(tool: &str) -> Icon {
    match tool {
        "Sessions" => Icon::Test,
        "State" => Icon::Sliders,
        "Network" => Icon::Network,
        "Logs" => Icon::Logs,
        "Performance" => Icon::Gauge,
        _ => Icon::Test,
    }
}

fn asset_kind(name: &str) -> (Icon, &'static str) {
    if name.contains("grass") || name.contains("wood") {
        (Icon::Material, "MATERIAL")
    } else if name == "campfire" || name == "tree" || name == "castle" {
        (Icon::Object, "MODEL")
    } else {
        (Icon::Image, "IMAGE")
    }
}

fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let size = rect.width().min(rect.height()).max(1.0);
    let r = size * 0.42;
    let stroke = Stroke::new((size * 0.09).clamp(1.0, 1.5), color);
    let left = c.x - r;
    let right = c.x + r;
    let top = c.y - r;
    let bottom = c.y + r;
    match icon {
        Icon::Project | Icon::Object => {
            let top_point = egui::pos2(c.x, top);
            let left_point = egui::pos2(left, c.y - r * 0.5);
            let right_point = egui::pos2(right, c.y - r * 0.5);
            let bottom_point = egui::pos2(c.x, bottom);
            painter.add(egui::Shape::closed_line(
                vec![
                    top_point,
                    right_point,
                    egui::pos2(right, c.y + r * 0.5),
                    bottom_point,
                    egui::pos2(left, c.y + r * 0.5),
                    left_point,
                ],
                stroke,
            ));
            painter.line_segment([c, bottom_point], stroke);
            painter.line_segment([left_point, c], stroke);
            painter.line_segment([right_point, c], stroke);
        }
        Icon::World => {
            painter.circle_stroke(c, r, stroke);
            painter.line_segment([egui::pos2(left, c.y), egui::pos2(right, c.y)], stroke);
            painter.line_segment([egui::pos2(c.x, top), egui::pos2(c.x, bottom)], stroke);
            painter.circle_stroke(c, r * 0.52, Stroke::new(stroke.width * 0.75, color));
        }
        Icon::Assets | Icon::Grid | Icon::Test => {
            let cell = r * 0.72;
            for offset in [
                egui::vec2(-cell, -cell),
                egui::vec2(cell, -cell),
                egui::vec2(-cell, cell),
                egui::vec2(cell, cell),
            ] {
                painter.rect_stroke(
                    Rect::from_center_size(c + offset * 0.48, Vec2::splat(cell * 0.78)),
                    1.0,
                    stroke,
                    StrokeKind::Inside,
                );
            }
        }
        Icon::Material => {
            painter.circle_stroke(c, r, stroke);
            painter.circle_filled(c - egui::vec2(r * 0.22, r * 0.22), r * 0.22, color);
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.8, c.y + r * 0.5),
                    egui::pos2(c.x + r * 0.65, c.y - r * 0.55),
                ],
                stroke,
            );
        }
        Icon::Folder | Icon::Open => {
            let points = [
                egui::pos2(left, top + r * 0.25),
                egui::pos2(c.x - r * 0.2, top + r * 0.25),
                egui::pos2(c.x, top + r * 0.55),
                egui::pos2(right, top + r * 0.55),
                egui::pos2(right, bottom),
                egui::pos2(left, bottom),
                egui::pos2(left, top + r * 0.25),
            ];
            painter.add(egui::Shape::line(points.to_vec(), stroke));
            if matches!(icon, Icon::Open) {
                painter.line_segment(
                    [egui::pos2(c.x, c.y), egui::pos2(c.x, bottom + r * 0.18)],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(c.x - r * 0.25, bottom - r * 0.05),
                        egui::pos2(c.x, bottom + r * 0.18),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(c.x + r * 0.25, bottom - r * 0.05),
                        egui::pos2(c.x, bottom + r * 0.18),
                    ],
                    stroke,
                );
            }
        }
        Icon::Image => {
            painter.rect_stroke(rect.shrink(size * 0.08), 1.0, stroke, StrokeKind::Inside);
            painter.circle_filled(
                egui::pos2(right - r * 0.25, top + r * 0.28),
                r * 0.13,
                color,
            );
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left + r * 0.2, bottom - r * 0.2),
                    egui::pos2(c.x - r * 0.15, c.y),
                    egui::pos2(c.x + r * 0.12, c.y + r * 0.25),
                    egui::pos2(right - r * 0.1, c.y - r * 0.15),
                ],
                stroke,
            ));
        }
        Icon::Character => {
            painter.circle_stroke(egui::pos2(c.x, top + r * 0.32), r * 0.28, stroke);
            painter.line_segment(
                [egui::pos2(c.x, c.y), egui::pos2(c.x, bottom - r * 0.1)],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(left + r * 0.18, c.y + r * 0.2),
                    egui::pos2(right - r * 0.18, c.y + r * 0.2),
                ],
                stroke,
            );
        }
        Icon::Search => {
            painter.circle_stroke(c - egui::vec2(r * 0.15, r * 0.15), r * 0.56, stroke);
            painter.line_segment(
                [
                    c + egui::vec2(r * 0.25, r * 0.25),
                    egui::pos2(right, bottom),
                ],
                stroke,
            );
        }
        Icon::Filter => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left, top),
                    egui::pos2(right, top),
                    egui::pos2(c.x + r * 0.18, c.y),
                    egui::pos2(c.x + r * 0.18, bottom),
                    egui::pos2(c.x - r * 0.18, bottom - r * 0.2),
                    egui::pos2(c.x - r * 0.18, c.y),
                ],
                stroke,
            ));
        }
        Icon::Camera => {
            painter.rect_stroke(
                Rect::from_min_max(egui::pos2(left, top + r * 0.22), egui::pos2(right, bottom)),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.circle_stroke(c + egui::vec2(0.0, r * 0.1), r * 0.28, stroke);
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.45, top + r * 0.22),
                    egui::pos2(c.x - r * 0.2, top),
                ],
                stroke,
            );
        }
        Icon::Sliders => {
            for (index, y) in [-0.55_f32, 0.0, 0.55].into_iter().enumerate() {
                let y = c.y + r * y;
                painter.line_segment([egui::pos2(left, y), egui::pos2(right, y)], stroke);
                let knob = if index == 1 {
                    c.x - r * 0.35
                } else {
                    c.x + r * 0.28
                };
                painter.circle_filled(egui::pos2(knob, y), r * 0.13, color);
            }
        }
        Icon::More => {
            for x in [-0.55_f32, 0.0, 0.55] {
                painter.circle_filled(egui::pos2(c.x + r * x, c.y), r * 0.13, color);
            }
        }
        Icon::Plus => {
            painter.line_segment([egui::pos2(left, c.y), egui::pos2(right, c.y)], stroke);
            painter.line_segment([egui::pos2(c.x, top), egui::pos2(c.x, bottom)], stroke);
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(left + r * 0.25, top),
                    egui::pos2(right, c.y),
                    egui::pos2(left + r * 0.25, bottom),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(r * 1.35)), 1.0, color);
        }
        Icon::Check => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left, c.y),
                    egui::pos2(c.x - r * 0.15, bottom - r * 0.1),
                    egui::pos2(right, top + r * 0.08),
                ],
                stroke,
            ));
        }
        Icon::ChevronDown => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left, c.y - r * 0.2),
                    egui::pos2(c.x, c.y + r * 0.3),
                    egui::pos2(right, c.y - r * 0.2),
                ],
                stroke,
            ));
        }
        Icon::ChevronRight => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - r * 0.2, top),
                    egui::pos2(c.x + r * 0.3, c.y),
                    egui::pos2(c.x - r * 0.2, bottom),
                ],
                stroke,
            ));
        }
        Icon::Eye => {
            for direction in [-1.0, 1.0] {
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                    [
                        egui::pos2(left, c.y),
                        egui::pos2(c.x - r * 0.4, c.y + direction * r * 0.9),
                        egui::pos2(c.x + r * 0.4, c.y + direction * r * 0.9),
                        egui::pos2(right, c.y),
                    ],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ));
            }
            painter.circle_stroke(c, r * 0.27, stroke);
        }
        Icon::Lock => {
            painter.rect_stroke(
                Rect::from_min_max(egui::pos2(left, c.y - r * 0.05), egui::pos2(right, bottom)),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - r * 0.5, c.y - r * 0.05),
                    egui::pos2(c.x - r * 0.5, top + r * 0.35),
                    egui::pos2(c.x + r * 0.5, top + r * 0.35),
                    egui::pos2(c.x + r * 0.5, c.y - r * 0.05),
                ],
                stroke,
            ));
        }
        Icon::Save => {
            painter.rect_stroke(rect.shrink(size * 0.08), 1.0, stroke, StrokeKind::Inside);
            painter.rect_stroke(
                Rect::from_min_max(
                    egui::pos2(c.x - r * 0.45, top + r * 0.12),
                    egui::pos2(c.x + r * 0.28, c.y - r * 0.05),
                ),
                0.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_max(
                    egui::pos2(c.x - r * 0.45, c.y + r * 0.25),
                    egui::pos2(c.x + r * 0.45, bottom - r * 0.1),
                ),
                0.0,
                stroke,
                StrokeKind::Inside,
            );
        }
        Icon::Undo | Icon::Redo => {
            let direction = if matches!(icon, Icon::Redo) {
                -1.0
            } else {
                1.0
            };
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x + direction * r, bottom - r * 0.1),
                    egui::pos2(c.x + direction * r * 0.85, top + r * 0.35),
                    egui::pos2(c.x - direction * r * 0.35, top + r * 0.2),
                    egui::pos2(c.x - direction * r, c.y),
                ],
                stroke,
            ));
            painter.line_segment(
                [
                    egui::pos2(c.x - direction * r, c.y),
                    egui::pos2(c.x - direction * r * 0.58, c.y - r * 0.42),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - direction * r, c.y),
                    egui::pos2(c.x - direction * r * 0.58, c.y + r * 0.42),
                ],
                stroke,
            );
        }
        Icon::Settings => {
            painter.circle_stroke(c, r * 0.42, stroke);
            painter.circle_stroke(c, r * 0.14, stroke);
            for direction in [
                egui::vec2(1.0, 0.0),
                egui::vec2(-1.0, 0.0),
                egui::vec2(0.0, 1.0),
                egui::vec2(0.0, -1.0),
            ] {
                painter.line_segment([c + direction * r * 0.5, c + direction * r], stroke);
            }
        }
        Icon::Network => {
            let nodes = [
                egui::pos2(c.x, top),
                egui::pos2(left, bottom),
                egui::pos2(right, bottom),
            ];
            painter.line_segment([nodes[0], nodes[1]], stroke);
            painter.line_segment([nodes[0], nodes[2]], stroke);
            painter.line_segment([nodes[1], nodes[2]], stroke);
            for node in nodes {
                painter.circle_filled(node, r * 0.16, color);
            }
        }
        Icon::Logs => {
            for y in [-0.55_f32, 0.0, 0.55] {
                let y = c.y + y * r;
                painter.circle_filled(egui::pos2(left, y), r * 0.09, color);
                painter.line_segment(
                    [egui::pos2(left + r * 0.3, y), egui::pos2(right, y)],
                    stroke,
                );
            }
        }
        Icon::Gauge => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left, bottom),
                    egui::pos2(left + r * 0.15, c.y),
                    egui::pos2(c.x - r * 0.15, c.y + r * 0.2),
                    egui::pos2(c.x + r * 0.12, top + r * 0.25),
                    egui::pos2(right, top),
                ],
                stroke,
            ));
        }
    }
}
