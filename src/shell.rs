use crate::morphs::{
    MorphGlbPreviewMesh, MorphGlbSourceSummary, MorphSourceManifest, build_source_manifest_json,
    default_rigid_accessory_asset,
};
use cubacadabra_client::native::Renderer as GameRenderer;
use cubacadabra_morphs::{MorphAssetId, MorphAssetKind, MorphCatalog, parse_catalog};
#[cfg(target_os = "macos")]
use egui::FontTweak;
use egui::{
    Align, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Frame, Layout, Margin,
    Rect, RichText, Sense, Stroke, StrokeKind, TextStyle, Vec2,
};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor, wgpu};
use egui_winit::State as EguiState;
#[cfg(target_os = "macos")]
use std::collections::HashMap;
use std::{fs, sync::Arc, time::Duration};
use winit::{event::WindowEvent, window::Window};

#[cfg(test)]
mod tests;

struct UiMetrics {
    top_bar: f32,
    status_bar: f32,
    editor_header: f32,
    control: f32,
    inset: f32,
    icon: f32,
    row: f32,
    radius: f32,
}

struct TypographyMetrics {
    primary: f32,
    secondary: f32,
    meta: f32,
}

const UI: UiMetrics = UiMetrics {
    top_bar: 30.0,
    status_bar: 20.0,
    editor_header: 24.0,
    control: 20.0,
    inset: 6.0,
    icon: 13.0,
    row: 22.0,
    radius: 1.0,
};
const TYPE: TypographyMetrics = TypographyMetrics {
    primary: 13.0,
    secondary: 12.0,
    meta: 11.0,
};
const MEDIUM_FONT_FAMILY: &str = "studio-system-ui-medium";
const SEMIBOLD_FONT_FAMILY: &str = "studio-system-ui-semibold";
const SYSTEM_UI_REGULAR: &str = "studio-system-ui-regular";
const SYSTEM_UI_MEDIUM: &str = "studio-system-ui-medium-face";
const SYSTEM_UI_SEMIBOLD: &str = "studio-system-ui-semibold-face";
const REGULAR_FONT_WEIGHT: f32 = 400.0;
const MEDIUM_FONT_WEIGHT: f32 = 510.0;
const SEMIBOLD_FONT_WEIGHT: f32 = 590.0;
#[cfg(target_os = "macos")]
const UI_OPTICAL_SIZE: f32 = 13.0;

#[cfg(target_os = "macos")]
const REGULAR_FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/Library/Fonts/SF-Pro-Text-Regular.otf",
];
#[cfg(target_os = "macos")]
const MEDIUM_FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/Library/Fonts/SF-Pro-Text-Medium.otf",
];
#[cfg(target_os = "macos")]
const SEMIBOLD_FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/Library/Fonts/SF-Pro-Text-Semibold.otf",
];

#[cfg(target_os = "windows")]
const REGULAR_FONT_PATHS: &[&str] = &["C:/Windows/Fonts/segoeui.ttf"];
#[cfg(target_os = "windows")]
const MEDIUM_FONT_PATHS: &[&str] = &[
    "C:/Windows/Fonts/seguisb.ttf",
    "C:/Windows/Fonts/segoeuisl.ttf",
];
#[cfg(target_os = "windows")]
const SEMIBOLD_FONT_PATHS: &[&str] = &["C:/Windows/Fonts/seguisb.ttf"];

#[cfg(target_os = "linux")]
const REGULAR_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];
#[cfg(target_os = "linux")]
const MEDIUM_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/noto/NotoSans-Medium.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
];
#[cfg(target_os = "linux")]
const SEMIBOLD_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
];

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const REGULAR_FONT_PATHS: &[&str] = &[];
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const MEDIUM_FONT_PATHS: &[&str] = &[];
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const SEMIBOLD_FONT_PATHS: &[&str] = &[];
const TOP_BAR_HEIGHT: f32 = UI.top_bar;
const STATUS_BAR_HEIGHT: f32 = UI.status_bar;
const EDITOR_HEADER_HEIGHT: f32 = UI.editor_header;
const CONTROL_HEIGHT: f32 = UI.control;
const LABEL_PADDING: f32 = UI.inset;
#[derive(Clone, Copy)]
struct Palette {
    panel: Color32,
    panel_raised: Color32,
    panel_header: Color32,
    surface: Color32,
    surface_deep: Color32,
    field: Color32,
    border: Color32,
    border_strong: Color32,
    text: Color32,
    muted: Color32,
    secondary_text: Color32,
    faint: Color32,
    accent: Color32,
    selection: Color32,
    asset_selection: Color32,
    asset_selection_stroke: Color32,
    live: Color32,
    axis_x: Color32,
    axis_y: Color32,
    axis_z: Color32,
}

const DARK_PALETTE: Palette = Palette {
    panel: Color32::from_rgb(30, 30, 30),
    panel_raised: Color32::from_rgb(35, 35, 35),
    panel_header: Color32::from_rgb(39, 39, 39),
    surface: Color32::from_rgb(27, 27, 27),
    surface_deep: Color32::from_rgb(23, 23, 23),
    field: Color32::from_rgb(47, 47, 47),
    border: Color32::from_rgb(48, 48, 48),
    border_strong: Color32::from_rgb(66, 66, 66),
    text: Color32::from_rgb(226, 226, 226),
    muted: Color32::from_rgb(170, 170, 170),
    secondary_text: Color32::from_rgb(200, 200, 200),
    faint: Color32::from_rgb(124, 124, 124),
    accent: Color32::from_rgb(122, 157, 193),
    selection: Color32::from_rgb(63, 78, 94),
    asset_selection: Color32::from_rgb(43, 48, 54),
    asset_selection_stroke: Color32::from_rgb(78, 96, 114),
    live: Color32::from_rgb(120, 166, 137),
    axis_x: Color32::from_rgb(218, 105, 105),
    axis_y: Color32::from_rgb(112, 193, 126),
    axis_z: Color32::from_rgb(103, 151, 218),
};

const LIGHT_PALETTE: Palette = Palette {
    panel: Color32::from_rgb(242, 242, 242),
    panel_raised: Color32::from_rgb(232, 232, 232),
    panel_header: Color32::from_rgb(224, 224, 224),
    surface: Color32::from_rgb(250, 250, 250),
    surface_deep: Color32::from_rgb(255, 255, 255),
    field: Color32::from_rgb(255, 255, 255),
    border: Color32::from_rgb(198, 198, 198),
    border_strong: Color32::from_rgb(166, 166, 166),
    text: Color32::from_rgb(35, 35, 35),
    muted: Color32::from_rgb(92, 92, 92),
    secondary_text: Color32::from_rgb(65, 65, 65),
    faint: Color32::from_rgb(128, 128, 128),
    accent: Color32::from_rgb(53, 103, 150),
    selection: Color32::from_rgb(204, 218, 232),
    asset_selection: Color32::from_rgb(222, 231, 240),
    asset_selection_stroke: Color32::from_rgb(118, 145, 171),
    live: Color32::from_rgb(54, 128, 79),
    axis_x: Color32::from_rgb(180, 55, 55),
    axis_y: Color32::from_rgb(39, 128, 62),
    axis_z: Color32::from_rgb(42, 98, 173),
};
const LOGO_BYTES: &[u8] = include_bytes!("../assets/logo.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Icon {
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
    #[cfg(not(target_os = "macos"))]
    Save,
    Open,
    #[cfg(not(target_os = "macos"))]
    Undo,
    #[cfg(not(target_os = "macos"))]
    Redo,
    #[cfg(not(target_os = "macos"))]
    Settings,
    Network,
    Logs,
    Gauge,
}

#[cfg(target_os = "macos")]
const MACOS_SYSTEM_SYMBOLS: &[(Icon, &str)] = &[
    (Icon::World, "globe"),
    (Icon::Assets, "square.grid.2x2"),
    (Icon::Material, "circle.lefthalf.filled"),
    (Icon::Test, "play.rectangle"),
    (Icon::Folder, "folder.fill"),
    (Icon::Object, "cube"),
    (Icon::Image, "photo"),
    (Icon::Character, "person"),
    (Icon::Search, "magnifyingglass"),
    (Icon::Filter, "line.3.horizontal.decrease"),
    (Icon::Grid, "square.grid.2x2"),
    (Icon::Camera, "camera"),
    (Icon::Sliders, "slider.horizontal.3"),
    (Icon::More, "ellipsis"),
    (Icon::Plus, "plus"),
    (Icon::Play, "play.fill"),
    (Icon::Stop, "stop.fill"),
    (Icon::Check, "checkmark"),
    (Icon::ChevronDown, "chevron.down"),
    (Icon::ChevronRight, "chevron.right"),
    (Icon::Eye, "eye"),
    (Icon::Lock, "lock"),
    (Icon::Open, "square.and.arrow.down"),
    (Icon::Network, "network"),
    (Icon::Logs, "list.bullet.rectangle"),
    (Icon::Gauge, "speedometer"),
];

#[cfg(target_os = "macos")]
const SYSTEM_ICON_ATLAS_ID: &str = "studio-macos-system-icons";

#[cfg(target_os = "macos")]
type SystemIconAtlas = Arc<HashMap<Icon, egui::TextureHandle>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Workspace {
    #[default]
    World,
    Assets,
    Materials,
    Morphs,
    Test,
}

impl Workspace {
    const ALL: [Self; 5] = [
        Self::World,
        Self::Assets,
        Self::Materials,
        Self::Morphs,
        Self::Test,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::World => "World",
            Self::Assets => "Assets",
            Self::Materials => "Materials",
            Self::Morphs => "Morphs",
            Self::Test => "Test",
        }
    }

    fn command(self) -> StudioCommand {
        match self {
            Self::World => StudioCommand::ShowWorld,
            Self::Assets => StudioCommand::ShowAssets,
            Self::Materials => StudioCommand::ShowMaterials,
            Self::Morphs => StudioCommand::ShowMorphs,
            Self::Test => StudioCommand::ShowTest,
        }
    }
}

const MORPH_LIBRARY_KINDS: [MorphAssetKind; 17] = [
    MorphAssetKind::Base,
    MorphAssetKind::Face,
    MorphAssetKind::Hair,
    MorphAssetKind::Outfit,
    MorphAssetKind::Top,
    MorphAssetKind::Outerwear,
    MorphAssetKind::Bottom,
    MorphAssetKind::OnePiece,
    MorphAssetKind::Footwear,
    MorphAssetKind::Headwear,
    MorphAssetKind::Facewear,
    MorphAssetKind::Accessory,
    MorphAssetKind::Tail,
    MorphAssetKind::Wings,
    MorphAssetKind::Horns,
    MorphAssetKind::Ears,
    MorphAssetKind::HeldItem,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StudioCommand {
    OpenProject,
    Save,
    RevealProject,
    Preferences,
    MaximizeViewport,
    ResetLayout,
    ShowWorld,
    ShowAssets,
    ShowMaterials,
    ShowMorphs,
    ShowTest,
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
    morph_query: String,
    morph_catalog: MorphCatalog,
    selected_morph: MorphAssetId,
    morph_import_requested: bool,
    morph_sidecar_import_requested: bool,
    morph_preview_path: Option<String>,
    morph_preview: Option<MorphGlbPreviewMesh>,
    morph_lod_previews: [Option<MorphGlbPreviewMesh>; 3],
    morph_preview_lod: Option<usize>,
    morph_source_summary: Option<MorphGlbSourceSummary>,
    morph_draft_asset: Option<cubacadabra_morphs::MorphAssetDefinition>,
    morph_attachment_joint: String,
    morph_lod_nodes: [String; 3],
    morph_draft_status: Option<(bool, String)>,
    morph_sidecar_export_requested: bool,
    morph_publish_requested: bool,
    morph_thumbnail_requested: bool,
    morph_wireframe: bool,
    morph_import_error: Option<String>,
    logo_texture: egui::TextureHandle,
    position: [f32; 3],
    rotation: f32,
    scale: f32,
    roughness: f32,
    pending_textures_delta: egui::TexturesDelta,
}

impl StudioShell {
    pub(crate) fn new(window: &Window, game_renderer: &GameRenderer) -> Self {
        let context = egui::Context::default();
        configure_context(&context);
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
        let logo_texture = load_logo_texture(&context);
        #[cfg(target_os = "macos")]
        install_system_icon_textures(&context);
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
            // Studio historically launched directly into its live runtime.
            // Keep that behavior now that the shell has a Play/Stop toggle so
            // keyboard and engine-owned pointer controls work immediately.
            playing: true,
            notice: "Ready".to_owned(),
            search_query: String::new(),
            morph_query: String::new(),
            morph_catalog: parse_catalog(include_str!(
                "../../rust/assets/characters/morph_catalog.json"
            ))
            .expect("bundled morph catalog must be valid"),
            selected_morph: MorphAssetId::parse("cuba:base/person.v1")
                .expect("built-in morph ID must be valid"),
            morph_import_requested: false,
            morph_sidecar_import_requested: false,
            morph_preview_path: None,
            morph_preview: None,
            morph_lod_previews: [None, None, None],
            morph_preview_lod: None,
            morph_source_summary: None,
            morph_draft_asset: None,
            morph_attachment_joint: "head".to_owned(),
            morph_lod_nodes: [String::new(), String::new(), String::new()],
            morph_draft_status: None,
            morph_sidecar_export_requested: false,
            morph_publish_requested: false,
            morph_thumbnail_requested: false,
            morph_wireframe: true,
            morph_import_error: None,
            logo_texture,
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

    pub(crate) fn take_morph_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_import_requested)
    }

    pub(crate) fn take_morph_sidecar_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_sidecar_import_requested)
    }

    pub(crate) fn take_morph_sidecar_export_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_sidecar_export_requested)
    }

    pub(crate) fn take_morph_publish_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_publish_requested)
    }

    pub(crate) fn take_morph_thumbnail_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_thumbnail_requested)
    }

    pub(crate) fn morph_sidecar_payload(&self) -> Result<(String, String), String> {
        let path = self
            .morph_preview_path
            .as_deref()
            .ok_or_else(|| "Import a GLB before exporting its sidecar.".to_owned())?;
        let preview = self
            .morph_preview
            .as_ref()
            .ok_or_else(|| "The imported GLB has no preview mesh to save.".to_owned())?;
        let asset = self
            .morph_draft_asset
            .as_ref()
            .or_else(|| self.morph_catalog.asset(&self.selected_morph))
            .ok_or_else(|| "Select a catalog asset before exporting its sidecar.".to_owned())?;
        let geometry_file = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "The imported GLB needs a safe filename for its sidecar.".to_owned())?
            .to_owned();
        let lod_nodes = [
            self.morph_lod_nodes[0].trim(),
            self.morph_lod_nodes[1].trim(),
            self.morph_lod_nodes[2].trim(),
        ];
        let fallback_triangle_count = u32::try_from(preview.indices.len() / 3)
            .map_err(|_| "The preview mesh triangle count is too large.".to_owned())?;
        let summary = self
            .morph_source_summary
            .as_ref()
            .ok_or_else(|| "The imported GLB has no source summary to save.".to_owned())?;
        let triangle_counts = lod_nodes.map(|node| {
            summary
                .node_triangle_counts
                .get(node)
                .copied()
                .unwrap_or(fallback_triangle_count)
                .max(1)
        });
        let json = build_source_manifest_json(
            asset,
            geometry_file,
            self.morph_attachment_joint.trim(),
            lod_nodes,
            triangle_counts,
        )
        .map_err(|diagnostics| {
            diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        let suggested_name = format!(
            "{}.morph.json",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, json))
    }

    pub(crate) fn morph_pack_inputs(&self) -> Result<(String, String, String), String> {
        let (sidecar_name, manifest_json) = self.morph_sidecar_payload()?;
        let glb_path = self
            .morph_preview_path
            .as_ref()
            .ok_or_else(|| "Import a GLB before publishing its pack.".to_owned())?
            .clone();
        let suggested_name = format!(
            "{}.morphpack",
            std::path::Path::new(&sidecar_name)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((glb_path, suggested_name, manifest_json))
    }

    pub(crate) fn set_morph_sidecar_export_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    true,
                    "Sidecar saved. The GLB and .morph.json can now travel together.".to_owned(),
                ));
                self.notice = "Morph sidecar saved".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_publish_result(
        &mut self,
        result: Result<(String, usize), String>,
    ) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status = Some((
                    true,
                    format!("Published {asset_id} ({byte_len} bytes)."),
                ));
                self.notice = "Morph pack published".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn morph_thumbnail_payload(&self) -> Result<(String, MorphGlbPreviewMesh), String> {
        let path = self
            .morph_preview_path
            .as_ref()
            .ok_or_else(|| "Import a GLB before generating a thumbnail.".to_owned())?;
        let preview = self
            .morph_preview
            .as_ref()
            .ok_or_else(|| "The imported GLB has no preview mesh.".to_owned())?
            .clone();
        let suggested_name = format!(
            "{}.png",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, preview))
    }

    pub(crate) fn set_morph_thumbnail_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    true,
                    "Thumbnail generated from the current shaded preview.".to_owned(),
                ));
                self.notice = "Morph thumbnail generated".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_preview(
        &mut self,
        path: String,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        let triangle_count = u32::try_from(preview.indices.len() / 3)
            .unwrap_or(u32::MAX)
            .max(1);
        let draft_asset = Some(default_rigid_accessory_asset(&path, triangle_count));
        self.morph_preview_path = Some(path);
        self.morph_preview = Some(preview);
        self.morph_lod_previews = [None, None, None];
        self.morph_preview_lod = None;
        self.morph_attachment_joint = "head".to_owned();
        self.morph_lod_nodes = ["near", "mid", "far"].map(|level| {
            summary
                .lod_candidates
                .get(level)
                .filter(|candidates| candidates.len() == 1)
                .and_then(|candidates| candidates.first())
                .cloned()
                .unwrap_or_default()
        });
        self.morph_source_summary = Some(summary);
        self.morph_draft_asset = draft_asset;
        self.morph_draft_status = None;
        self.morph_import_error = None;
        self.notice = "GLB preview imported".to_owned();
    }

    pub(crate) fn set_morph_lod_preview(
        &mut self,
        level: usize,
        preview: MorphGlbPreviewMesh,
    ) {
        if level >= self.morph_lod_previews.len() {
            return;
        }
        self.morph_lod_previews[level] = Some(preview.clone());
        if self.morph_preview_lod == Some(level) {
            self.morph_preview = Some(preview);
        }
    }

    pub(crate) fn select_morph_preview_lod(&mut self, level: Option<usize>) {
        let Some(level) = level else {
            self.morph_preview_lod = None;
            return;
        };
        let Some(preview) = self.morph_lod_previews.get(level).and_then(Option::as_ref) else {
            return;
        };
        self.morph_preview_lod = Some(level);
        self.morph_preview = Some(preview.clone());
    }

    pub(crate) fn set_morph_sidecar_preview(
        &mut self,
        glb_path: String,
        manifest: MorphSourceManifest,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        let attachment_joint = manifest.attachment.joint.clone();
        let lod_nodes = ["near", "mid", "far"].map(|level| {
            manifest
                .geometry
                .lod_nodes
                .get(level)
                .cloned()
                .unwrap_or_default()
        });
        self.set_morph_preview(glb_path, preview, summary);
        self.morph_draft_asset = Some(manifest.asset);
        self.morph_attachment_joint = attachment_joint;
        self.morph_lod_nodes = lod_nodes;
        self.morph_draft_status = Some((
            true,
            "Sidecar reimported and GLB contract validated.".to_owned(),
        ));
        self.notice = "Morph sidecar reimported".to_owned();
    }

    pub(crate) fn set_morph_import_error(&mut self, message: String) {
        self.morph_import_error = Some(message.clone());
        self.notice = message;
    }

    fn validate_morph_draft(&mut self) {
        let Some(summary) = &self.morph_source_summary else {
            self.morph_draft_status = Some((
                false,
                "Import a GLB before validating its draft.".to_owned(),
            ));
            return;
        };
        let mut issues = Vec::new();
        let joint = self.morph_attachment_joint.trim();
        if joint.is_empty() || joint.contains('/') || joint.contains('\\') {
            issues.push("Attachment joint must be a non-empty joint name.".to_owned());
        }
        for (index, level) in ["Near", "Mid", "Far"].into_iter().enumerate() {
            let node = self.morph_lod_nodes[index].trim();
            if node.is_empty() {
                issues.push(format!("{level} LOD needs a node mapping."));
            } else if !summary.node_names.iter().any(|candidate| candidate == node) {
                issues.push(format!(
                    "{level} LOD node {node:?} is not present in the GLB."
                ));
            }
        }
        for first in 0..self.morph_lod_nodes.len() {
            for second in (first + 1)..self.morph_lod_nodes.len() {
                let first_node = self.morph_lod_nodes[first].trim();
                if !first_node.is_empty() && first_node == self.morph_lod_nodes[second].trim() {
                    issues.push("Near, Mid, and Far must use distinct nodes.".to_owned());
                }
            }
        }
        if issues.is_empty() {
            self.morph_draft_status = Some((
                true,
                "Draft mapping is ready for sidecar export.".to_owned(),
            ));
            self.notice = "Draft mapping validated".to_owned();
        } else {
            self.morph_draft_status = Some((false, issues.join(" ")));
            self.notice = "Draft mapping needs attention".to_owned();
        }
    }

    pub(crate) fn execute_command(&mut self, command: StudioCommand) {
        match command {
            StudioCommand::OpenProject => {
                self.notice = "Open Project is a layout preview".to_owned();
            }
            StudioCommand::Save => {
                self.notice = "Nothing to save yet".to_owned();
            }
            StudioCommand::RevealProject => {
                self.notice = "Reveal Project is not connected yet".to_owned();
            }
            StudioCommand::Preferences => {
                self.notice = "Preferences are coming later".to_owned();
            }
            StudioCommand::MaximizeViewport => {
                self.notice = "Viewport maximize is coming later".to_owned();
            }
            StudioCommand::ResetLayout => {
                self.notice = "Layout reset".to_owned();
            }
            StudioCommand::ShowWorld => self.select_workspace(Workspace::World),
            StudioCommand::ShowAssets => self.select_workspace(Workspace::Assets),
            StudioCommand::ShowMaterials => self.select_workspace(Workspace::Materials),
            StudioCommand::ShowMorphs => self.select_workspace(Workspace::Morphs),
            StudioCommand::ShowTest => self.select_workspace(Workspace::Test),
        }
    }

    fn select_workspace(&mut self, workspace: Workspace) {
        self.workspace = workspace;
        self.notice = format!("{} workspace", workspace.label());
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
            Workspace::Morphs => self.show_morphs(ui),
            Workspace::Test => self.show_test(ui),
        }
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    fn show_top_bar(&mut self, root: &mut egui::Ui, project_name: &str) {
        let colors = palette(root);
        egui::Panel::top("studio_top_bar")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(editor_frame(colors.surface).inner_margin(Margin::symmetric(6, 0)))
            .show(root, |ui| {
                egui::MenuBar::new().style(menu_bar_style).ui(ui, |ui| {
                    ui.add(
                        egui::Image::from_texture(&self.logo_texture)
                            .fit_to_exact_size(egui::vec2(20.0, 20.0))
                            .sense(Sense::hover()),
                    );

                    #[cfg(not(target_os = "macos"))]
                    {
                        ui.menu_button(RichText::new("File").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            if menu_entry(ui, Icon::Open, "Open Project…", "Ctrl+O", true).clicked()
                            {
                                self.execute_command(StudioCommand::OpenProject);
                                ui.close();
                            }
                            if menu_entry(ui, Icon::Save, "Save", "Ctrl+S", true).clicked() {
                                self.execute_command(StudioCommand::Save);
                                ui.close();
                            }
                            ui.separator();
                            if menu_entry(ui, Icon::Folder, "Reveal Project", "", true).clicked() {
                                self.execute_command(StudioCommand::RevealProject);
                                ui.close();
                            }
                        });
                        ui.menu_button(RichText::new("Edit").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            menu_entry(ui, Icon::Undo, "Undo", "Ctrl+Z", false);
                            menu_entry(ui, Icon::Redo, "Redo", "Ctrl+Shift+Z", false);
                            ui.separator();
                            if menu_entry(ui, Icon::Settings, "Preferences…", "Ctrl+,", true)
                                .clicked()
                            {
                                self.execute_command(StudioCommand::Preferences);
                                ui.close();
                            }
                        });
                        ui.menu_button(RichText::new("Window").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            if menu_entry(ui, Icon::Grid, "Maximize Viewport", "Space", true)
                                .clicked()
                            {
                                self.execute_command(StudioCommand::MaximizeViewport);
                                ui.close();
                            }
                            if menu_entry(ui, Icon::Sliders, "Reset Layout", "", true).clicked() {
                                self.execute_command(StudioCommand::ResetLayout);
                                ui.close();
                            }
                        });
                    }

                    ui.add_space(8.0);
                    for workspace in Workspace::ALL {
                        if workspace_tab(ui, workspace.label(), self.workspace == workspace)
                            .clicked()
                        {
                            self.execute_command(workspace.command());
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let live =
                            ui.allocate_response(egui::vec2(40.0, CONTROL_HEIGHT), Sense::hover());
                        paint_status_label(ui, live.rect, colors.live, "Live");
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
                            ui.label(
                                RichText::new(project_name)
                                    .size(TYPE.secondary)
                                    .color(colors.secondary_text),
                            );
                        }
                    });
                });
            });
    }

    fn show_status_bar(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::bottom("studio_status_bar")
            .exact_size(STATUS_BAR_HEIGHT)
            .frame(editor_frame(colors.panel_raised).inner_margin(Margin::symmetric(8, 0)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(STATUS_BAR_HEIGHT);
                    ui.spacing_mut().interact_size.y = 16.0;
                    inline_icon(ui, Icon::Check, colors.muted);
                    ui.label(
                        RichText::new(&self.notice)
                            .size(TYPE.meta)
                            .color(colors.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("Layout preview")
                                .size(TYPE.meta)
                                .color(colors.faint),
                        );
                        vertical_separator(ui, 12.0);
                        ui.label(RichText::new("Metal").size(TYPE.meta).color(colors.faint));
                    });
                });
            });
    }

    fn show_world(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::bottom("world_assets")
            .resizable(true)
            .default_size(112.0)
            .size_range(100.0..=280.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| self.asset_shelf(ui));

        egui::Panel::left("world_scene")
            .resizable(true)
            .default_size(208.0)
            .size_range(180.0..=340.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| self.scene_tree(ui));

        egui::Panel::right("world_inspector")
            .resizable(true)
            .default_size(256.0)
            .size_range(224.0..=340.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| self.inspector(ui));

        self.viewport_panel(root, "Perspective", "Viewport");
    }

    fn show_assets(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::left("asset_categories")
            .resizable(true)
            .default_size(220.0)
            .frame(editor_frame(colors.panel))
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
            .frame(editor_frame(colors.panel_raised))
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
            .frame(editor_frame(colors.surface))
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
        let colors = palette(root);
        egui::Panel::left("material_list")
            .resizable(true)
            .default_size(220.0)
            .frame(editor_frame(colors.panel))
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
            .frame(editor_frame(colors.panel_raised))
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
                        ui.label(
                            RichText::new("Roughness")
                                .size(TYPE.secondary)
                                .color(colors.secondary_text),
                        );
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

    fn show_morphs(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::left("morph_library")
            .resizable(true)
            .default_size(236.0)
            .size_range(200.0..=340.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                panel_header(ui, Icon::Character, "Morph library", |ui| {
                    if icon_button(ui, Icon::Plus, "Create morph", false).clicked() {
                        self.notice = "Create Morph is not connected yet".to_owned();
                    }
                });
                content_frame().show(ui, |ui| {
                    search_field(ui, &mut self.morph_query, ui.available_width());
                    ui.add_space(6.0);
                    let query = self.morph_query.trim().to_ascii_lowercase();
                    for kind in MORPH_LIBRARY_KINDS {
                        let assets = self
                            .morph_catalog
                            .assets
                            .iter()
                            .filter(|asset| asset.kind == kind)
                            .filter(|asset| {
                                query.is_empty()
                                    || asset.display_name.to_ascii_lowercase().contains(&query)
                                    || asset.id.as_str().contains(&query)
                            })
                            .map(|asset| (asset.id.clone(), asset.display_name.clone()))
                            .collect::<Vec<_>>();
                        if assets.is_empty() {
                            continue;
                        }
                        egui::CollapsingHeader::new(
                            RichText::new(morph_kind_label(kind))
                                .font(semibold_font(TYPE.secondary))
                                .color(colors.secondary_text),
                        )
                        .default_open(matches!(kind, MorphAssetKind::Base | MorphAssetKind::Hair))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            for (id, name) in &assets {
                                if navigation_row(
                                    ui,
                                    Icon::Character,
                                    name,
                                    self.selected_morph == *id,
                                )
                                .clicked()
                                {
                                    self.selected_morph = id.clone();
                                    self.notice = format!("Selected {name}");
                                }
                            }
                        });
                    }
                    if self.morph_catalog.assets.iter().all(|asset| {
                        !query.is_empty()
                            && !asset.display_name.to_ascii_lowercase().contains(&query)
                            && !asset.id.as_str().contains(&query)
                    }) {
                        ui.label(
                            RichText::new("No morphs match this search.")
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                    }
                });
            });

        egui::Panel::right("morph_inspector")
            .resizable(true)
            .default_size(280.0)
            .size_range(236.0..=360.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                panel_header(ui, Icon::Sliders, "Morph inspector", |ui| {
                    if icon_button(ui, Icon::Open, "Import GLB", false).clicked() {
                        self.morph_import_requested = true;
                        self.morph_import_error = None;
                    }
                    if icon_button(ui, Icon::Folder, "Open .morph.json", false).clicked() {
                        self.morph_sidecar_import_requested = true;
                        self.morph_import_error = None;
                    }
                    icon_button(ui, Icon::More, "Morph options", false);
                });
                content_frame().show(ui, |ui| {
                    if let Some(path) = &self.morph_preview_path {
                        property_section(ui, "Imported source", |ui| {
                            let filename = std::path::Path::new(path)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or(path);
                            property_row(ui, "File", filename);
                            if let Some(preview) = &self.morph_preview {
                                property_row(ui, "Vertices", &preview.vertices.len().to_string());
                                property_row(ui, "Indices", &preview.indices.len().to_string());
                                property_row(
                                    ui,
                                    "Triangles",
                                    &(preview.indices.len() / 3).to_string(),
                                );
                            }
                            if let Some(asset) = &self.morph_draft_asset {
                                property_row(ui, "Draft", &asset.display_name);
                                property_row(ui, "Asset ID", asset.id.as_str());
                            }
                        });
                        if let Some(summary) = self.morph_source_summary.clone() {
                            property_section(ui, "Source contract", |ui| {
                                property_row(ui, "Nodes", &summary.node_names.len().to_string());
                                property_row(ui, "Meshes", &summary.mesh_names.len().to_string());
                                property_row(
                                    ui,
                                    "Materials",
                                    &summary.material_names.len().to_string(),
                                );
                                property_row(
                                    ui,
                                    "Source triangles",
                                    &summary.triangle_count.to_string(),
                                );
                                for level in ["near", "mid", "far"] {
                                    let status = source_lod_status(&summary, level);
                                    property_row(ui, &format!("{} LOD", title_case(level)), &status);
                                }
                                if ["near", "mid", "far"]
                                    .into_iter()
                                    .any(|level| summary.lod_candidates[level].is_empty())
                                {
                                    ui.label(
                                        RichText::new(
                                            "Preview only: map distinct Near / Mid / Far nodes before publishing.",
                                        )
                                        .size(TYPE.meta)
                                        .color(colors.axis_x),
                                    );
                                }
                            });
                            property_section(ui, "Draft mapping", |ui| {
                                ui.label(
                                    RichText::new("Attachment joint")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.morph_attachment_joint)
                                        .hint_text("head")
                                        .desired_width(ui.available_width()),
                                );
                                for (index, level) in ["Near", "Mid", "Far"].into_iter().enumerate()
                                {
                                    ui.label(
                                        RichText::new(format!("{level} LOD node"))
                                            .size(TYPE.meta)
                                            .color(colors.secondary_text),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.morph_lod_nodes[index])
                                            .hint_text("GLB node name")
                                            .desired_width(ui.available_width()),
                                    );
                                }
                                if ui.button("Validate mapping").clicked() {
                                    self.validate_morph_draft();
                                }
                                if ui.button("Export .morph.json").clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_sidecar_export_requested = true;
                                    }
                                }
                                if ui.button("Publish .morphpack").clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_publish_requested = true;
                                    }
                                }
                                if ui.button("Generate thumbnail PNG").clicked() {
                                    self.morph_thumbnail_requested = true;
                                }
                                if let Some((valid, status)) = &self.morph_draft_status {
                                    ui.label(
                                        RichText::new(status)
                                            .size(TYPE.meta)
                                            .color(if *valid { colors.live } else { colors.axis_x }),
                                    );
                                }
                            });
                        }
                    }
                    if let Some(error) = &self.morph_import_error {
                        ui.label(RichText::new(error).size(TYPE.meta).color(colors.axis_x));
                        ui.add_space(4.0);
                    }
                    if let Some(asset) = self.morph_catalog.asset(&self.selected_morph) {
                        selected_object_header(
                            ui,
                            &asset.display_name,
                            morph_kind_label(asset.kind),
                        );
                        ui.add_space(4.0);
                        property_section(ui, "Identity", |ui| {
                            let id = asset.id.to_string();
                            property_row(ui, "ID", &id);
                            property_row(ui, "Kind", morph_kind_label(asset.kind));
                            if let Some(rig) = &asset.rig_profile {
                                let rig = rig.to_string();
                                property_row(ui, "Rig", &rig);
                            }
                        });
                        property_section(ui, "Compatibility", |ui| {
                            let fits = asset
                                .fit_profiles
                                .iter()
                                .map(MorphAssetId::as_str)
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(ui, "Fits", &fits);
                            let bases = asset
                                .supported_bases
                                .iter()
                                .map(MorphAssetId::as_str)
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(
                                ui,
                                "Bases",
                                if bases.is_empty() { "Any" } else { &bases },
                            );
                        });
                        property_section(ui, "Runtime", |ui| {
                            let capabilities = asset
                                .required_capabilities
                                .iter()
                                .map(|capability| capability.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(ui, "Needs", &capabilities);
                            property_row(ui, "LOD", "Near / Mid / Far");
                        });
                    } else {
                        ui.label(
                            RichText::new("Select a morph to inspect its contract.")
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                    }
                });
            });

        self.morph_preview_panel(root);
    }

    fn morph_preview_panel(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Camera, colors.muted);
                    ui.label(
                        RichText::new("Morph preview")
                            .font(semibold_font(TYPE.primary))
                            .color(colors.text),
                    );
                    vertical_separator(ui, 12.0);
                    ui.label(
                        RichText::new("Imported GLB")
                            .size(TYPE.secondary)
                            .color(colors.secondary_text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, Icon::More, "Preview options", false);
                        icon_button(ui, Icon::Camera, "Camera view", false);
                        icon_button(ui, Icon::Grid, "Toggle grid", true);
                        if icon_button(
                            ui,
                            Icon::Object,
                            "Toggle wireframe",
                            self.morph_wireframe,
                        )
                        .clicked()
                        {
                            self.morph_wireframe = !self.morph_wireframe;
                        }
                    });
                });
                let mode_rect = ui
                    .allocate_exact_size(
                        egui::vec2(ui.available_width(), CONTROL_HEIGHT),
                        Sense::hover(),
                    )
                    .0;
                ui.painter()
                    .rect_filled(mode_rect, 0.0, colors.panel_header);
                let mut mode_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(mode_rect.shrink2(egui::vec2(UI.inset, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                mode_ui.set_clip_rect(ui.clip_rect().intersect(mode_rect));
                mode_ui.spacing_mut().item_spacing.x = 2.0;
                for (level, label) in [
                    (None, "Source"),
                    (Some(0), "Near"),
                    (Some(1), "Mid"),
                    (Some(2), "Far"),
                ] {
                    let enabled = level.is_none()
                        || self
                            .morph_lod_previews
                            .get(level.unwrap_or_default())
                            .is_some_and(Option::is_some);
                    if mode_ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(label).selected(self.morph_preview_lod == level),
                        )
                        .clicked()
                    {
                        self.select_morph_preview_lod(level);
                    }
                }
                let preview_rect = Rect::from_min_max(
                    egui::pos2(
                        available.min.x + 1.0,
                        available.min.y + EDITOR_HEADER_HEIGHT + CONTROL_HEIGHT + 2.0,
                    ),
                    egui::pos2(available.max.x - 1.0, available.max.y - 1.0),
                );
                ui.painter()
                    .rect_filled(preview_rect, 0.0, colors.surface_deep);
                ui.painter().rect_stroke(
                    available,
                    0.0,
                    Stroke::new(1.0, colors.border),
                    StrokeKind::Inside,
                );

                let grid_color = colors.border.linear_multiply(0.55);
                let grid_step = 32.0;
                let mut x = preview_rect.left();
                while x <= preview_rect.right() {
                    ui.painter().line_segment(
                        [
                            egui::pos2(x, preview_rect.top()),
                            egui::pos2(x, preview_rect.bottom()),
                        ],
                        Stroke::new(1.0, grid_color),
                    );
                    x += grid_step;
                }
                let mut y = preview_rect.top();
                while y <= preview_rect.bottom() {
                    ui.painter().line_segment(
                        [
                            egui::pos2(preview_rect.left(), y),
                            egui::pos2(preview_rect.right(), y),
                        ],
                        Stroke::new(1.0, grid_color),
                    );
                    y += grid_step;
                }

                if let Some(preview) = &self.morph_preview {
                    paint_morph_surface(ui, preview_rect, preview, colors.accent);
                    if self.morph_wireframe {
                        paint_morph_wireframe(ui, preview_rect, preview, colors.accent);
                    }
                    ui.painter().text(
                        preview_rect.left_top() + egui::vec2(10.0, 10.0),
                        Align2::LEFT_TOP,
                        format!("{} · {} vertices", preview.name, preview.vertices.len()),
                        FontId::proportional(TYPE.meta),
                        colors.secondary_text,
                    );
                } else {
                    ui.painter().text(
                        preview_rect.center(),
                        Align2::CENTER_CENTER,
                        "Import a GLB to preview its mesh",
                        FontId::proportional(TYPE.secondary),
                        colors.muted,
                    );
                }
            });
    }

    fn show_test(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::bottom("test_tools")
            .resizable(true)
            .default_size(112.0)
            .size_range(80.0..=260.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Test, colors.muted);
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
                        inline_icon(ui, tool_icon(self.test_tool), colors.faint);
                        ui.label(
                            RichText::new(format!("{} tools will appear here.", self.test_tool))
                                .size(TYPE.secondary)
                                .color(colors.muted),
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
                        ui.painter().rect_filled(rect, 0.0, colors.surface);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Session preview",
                            FontId::proportional(TYPE.secondary),
                            colors.muted,
                        );
                    }
                    ui.painter().rect_filled(header, 0.0, colors.panel_header);
                    let icon_rect = Rect::from_center_size(
                        header.left_center() + egui::vec2(14.0, 0.0),
                        Vec2::splat(UI.icon),
                    );
                    paint_icon(ui.painter(), icon_rect, Icon::Camera, colors.faint);
                    ui.painter().text(
                        header.left_center() + egui::vec2(27.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("Player {}", index + 1),
                        medium_font(TYPE.secondary),
                        colors.text,
                    );
                    let status_center = header.right_center() - egui::vec2(13.0, 0.0);
                    ui.painter().circle_filled(
                        status_center,
                        3.0,
                        if index == 0 {
                            colors.accent
                        } else {
                            colors.faint
                        },
                    );
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(1.0, colors.border),
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
        let colors = palette(root);
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                let header = editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Camera, colors.muted);
                    ui.label(
                        RichText::new(title)
                            .font(semibold_font(TYPE.primary))
                            .color(colors.text),
                    );
                    vertical_separator(ui, 12.0);
                    ui.label(
                        RichText::new(mode)
                            .size(TYPE.secondary)
                            .color(colors.secondary_text),
                    );
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
                    Stroke::new(1.0, colors.border),
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
            ui.add_space(2.0);
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
                ui.add_space(2.0);
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
        let colors = palette(ui);
        editor_header(ui, |ui| {
            inline_icon(ui, Icon::Assets, colors.muted);
            ui.label(
                RichText::new("Assets")
                    .font(semibold_font(TYPE.primary))
                    .color(colors.text),
            );
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

fn morph_preview_projection(
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
) -> Option<(impl Fn([f32; 3]) -> egui::Pos2, f32)> {
    if mesh.vertices.is_empty() {
        return None;
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for vertex in &mesh.vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let span = (max[0] - min[0])
        .max(max[1] - min[1])
        .max(max[2] - min[2])
        .max(0.0001);
    let scale = (rect.width().min(rect.height()) * 0.78) / span;
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let project = move |vertex: [f32; 3]| {
        // A small depth offset keeps front and back surfaces legible while
        // preserving the model's useful front-facing silhouette.
        egui::pos2(
            rect.center().x + (vertex[0] - center[0]) * scale
                + (vertex[2] - center[2]) * scale * 0.12,
            rect.center().y - (vertex[1] - center[1]) * scale
                + (vertex[2] - center[2]) * scale * 0.06,
        )
    };
    Some((project, scale))
}

fn paint_morph_surface(ui: &egui::Ui, rect: Rect, mesh: &MorphGlbPreviewMesh, color: Color32) {
    if mesh.indices.len() < 3 {
        return;
    }
    let Some((project, _scale)) = morph_preview_projection(rect, mesh) else {
        return;
    };
    let light = normalize3([0.35, 0.75, 0.65]);
    let base_color = mesh.base_color.unwrap_or([
        f32::from(color.r()) / 255.0,
        f32::from(color.g()) / 255.0,
        f32::from(color.b()) / 255.0,
        1.0,
    ]);
    let mut triangles = Vec::new();
    for triangle in mesh
        .indices
        .chunks(3)
        .filter(|triangle| triangle.len() == 3)
    {
        let Some(a) = mesh.vertices.get(triangle[0] as usize).copied() else {
            continue;
        };
        let Some(b) = mesh.vertices.get(triangle[1] as usize).copied() else {
            continue;
        };
        let Some(c) = mesh.vertices.get(triangle[2] as usize).copied() else {
            continue;
        };
        let normal = cross3(sub3(b, a), sub3(c, a));
        let brightness = (dot3(normalize3(normal), light).abs() * 0.55 + 0.45).clamp(0.0, 1.0);
        triangles.push((
            (a[2] + b[2] + c[2]) / 3.0,
            [project(a), project(b), project(c)],
            brightness,
        ));
    }
    triangles.sort_by(|first, second| first.0.total_cmp(&second.0));
    for (_, points, brightness) in triangles {
        let fill = Color32::from_rgba_unmultiplied(
            (base_color[0] * brightness * 255.0) as u8,
            (base_color[1] * brightness * 255.0) as u8,
            (base_color[2] * brightness * 255.0) as u8,
            (base_color[3] * 185.0) as u8,
        );
        ui.painter()
            .add(egui::Shape::convex_polygon(points.to_vec(), fill, Stroke::NONE));
    }
}

fn paint_morph_wireframe(ui: &egui::Ui, rect: Rect, mesh: &MorphGlbPreviewMesh, color: Color32) {
    if mesh.vertices.is_empty() || mesh.indices.len() < 3 {
        return;
    }
    let Some((project, _scale)) = morph_preview_projection(rect, mesh) else {
        return;
    };
    let stroke = Stroke::new(1.0, color);
    for triangle in mesh
        .indices
        .chunks(3)
        .filter(|triangle| triangle.len() == 3)
    {
        let Some(a) = mesh.vertices.get(triangle[0] as usize).copied() else {
            continue;
        };
        let Some(b) = mesh.vertices.get(triangle[1] as usize).copied() else {
            continue;
        };
        let Some(c) = mesh.vertices.get(triangle[2] as usize).copied() else {
            continue;
        };
        let points = [project(a), project(b), project(c)];
        ui.painter().line_segment([points[0], points[1]], stroke);
        ui.painter().line_segment([points[1], points[2]], stroke);
        ui.painter().line_segment([points[2], points[0]], stroke);
    }
}

fn sub3(first: [f32; 3], second: [f32; 3]) -> [f32; 3] {
    [
        first[0] - second[0],
        first[1] - second[1],
        first[2] - second[2],
    ]
}

fn cross3(first: [f32; 3], second: [f32; 3]) -> [f32; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn dot3(first: [f32; 3], second: [f32; 3]) -> f32 {
    first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let length = dot3(value, value).sqrt();
    if length > f32::EPSILON {
        [value[0] / length, value[1] / length, value[2] / length]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn configure_context(context: &egui::Context) {
    configure_fonts(context);
    configure_style(context);
}

fn configure_fonts(context: &egui::Context) {
    // egui rasterizes fonts itself, so use each desktop OS's installed UI face
    // and retain its bundled fonts as fallbacks for missing glyphs or files.
    let mut fonts = FontDefinitions::default();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_REGULAR,
        REGULAR_FONT_PATHS,
        REGULAR_FONT_WEIGHT,
    ) {
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .expect("egui should define its proportional fallback family")
            .insert(0, SYSTEM_UI_REGULAR.to_owned());
    }
    let proportional = fonts
        .families
        .get(&FontFamily::Proportional)
        .expect("egui should define its proportional fallback family")
        .clone();

    let mut medium_family = Vec::new();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_MEDIUM,
        MEDIUM_FONT_PATHS,
        MEDIUM_FONT_WEIGHT,
    ) {
        medium_family.push(SYSTEM_UI_MEDIUM.to_owned());
    }
    medium_family.extend(proportional.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(MEDIUM_FONT_FAMILY.into()), medium_family);

    let mut semibold_family = Vec::new();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_SEMIBOLD,
        SEMIBOLD_FONT_PATHS,
        SEMIBOLD_FONT_WEIGHT,
    ) {
        semibold_family.push(SYSTEM_UI_SEMIBOLD.to_owned());
    }
    semibold_family.extend(proportional);
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD_FONT_FAMILY.into()),
        semibold_family,
    );
    context.set_fonts(fonts);
}

fn install_font_face(fonts: &mut FontDefinitions, name: &str, paths: &[&str], weight: f32) -> bool {
    let Some(bytes) = read_first_font(paths) else {
        return false;
    };
    fonts
        .font_data
        .insert(name.to_owned(), Arc::new(platform_font_data(bytes, weight)));
    true
}

fn platform_font_data(bytes: Vec<u8>, weight: f32) -> FontData {
    let data = FontData::from_owned(bytes);
    #[cfg(target_os = "macos")]
    {
        // SFNS defaults to its narrower display cut outside AppKit. Select the
        // text optical size explicitly so small editor labels match native UI.
        let mut tweak = FontTweak::default();
        tweak.coords.push(b"opsz", UI_OPTICAL_SIZE);
        tweak.coords.push(b"wght", weight);
        tweak.hinting = Some(true);
        tweak.subpixel_binning = Some(false);
        data.tweak(tweak)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = weight;
        data
    }
}

fn read_first_font(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| fs::read(path).ok())
}

fn medium_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM_FONT_FAMILY.into()))
}

fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD_FONT_FAMILY.into()))
}

fn configure_style(context: &egui::Context) {
    configure_theme_style(context, egui::Theme::Dark, DARK_PALETTE);
    configure_theme_style(context, egui::Theme::Light, LIGHT_PALETTE);
    context.set_theme(egui::ThemePreference::System);
}

fn configure_theme_style(context: &egui::Context, theme: egui::Theme, palette: Palette) {
    let mut style = (*context.style_of(theme)).clone();
    style.spacing.item_spacing = egui::vec2(4.0, 1.0);
    style.spacing.button_padding = egui::vec2(6.0, 1.0);
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.spacing.indent = 12.0;
    style.spacing.menu_margin = Margin::same(4);
    style.animation_time = 0.15;
    style.visuals.dark_mode = theme == egui::Theme::Dark;
    style.visuals.text_options.font_hinting = true;
    style.visuals.text_options.subpixel_binning = false;
    style.visuals.panel_fill = palette.panel;
    style.visuals.window_fill = palette.panel_raised;
    style.visuals.window_stroke = Stroke::new(1.0, palette.border);
    style.visuals.window_corner_radius = egui::CornerRadius::same(2);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(3);
    style.visuals.extreme_bg_color = palette.surface_deep;
    style.visuals.text_edit_bg_color = Some(palette.field);
    style.visuals.faint_bg_color = palette.surface;
    style.visuals.indent_has_left_vline = false;
    style.visuals.selection.bg_fill = palette.selection;
    style.visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.muted);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.hovered.bg_fill = palette.panel_raised;
    style.visuals.widgets.hovered.weak_bg_fill = palette.panel_raised;
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.active.bg_fill = palette.selection;
    style.visuals.widgets.active.weak_bg_fill = palette.selection;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.open.bg_fill = palette.field;
    style.visuals.widgets.open.weak_bg_fill = palette.field;
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(2);
    // Egui removes text padding for frameless buttons, including menu labels.
    // Keep the frame geometry and make inactive menu frames transparent instead.
    style.visuals.button_frame = true;
    style.visuals.slider_trailing_fill = true;
    style.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(TYPE.secondary));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(TYPE.secondary));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(TYPE.meta));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(TYPE.secondary));
    context.set_style_of(theme, style);
}

fn palette(ui: &egui::Ui) -> Palette {
    if ui.visuals().dark_mode {
        DARK_PALETTE
    } else {
        LIGHT_PALETTE
    }
}

fn editor_frame(fill: Color32) -> Frame {
    Frame::NONE.fill(fill).inner_margin(Margin::same(0))
}

fn load_logo_texture(context: &egui::Context) -> egui::TextureHandle {
    let image = image::load_from_memory(LOGO_BYTES)
        .expect("Cubacadabra Studio logo should be a valid image")
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    context.load_texture(
        "cubacadabra-studio-logo",
        color_image,
        egui::TextureOptions::LINEAR,
    )
}

#[cfg(target_os = "macos")]
fn install_system_icon_textures(context: &egui::Context) {
    let textures = MACOS_SYSTEM_SYMBOLS
        .iter()
        .filter_map(|&(icon, symbol)| {
            let png = crate::macos::system_symbol_png(symbol)?;
            let image = system_icon_color_image(&png)?;
            let texture = context.load_texture(
                format!("sf-symbol-{symbol}"),
                image,
                egui::TextureOptions::LINEAR,
            );
            Some((icon, texture))
        })
        .collect::<HashMap<_, _>>();
    context.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(SYSTEM_ICON_ATLAS_ID),
            Arc::new(textures) as SystemIconAtlas,
        );
    });
}

#[cfg(target_os = "macos")]
fn system_icon_color_image(png: &[u8]) -> Option<egui::ColorImage> {
    let rgba = image::load_from_memory(png).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut bounds = None::<(u32, u32, u32, u32)>;
    for (x, y, pixel) in rgba.enumerate_pixels() {
        if pixel[3] == 0 {
            continue;
        }
        bounds = Some(match bounds {
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
            None => (x, y, x, y),
        });
    }
    let (min_x, min_y, max_x, max_y) = bounds?;
    let min_x = min_x.saturating_sub(1);
    let min_y = min_y.saturating_sub(1);
    let max_x = (max_x + 1).min(width - 1);
    let max_y = (max_y + 1).min(height - 1);
    let mut cropped =
        image::imageops::crop_imm(&rgba, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
            .to_image();
    for pixel in cropped.pixels_mut() {
        *pixel = image::Rgba([255, 255, 255, pixel[3]]);
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [cropped.width() as usize, cropped.height() as usize],
        cropped.as_raw(),
    ))
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
    Frame::NONE.inner_margin(Margin::symmetric(6, 4))
}

fn workspace_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let colors = palette(ui);
    let font = if selected {
        medium_font(TYPE.primary)
    } else {
        FontId::proportional(TYPE.primary)
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        font,
        if selected {
            colors.text
        } else {
            colors.secondary_text
        },
    );
    let width = galley.size().x.ceil() + LABEL_PADDING * 2.0;
    let (slot, response) =
        ui.allocate_exact_size(egui::vec2(width, TOP_BAR_HEIGHT), Sense::click());
    let rect = Rect::from_min_max(slot.min, slot.max);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, UI.radius, colors.panel_raised);
    }
    if selected {
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 1.0,
            Stroke::new(1.0, colors.selection),
        );
    }
    ui.painter().galley(
        egui::pos2(
            rect.min.x + LABEL_PADDING,
            slot.center().y - galley.size().y * 0.5,
        ),
        galley,
        if selected {
            colors.text
        } else {
            colors.secondary_text
        },
    );
    paint_focus(ui, &response);
    response
}

// Allocate headers once: frame strokes/margins must never change their height.
fn editor_header(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) -> Rect {
    let colors = palette(ui);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), EDITOR_HEADER_HEIGHT),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, colors.panel_header);
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - 0.5,
        Stroke::new(1.0, colors.border),
    );
    let mut header = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(UI.inset, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    header.set_clip_rect(ui.clip_rect().intersect(rect));
    header.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
    content(&mut header);
    rect
}

fn panel_header(ui: &mut egui::Ui, icon: Icon, title: &str, actions: impl FnOnce(&mut egui::Ui)) {
    let colors = palette(ui);
    editor_header(ui, |ui| {
        inline_icon(ui, icon, colors.muted);
        ui.label(
            RichText::new(title)
                .font(semibold_font(TYPE.primary))
                .color(colors.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), actions);
    });
}

fn selected_object_header(ui: &mut egui::Ui, name: &str, kind: &str) {
    let colors = palette(ui);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), UI.row),
        Layout::left_to_right(Align::Center),
        |ui| {
            inline_icon(ui, Icon::Object, colors.muted);
            ui.add(
                egui::Label::new(
                    RichText::new(name)
                        .font(semibold_font(TYPE.primary))
                        .color(colors.text),
                )
                .truncate(),
            )
            .on_hover_text(kind);
        },
    );
}

fn property_section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    let colors = palette(ui);
    egui::CollapsingHeader::new(
        RichText::new(title)
            .font(semibold_font(TYPE.secondary))
            .color(colors.text),
    )
    .default_open(true)
    .show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        content(ui);
        ui.add_space(1.0);
    });
}

fn property_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let colors = palette(ui);
    let field = property_field(ui, label);
    ui.painter().rect_filled(field, UI.radius, colors.surface);
    ui.put(
        field.shrink2(egui::vec2(6.0, 0.0)),
        egui::Label::new(RichText::new(value).size(TYPE.secondary).color(colors.text)).truncate(),
    );
}

fn property_field(ui: &mut egui::Ui, label: &str) -> Rect {
    let colors = palette(ui);
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CONTROL_HEIGHT),
        Sense::hover(),
    );
    let label_width = 64.0;
    ui.painter().text(
        egui::pos2(row.min.x + label_width - 8.0, row.center().y),
        Align2::RIGHT_CENTER,
        label,
        FontId::proportional(TYPE.secondary),
        colors.secondary_text,
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
    let colors = palette(ui);
    let field = property_field(ui, label);
    ui.painter().rect_filled(field, UI.radius, colors.field);
    let mut value_rect = field;
    if !axis.is_empty() {
        value_rect.min.x += 20.0;
        ui.painter().text(
            field.left_center() + egui::vec2(10.0, 0.0),
            Align2::CENTER_CENTER,
            axis,
            FontId::proportional(TYPE.meta),
            axis_color(axis, colors),
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
    let colors = palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), UI.row), Sense::click());
    let is_selected = *selected == name;
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, is_selected, name)
    });
    if is_selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0.0,
            if is_selected {
                colors.selection
            } else {
                colors.panel_raised
            },
        );
    }
    paint_focus(ui, &response);
    let x = rect.min.x + UI.inset + depth as f32 * 12.0;
    if matches!(icon, Icon::Folder | Icon::World) {
        paint_icon(
            ui.painter(),
            Rect::from_center_size(egui::pos2(x + 5.0, rect.center().y), Vec2::splat(10.0)),
            if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            colors.faint,
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(egui::pos2(x + 19.0, rect.center().y), Vec2::splat(UI.icon)),
        icon,
        if is_selected {
            colors.text
        } else {
            colors.faint
        },
    );
    let label_font = if is_selected || matches!(icon, Icon::Folder | Icon::World) {
        medium_font(TYPE.primary)
    } else {
        FontId::proportional(TYPE.primary)
    };
    ui.painter().text(
        egui::pos2(x + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        label_font,
        if is_selected {
            colors.text
        } else {
            colors.secondary_text
        },
    );
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.right_center() - egui::vec2(8.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        Icon::Eye,
        if response.hovered() || is_selected {
            colors.muted
        } else {
            colors.surface
        },
    );
    if response.clicked() {
        *selected = name;
    }
}

fn asset_tile(ui: &mut egui::Ui, name: &'static str, selected: bool, selection: &mut &'static str) {
    let colors = palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(76.0, 62.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, name)
    });
    let border = if selected {
        colors.asset_selection_stroke
    } else if response.hovered() {
        colors.border_strong
    } else {
        Color32::TRANSPARENT
    };
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if selected {
                colors.asset_selection
            } else {
                colors.panel_raised
            },
        );
    }
    let preview = Rect::from_min_max(
        rect.min + egui::vec2(6.0, 4.0),
        egui::pos2(rect.max.x - 6.0, rect.max.y - 17.0),
    );
    ui.painter().rect_filled(preview, UI.radius, colors.surface);
    let (asset_icon, kind) = asset_kind(name);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(preview.center(), Vec2::splat(22.0)),
        asset_icon,
        colors.muted,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y - 8.0),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional(TYPE.meta),
        if selected { colors.text } else { colors.muted },
    );
    ui.painter().rect_stroke(
        rect,
        UI.radius,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    paint_focus(ui, &response);
    if response.clicked() {
        *selection = name;
    }
    response.on_hover_text(format!("{name} · {} preview", kind.to_lowercase()));
}

fn compact_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let colors = palette(ui);
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(TYPE.secondary),
        if selected { colors.text } else { colors.muted },
    );
    let width = galley.size().x.ceil() + 12.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, UI.radius, colors.panel_raised);
    }
    if selected {
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 1.0,
            Stroke::new(1.0, colors.selection),
        );
    }
    ui.painter().galley(
        rect.center() - galley.size() * 0.5,
        galley,
        if selected { colors.text } else { colors.muted },
    );
    paint_focus(ui, &response);
    response
}

fn navigation_row(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let colors = palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), UI.row), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if selected {
                colors.selection
            } else {
                colors.panel_raised
            },
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(11.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        if selected {
            colors.accent
        } else {
            colors.muted
        },
    );
    let label_font = if selected {
        medium_font(TYPE.primary)
    } else {
        FontId::proportional(TYPE.primary)
    };
    ui.painter().text(
        rect.left_center() + egui::vec2(24.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        label_font,
        if selected { colors.text } else { colors.muted },
    );
    paint_focus(ui, &response);
    response
}

fn search_field(ui: &mut egui::Ui, query: &mut String, width: f32) {
    let colors = palette(ui);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.min(ui.available_width()).max(80.0), CONTROL_HEIGHT),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, UI.radius, colors.surface_deep);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(11.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        Icon::Search,
        colors.faint,
    );
    let text_rect = Rect::from_min_max(
        rect.min + egui::vec2(23.0, 1.0),
        rect.max - egui::vec2(4.0, 1.0),
    );
    let response = ui.put(
        text_rect,
        egui::TextEdit::singleline(query)
            .hint_text("Search assets…")
            .font(FontId::proportional(TYPE.secondary))
            .margin(Margin::ZERO)
            .frame(Frame::NONE),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Search assets")
    });
}

fn drop_target(ui: &mut egui::Ui, label: &str) {
    let colors = palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 52.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        UI.radius,
        if response.hovered() {
            colors.panel_raised
        } else {
            colors.surface
        },
    );
    ui.painter().rect_stroke(
        rect,
        UI.radius,
        Stroke::new(
            1.0,
            if response.hovered() {
                colors.accent
            } else {
                colors.border
            },
        ),
        StrokeKind::Inside,
    );
    let icon_rect =
        Rect::from_center_size(rect.center() - egui::vec2(0.0, 7.0), Vec2::splat(UI.icon));
    paint_icon(ui.painter(), icon_rect, Icon::Open, colors.muted);
    ui.painter().text(
        rect.center() + egui::vec2(0.0, 10.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(TYPE.secondary),
        colors.muted,
    );
}

#[cfg(not(target_os = "macos"))]
fn menu_entry(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> egui::Response {
    let colors = palette(ui);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(220.0, 24.0), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if enabled && (response.hovered() || response.has_focus()) {
        ui.painter().rect_filled(rect, UI.radius, colors.selection);
    }
    let color = if enabled { colors.text } else { colors.faint };
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(13.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        if enabled { colors.muted } else { colors.faint },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(28.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(TYPE.primary),
        color,
    );
    if !shortcut.is_empty() {
        ui.painter().text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::proportional(TYPE.meta),
            colors.faint,
        );
    }
    response
}

fn toolbar_button(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool) -> egui::Response {
    let colors = palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(54.0, CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if active {
                colors.selection
            } else {
                colors.panel_raised
            },
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(12.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        if active { colors.accent } else { colors.text },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(22.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        medium_font(TYPE.secondary),
        colors.text,
    );
    paint_focus(ui, &response);
    response
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str, active: bool) -> egui::Response {
    let colors = palette(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if active {
                colors.field
            } else {
                colors.panel_raised
            },
        );
    }
    paint_icon(
        ui.painter(),
        rect.shrink(3.0),
        icon,
        if active { colors.text } else { colors.muted },
    );
    paint_focus(ui, &response);
    response.on_hover_text(tooltip)
}

fn paint_focus(ui: &egui::Ui, response: &egui::Response) {
    let colors = palette(ui);
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            UI.radius,
            Stroke::new(1.0, colors.accent),
            StrokeKind::Inside,
        );
    }
}

fn inline_icon(ui: &mut egui::Ui, icon: Icon, color: Color32) {
    let response = ui.allocate_response(Vec2::splat(UI.icon), Sense::hover());
    paint_icon(ui.painter(), response.rect, icon, color);
}

fn paint_status_label(ui: &egui::Ui, rect: Rect, color: Color32, label: &str) {
    ui.painter()
        .circle_filled(rect.left_center() + egui::vec2(9.0, 0.0), 3.0, color);
    ui.painter().text(
        rect.left_center() + egui::vec2(18.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(TYPE.meta),
        color,
    );
}

fn vertical_separator(ui: &mut egui::Ui, height: f32) {
    let colors = palette(ui);
    let response = ui.allocate_response(egui::vec2(1.0, height), Sense::hover());
    ui.painter().line_segment(
        [response.rect.center_top(), response.rect.center_bottom()],
        Stroke::new(1.0, colors.border),
    );
}

fn paint_down_chevron(ui: &mut egui::Ui) {
    let colors = palette(ui);
    let response = ui.allocate_response(Vec2::splat(UI.icon), Sense::hover());
    paint_icon(ui.painter(), response.rect, Icon::ChevronDown, colors.faint);
}

fn axis_color(axis: &str, colors: Palette) -> Color32 {
    match axis {
        "X" => colors.axis_x,
        "Y" => colors.axis_y,
        "Z" => colors.axis_z,
        _ => colors.muted,
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

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

fn source_lod_status(summary: &MorphGlbSourceSummary, level: &str) -> String {
    match summary.lod_candidates.get(level) {
        Some(candidates) if candidates.len() == 1 => format!("Mapped: {}", candidates[0]),
        Some(candidates) if candidates.is_empty() => "Missing mapping".to_owned(),
        Some(candidates) => format!("Ambiguous: {} candidates", candidates.len()),
        None => "Missing mapping".to_owned(),
    }
}

fn morph_kind_label(kind: MorphAssetKind) -> &'static str {
    match kind {
        MorphAssetKind::Base => "Bases",
        MorphAssetKind::Face => "Faces",
        MorphAssetKind::Hair => "Hair",
        MorphAssetKind::Outfit => "Outfits",
        MorphAssetKind::Top => "Tops",
        MorphAssetKind::Outerwear => "Outerwear",
        MorphAssetKind::Bottom => "Bottoms",
        MorphAssetKind::OnePiece => "One-piece",
        MorphAssetKind::Footwear => "Footwear",
        MorphAssetKind::Headwear => "Headwear",
        MorphAssetKind::Facewear => "Facewear",
        MorphAssetKind::Accessory => "Accessories",
        MorphAssetKind::Tail => "Tails",
        MorphAssetKind::Wings => "Wings",
        MorphAssetKind::Horns => "Horns",
        MorphAssetKind::Ears => "Ears",
        MorphAssetKind::HeldItem => "Held items",
    }
}

fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    #[cfg(target_os = "macos")]
    if paint_system_icon(painter, rect, icon, color) {
        return;
    }

    let c = rect.center();
    let size = rect.width().min(rect.height()).max(1.0);
    let r = size * 0.42;
    let stroke = Stroke::new((size * 0.09).clamp(1.0, 1.5), color);
    let left = c.x - r;
    let right = c.x + r;
    let top = c.y - r;
    let bottom = c.y + r;
    match icon {
        Icon::Object => {
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
        #[cfg(not(target_os = "macos"))]
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
        #[cfg(not(target_os = "macos"))]
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
        #[cfg(not(target_os = "macos"))]
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

#[cfg(target_os = "macos")]
fn paint_system_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) -> bool {
    let atlas = painter
        .ctx()
        .data(|data| data.get_temp::<SystemIconAtlas>(egui::Id::new(SYSTEM_ICON_ATLAS_ID)));
    let Some(texture) = atlas.as_ref().and_then(|atlas| atlas.get(&icon)) else {
        return false;
    };
    let texture_size = texture.size_vec2();
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
        return false;
    }
    let scale = (rect.width() / texture_size.x).min(rect.height() / texture_size.y) * 0.94;
    let symbol_rect = Rect::from_center_size(rect.center(), texture_size * scale);
    painter.image(
        texture.id(),
        symbol_rect,
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        color,
    );
    true
}
