use crate::{
    codex::{ChatGptAccount, CodexWorkStatus},
    morphs::{
        MorphDraftDocument, MorphGlbPreviewMesh, MorphGlbSourceSummary, MorphSourceManifest,
        build_morph_draft_json, build_source_manifest_json, default_rigid_accessory_asset,
        fit_rigid_headwear_to_person, morph_mesh_bounds,
    },
    project::{SourceAsset, SourceAssetKind},
};
use cubacadabra_builder::{AuthoringNode, AuthoringScene, parse_authoring_scene};
use cubacadabra_client::native::Renderer as GameRenderer;
use cubacadabra_morph_authoring::{MorphAttachment, MorphAttachmentMode};
use cubacadabra_morphs::{MorphAssetId, MorphAssetKind, MorphCatalog, parse_catalog};
#[cfg(target_os = "macos")]
use egui::FontTweak;
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Frame, Layout, Margin,
    Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextStyle, Vec2,
};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
#[cfg(test)]
pub(crate) use egui_wgpu::wgpu;
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use egui_winit::State as EguiState;
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::collections::HashMap;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{event::WindowEvent, window::Window};

#[path = "shell/morph_library.rs"]
mod morph_library;
#[path = "shell/morph_preview.rs"]
#[allow(dead_code)]
mod morph_preview;
#[cfg(test)]
#[path = "shell/morph_ui_tests.rs"]
mod morph_ui_tests;
#[path = "shell/scene.rs"]
mod scene;
#[path = "shell/studio_chrome.rs"]
mod studio_chrome;
#[path = "shell/studio_codex_ui.rs"]
mod studio_codex_ui;
#[path = "shell/studio_commands.rs"]
mod studio_commands;
#[path = "shell/studio_morph_state.rs"]
mod studio_morph_state;
#[path = "shell/studio_morph_ui.rs"]
mod studio_morph_ui;
#[path = "shell/studio_project_ui.rs"]
mod studio_project_ui;
#[path = "shell/studio_scene_ui.rs"]
mod studio_scene_ui;
#[path = "shell/studio_shell.rs"]
mod studio_shell;
#[path = "shell/studio_source_ui.rs"]
mod studio_source_ui;
#[path = "shell/studio_start_ui.rs"]
mod studio_start_ui;
#[path = "shell/studio_state.rs"]
mod studio_state;
#[path = "shell/studio_test_ui.rs"]
mod studio_test_ui;
#[path = "shell/studio_world.rs"]
mod studio_world;
#[path = "shell/style.rs"]
mod style;
#[path = "shell/style_controls.rs"]
mod style_controls;
#[path = "shell/style_icons.rs"]
mod style_icons;
#[path = "shell/style_layout.rs"]
mod style_layout;
#[cfg(test)]
mod tests;
#[path = "shell/theme.rs"]
mod theme;

#[cfg(test)]
pub(crate) use morph_preview::projected_triangle_area;
pub(crate) use scene::*;
use style::*;
use style_controls::*;
use style_icons::*;
use style_layout::*;
pub(crate) use theme::*;

pub(crate) fn source_syntax_for_path(path: &Path) -> Syntax {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        Syntax::new("JSON")
    } else {
        Syntax::lua()
            .with_keywords([
                "and", "break", "do", "else", "elseif", "end", "for", "function", "if", "in",
                "local", "not", "or", "repeat", "return", "then", "until", "while", "continue",
                "export", "type", "typeof", "self",
            ])
            .with_types([
                "boolean", "number", "string", "function", "userdata", "thread", "table", "vector",
                "CFrame", "Color3", "Instance",
            ])
            .with_special(["false", "nil", "true"])
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct SourceAudioPreview {
    path: PathBuf,
    _stream: rodio::OutputStream,
    sink: rodio::Sink,
    duration: Option<Duration>,
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
    NewProject,
    OpenProject,
    Save,
    Undo,
    Redo,
    #[allow(dead_code)]
    CloseWindow,
    Copy,
    ShowWorld,
    ShowScripts,
    ShowAssets,
    ShowMaterials,
    ShowMorphs,
    ShowTest,
}

pub(crate) struct PreparedShell {
    paint_jobs: Vec<egui::ClippedPrimitive>,
    screen: ScreenDescriptor,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CodexChatRole {
    User,
    Assistant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodexActivity {
    Idle,
    Thinking,
    Editing,
    Checking,
    Working,
    Cancelling,
    Rebuilding,
}

impl CodexActivity {
    fn is_active(self) -> bool {
        self != Self::Idle
    }

    fn is_cancellable(self) -> bool {
        matches!(
            self,
            Self::Thinking | Self::Editing | Self::Checking | Self::Working
        )
    }

    fn after_agent_progress(self) -> Self {
        if self.is_cancellable() {
            Self::Working
        } else {
            self
        }
    }
}

const CODEX_CHAT_DEFAULT_MODEL: &str = "gpt-6-astra";
const CODEX_CHAT_CURRENT_MODEL: &str = "gpt-5.6-luna";
const CODEX_CHAT_DEFAULT_EFFORT: &str = "medium";
const CODEX_CHAT_MODELS: [(&str, &str); 4] = [
    (
        "gpt-6-astra",
        "Our most capable model for complex, demanding work.",
    ),
    (
        "gpt-5.6-sol",
        "Reliable agentic workhorse for everyday tasks.",
    ),
    (
        "gpt-5.6-terra",
        "Balanced agentic coding model for everyday work.",
    ),
    ("gpt-5.6-luna", "Fast and affordable agentic coding model."),
];
const CODEX_CHAT_EFFORTS: [(&str, &str); 4] = [
    (
        "medium",
        "Balances speed and reasoning depth for everyday tasks.",
    ),
    ("high", "Greater reasoning depth for complex problems."),
    ("xhigh", "Extra high reasoning depth for complex problems."),
    ("max", "Maximum reasoning depth for the hardest problems."),
];

struct CodexChatMessage {
    role: CodexChatRole,
    text: String,
}

pub(crate) struct CodexChatSendRequest {
    pub(crate) message: String,
    pub(crate) model: &'static str,
    pub(crate) reasoning_effort: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) enum SceneEditRequest {
    AddObject {
        world_id: Option<String>,
        kind: SceneObjectKind,
    },
    UseImageAsFloor {
        asset_path: PathBuf,
    },
    UpdateTransform {
        target: String,
        position: [f32; 3],
        size: Option<[f32; 3]>,
        scale: Option<[f32; 3]>,
    },
    UpdateSignText {
        target: String,
        text: String,
    },
    UpdateProperty {
        target: String,
        key: String,
        value: Value,
    },
    DuplicateObject {
        target: String,
    },
    DeleteObject {
        target: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneObjectKind {
    Block,
    Sign,
    Ladder,
    Interaction,
    Checkpoint,
    Hazard,
    SafeZone,
}

impl SceneObjectKind {
    pub(crate) const ALL: [Self; 7] = [
        Self::Block,
        Self::Sign,
        Self::Ladder,
        Self::Interaction,
        Self::Checkpoint,
        Self::Hazard,
        Self::SafeZone,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Block => "Block",
            Self::Sign => "Sign",
            Self::Ladder => "Ladder",
            Self::Interaction => "Interaction",
            Self::Checkpoint => "Checkpoint",
            Self::Hazard => "Hazard",
            Self::SafeZone => "Safe zone",
        }
    }

    pub(crate) fn collection(self) -> &'static str {
        match self {
            Self::Block => "blocks",
            Self::Sign => "signs",
            Self::Ladder => "ladders",
            Self::Interaction => "interactions",
            Self::Checkpoint => "checkpoints",
            Self::Hazard => "hazards",
            Self::SafeZone => "safeZones",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SceneViewportTool {
    #[default]
    Move,
    Resize,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneObjectGeometry {
    pub(crate) id: String,
    pub(crate) position: [f32; 3],
    pub(crate) size: Option<[f32; 3]>,
    pub(crate) scale: Option<[f32; 3]>,
    pub(crate) editable: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneObjectProjection {
    pub(crate) id: String,
    pub(crate) position: [f32; 3],
    pub(crate) size: Option<[f32; 3]>,
    pub(crate) scale: Option<[f32; 3]>,
    pub(crate) base_size: Option<[f32; 3]>,
    pub(crate) editable: bool,
    pub(crate) center_screen: Pos2,
    pub(crate) world_corners: Option<[[f32; 3]; 4]>,
    pub(crate) screen_corners: Option<[Pos2; 4]>,
}

impl SceneObjectProjection {
    pub(crate) fn bounds(&self) -> Rect {
        self.screen_corners.map_or_else(
            || Rect::from_center_size(self.center_screen, Vec2::splat(20.0)),
            |corners| Rect::from_points(&corners).expand(4.0),
        )
    }

    pub(crate) fn contains(&self, point: Pos2) -> bool {
        self.screen_corners.map_or_else(
            || self.center_screen.distance(point) <= 10.0,
            |corners| point_in_scene_quad(point, corners),
        )
    }
}

fn point_in_scene_quad(point: Pos2, corners: [Pos2; 4]) -> bool {
    let mut sign = 0.0_f32;
    for index in 0..4 {
        let a = corners[index];
        let b = corners[(index + 1) % 4];
        let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        if cross.abs() <= f32::EPSILON {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign != cross.signum() {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneViewportEditPhase {
    Begin,
    Update,
    Commit,
}

#[derive(Clone, Debug)]
pub(crate) enum SceneViewportEditRequest {
    Move {
        phase: SceneViewportEditPhase,
        target: String,
        origin_screen: Pos2,
        current_screen: Pos2,
        origin_position: [f32; 3],
        size: Option<[f32; 3]>,
    },
    Resize {
        phase: SceneViewportEditPhase,
        target: String,
        current_screen: Pos2,
        fixed_corner: [f32; 3],
        origin_position: [f32; 3],
        origin_size: [f32; 3],
        origin_scale: Option<[f32; 3]>,
        base_size: Option<[f32; 3]>,
    },
    ResizeHeight {
        phase: SceneViewportEditPhase,
        target: String,
        origin_screen: Pos2,
        current_screen: Pos2,
        origin_position: [f32; 3],
        origin_scale: [f32; 3],
        base_size: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReviewCameraPreset {
    Gameplay,
    Overview,
    Showcase,
}

impl ReviewCameraPreset {
    pub(crate) const ALL: [Self; 3] = [Self::Gameplay, Self::Overview, Self::Showcase];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Gameplay => "Gameplay",
            Self::Overview => "Overview",
            Self::Showcase => "Showcase",
        }
    }

    pub(crate) const fn renderer_value(self) -> cubacadabra_client::StudioCameraPreset {
        match self {
            Self::Gameplay => cubacadabra_client::StudioCameraPreset::Gameplay,
            Self::Overview => cubacadabra_client::StudioCameraPreset::Overview,
            Self::Showcase => cubacadabra_client::StudioCameraPreset::Showcase,
        }
    }
}

pub(crate) struct StudioShell {
    context: egui::Context,
    state: EguiState,
    renderer: EguiRenderer,
    workspace: Workspace,
    review_camera: ReviewCameraPreset,
    review_camera_reset: bool,
    runtime_viewport: Rect,
    scene_outline: SceneOutline,
    authoring_scene_source: Option<String>,
    runtime_ui_nodes: Vec<cubacadabra_client::StudioUiNode>,
    expanded_scene: BTreeSet<String>,
    selected_scene: String,
    scene_focus_requested: bool,
    scene_editor_target: String,
    scene_editor_position: [f32; 3],
    scene_editor_size: [f32; 3],
    scene_editor_scale: [f32; 3],
    scene_editor_position_text: [String; 3],
    scene_editor_size_text: [String; 3],
    scene_editor_scale_text: [String; 3],
    scene_editor_text: String,
    scene_editor_properties: BTreeMap<String, String>,
    scene_viewport_tool: SceneViewportTool,
    scene_object_projections: Vec<SceneObjectProjection>,
    scene_viewport_edit_requested: Option<SceneViewportEditRequest>,
    selected_world_asset: String,
    selected_asset: &'static str,
    test_tool: &'static str,
    asset_filter: &'static str,
    playing: bool,
    project_editable: bool,
    project_dirty: bool,
    preview_stale: bool,
    project_error: Option<String>,
    scene_edit_requested: Option<SceneEditRequest>,
    undo_requested: bool,
    redo_requested: bool,
    save_requested: bool,
    rebuild_and_play_requested: bool,
    restart_requested: bool,
    notice: String,
    search_query: String,
    scene_search_query: String,
    scene_search_matches: BTreeSet<String>,
    scene_tree_rows: Vec<SceneTreeRow>,
    scene_tree_rows_dirty: bool,
    morph_query: String,
    morph_catalog: MorphCatalog,
    morph_artifacts: BTreeMap<MorphAssetId, crate::wardrobe::Artifact>,
    morph_request: Option<crate::wardrobe::Request>,
    morph_starters: bool,
    morph_loading: bool,
    morph_catalog_ready: bool,
    morph_catalog_error: Option<String>,
    morph_thumbnails: BTreeMap<String, egui::TextureHandle>,
    active_loadout: cubacadabra_morphs::MorphLoadout,
    active_morphs: BTreeSet<String>,
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
    morph_attachment_translation: [f32; 3],
    morph_attachment_rotation: [f32; 4],
    morph_attachment_scale: [f32; 3],
    morph_lod_nodes: [String; 3],
    morph_draft_status: Option<(bool, String)>,
    morph_draft_export_requested: bool,
    morph_sidecar_export_requested: bool,
    morph_project_add_requested: bool,
    morph_pack_import_requested: bool,
    morph_publish_requested: bool,
    morph_thumbnail_requested: bool,
    morph_import_error: Option<String>,
    project_asset_available: bool,
    auth_requested: bool,
    auth_pending: bool,
    auth_user: Option<crate::network::AuthUser>,
    chatgpt_auth_requested: bool,
    chatgpt_pending: bool,
    chatgpt_available: bool,
    chatgpt_account: Option<ChatGptAccount>,
    chatgpt_error: Option<String>,
    codex_project_root: PathBuf,
    codex_chat_open: bool,
    codex_chat_open_requested: bool,
    codex_chat_send_requested: Option<CodexChatSendRequest>,
    codex_chat_messages: Vec<CodexChatMessage>,
    codex_chat_draft: String,
    codex_chat_model: &'static str,
    codex_chat_reasoning_effort: &'static str,
    codex_chat_ready: bool,
    codex_activity: CodexActivity,
    codex_live_excerpt: String,
    codex_live_pending_excerpt: String,
    codex_live_excerpt_queue: VecDeque<String>,
    codex_live_last_published_at: Option<Instant>,
    codex_live_last_received_at: Option<Instant>,
    codex_live_needs_separator: bool,
    codex_live_in_code_block: bool,
    codex_cancel_requested: bool,
    codex_cancel_sent: bool,
    codex_chat_error: Option<String>,
    codex_change_files: Option<Vec<String>>,
    codex_source_change_count: usize,
    codex_change_review_open: bool,
    codex_undo_requested: bool,
    source_files: BTreeMap<PathBuf, String>,
    source_assets: BTreeMap<PathBuf, SourceAsset>,
    source_directories: BTreeSet<PathBuf>,
    source_collapsed_directories: BTreeSet<PathBuf>,
    source_import_requested: Option<PathBuf>,
    dropped_files: Vec<PathBuf>,
    selected_source_file: Option<PathBuf>,
    selected_source_asset: Option<PathBuf>,
    source_asset_texture: Option<(PathBuf, egui::TextureHandle, [usize; 2])>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    audio_preview: Option<SourceAudioPreview>,
    source_editor_text: String,
    source_editor: CodeEditor,
    source_syntax: Syntax,
    start_screen: bool,
    start_screen_logged: bool,
    recent_projects: Vec<PathBuf>,
    recent_project_requested: Option<PathBuf>,
    open_project_requested: bool,
    project_loading: Option<ProjectLoadingState>,
    new_project_dialog_open: bool,
    new_project_title: String,
    new_project_parent: PathBuf,
    #[cfg(not(target_os = "macos"))]
    new_project_folder_requested: bool,
    #[cfg(not(target_os = "macos"))]
    new_project_create_requested: bool,
    new_project_error: Option<String>,
    #[cfg(not(target_os = "macos"))]
    new_project_title_focus_requested: bool,
    logo_texture: egui::TextureHandle,
    pending_project_action: Option<PendingProjectAction>,
    exit_requested: bool,
    imported_asset_paths: Vec<PathBuf>,
    roughness: f32,
    pending_textures_delta: egui::TexturesDelta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingProjectAction {
    NewProject,
    OpenProject,
    Close,
}
