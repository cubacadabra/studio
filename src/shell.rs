use crate::{
    codex::{ChatGptAccount, CodexWorkStatus},
    morphs::{
        MorphDraftDocument, MorphGlbPreviewMesh, MorphGlbSourceSummary, MorphSourceManifest,
        build_morph_draft_json, build_source_manifest_json, default_rigid_accessory_asset,
        fit_rigid_headwear_to_person, morph_mesh_bounds,
    },
};
use cubacadabra_client::native::Renderer as GameRenderer;
use cubacadabra_morph_authoring::{MorphAttachment, MorphAttachmentMode};
use cubacadabra_morphs::{MorphAssetId, MorphAssetKind, MorphCatalog, parse_catalog};
#[cfg(target_os = "macos")]
use egui::FontTweak;
use egui::{
    Align, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Frame, Layout, Margin,
    Rect, RichText, Sense, Stroke, StrokeKind, TextStyle, Vec2,
};
#[cfg(test)]
pub(crate) use egui_wgpu::wgpu;
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use egui_winit::State as EguiState;
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::collections::HashMap;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
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
#[path = "shell/studio_shell.rs"]
mod studio_shell;
#[path = "shell/style.rs"]
mod style;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use morph_preview::projected_triangle_area;
use style::*;

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
pub(crate) struct Palette {
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
    Sparkles,
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
    (Icon::Sparkles, "sparkles"),
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
pub(crate) enum Workspace {
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

#[derive(Clone, Debug)]
pub(crate) struct SceneNode {
    id: String,
    label: String,
    kind: &'static str,
    icon: Icon,
    detail: Option<String>,
    properties: Vec<(String, String)>,
    children: Vec<SceneNode>,
}

impl SceneNode {
    fn find(&self, id: &str) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }
}

#[derive(Clone, Debug)]
struct SceneOutline {
    root: SceneNode,
    assets: Vec<ManifestAsset>,
    initial_selection: String,
    initial_expanded: BTreeSet<String>,
}

struct ProjectLoadingState {
    progress: f32,
    previous_outline: SceneOutline,
    previous_expanded: BTreeSet<String>,
    previous_selection: String,
    previous_world_asset: String,
    previous_workspace: Workspace,
}

#[derive(Clone, Debug)]
struct ManifestAsset {
    name: String,
    kind: &'static str,
    icon: Icon,
}

impl SceneOutline {
    fn parse(source: &str) -> Result<Self, serde_json::Error> {
        let manifest: Value = serde_json::from_str(source)?;
        let assets = manifest_assets(&manifest);
        let game_name = manifest
            .get("displayName")
            .and_then(Value::as_str)
            .or_else(|| manifest.get("id").and_then(Value::as_str))
            .unwrap_or("Game")
            .to_owned();
        let game_id = manifest.get("id").and_then(Value::as_str).unwrap_or("game");
        let start_world = manifest
            .get("startWorld")
            .and_then(Value::as_str)
            .unwrap_or("lobby");
        let lobby_enabled = manifest
            .get("lobby")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let active_world = if lobby_enabled || start_world != "lobby" {
            start_world
        } else {
            manifest
                .pointer("/launch/destinationWorld")
                .and_then(Value::as_str)
                .unwrap_or(start_world)
        };

        let mut worlds = vec![world_node("lobby", &manifest, active_world == "lobby")];
        if let Some(entries) = manifest.get("worlds").and_then(Value::as_object) {
            for (world_id, definition) in entries {
                if world_id != "lobby" {
                    worlds.push(world_node(world_id, definition, active_world == world_id));
                }
            }
        }

        let world_count = worlds.len();
        let mut root_children = worlds;
        if let Some(avatars) = manifest.get("avatars").and_then(Value::as_object) {
            let mut characters = Vec::new();
            if let Some(player) = avatars.get("player").filter(|value| value.is_object()) {
                characters.push(SceneNode {
                    id: "game/characters/player".to_owned(),
                    label: "Player".to_owned(),
                    kind: "Character",
                    icon: Icon::Character,
                    detail: None,
                    properties: avatar_properties(player),
                    children: Vec::new(),
                });
            }
            if let Some(npcs) = avatars.get("npcs").and_then(Value::as_array) {
                characters.extend(npcs.iter().enumerate().map(|(index, npc)| SceneNode {
                    id: format!("game/characters/npc/{index}"),
                    label: item_label(npc, "Character", index),
                    kind: "Character",
                    icon: Icon::Character,
                    detail: None,
                    properties: avatar_properties(npc),
                    children: Vec::new(),
                }));
            }
            if !characters.is_empty() {
                root_children.push(SceneNode {
                    id: "game/characters".to_owned(),
                    label: "Characters".to_owned(),
                    kind: "Collection",
                    icon: Icon::Folder,
                    detail: Some(characters.len().to_string()),
                    properties: Vec::new(),
                    children: characters,
                });
            }
        }

        let mut root_properties = vec![
            ("Identifier".to_owned(), game_id.to_owned()),
            ("Start world".to_owned(), humanize_identifier(active_world)),
        ];
        if let Some(version) = manifest.get("version").and_then(Value::as_str) {
            root_properties.push(("Version".to_owned(), version.to_owned()));
        }
        if let Some(value) = manifest
            .pointer("/scene/maxPlayers")
            .and_then(compact_value)
        {
            root_properties.push(("Max players".to_owned(), value));
        }

        let root_id = "game".to_owned();
        let initial_selection = root_children
            .iter()
            .take(world_count)
            .find(|world| world.detail.as_deref() == Some("Start"))
            .map(|world| world.id.clone())
            .unwrap_or_else(|| root_id.clone());
        let initial_expanded = BTreeSet::from([root_id.clone(), initial_selection.clone()]);

        Ok(Self {
            root: SceneNode {
                id: root_id,
                label: game_name,
                kind: "Game",
                icon: Icon::World,
                detail: Some(format!(
                    "{world_count} {}",
                    if world_count == 1 { "world" } else { "worlds" }
                )),
                properties: root_properties,
                children: root_children,
            },
            assets,
            initial_selection,
            initial_expanded,
        })
    }

    fn empty() -> Self {
        let root = SceneNode {
            id: "game".to_owned(),
            label: "Game".to_owned(),
            kind: "Game",
            icon: Icon::World,
            detail: None,
            properties: Vec::new(),
            children: Vec::new(),
        };
        Self {
            initial_selection: root.id.clone(),
            initial_expanded: BTreeSet::from([root.id.clone()]),
            assets: Vec::new(),
            root,
        }
    }
}

const WORLD_COLLECTIONS: [(&str, &str, &str, Icon); 11] = [
    ("launchPads", "Launch Pads", "Launch Pad", Icon::Object),
    ("blocks", "Blocks", "Block", Icon::Object),
    ("portals", "Portals", "Portal", Icon::Object),
    ("signs", "Signs", "Sign", Icon::Object),
    ("billboards", "Billboards", "Billboard", Icon::Image),
    ("interactions", "Interactions", "Interaction", Icon::Object),
    ("ladders", "Ladders", "Ladder", Icon::Object),
    ("checkpoints", "Checkpoints", "Checkpoint", Icon::Object),
    ("hazards", "Hazards", "Hazard", Icon::Object),
    ("safeZones", "Safe Zones", "Safe Zone", Icon::Object),
    ("clouds", "Clouds", "Cloud", Icon::Object),
];

fn world_node(world_id: &str, definition: &Value, active: bool) -> SceneNode {
    let id = format!("world/{world_id}");
    let settings = definition.get("world").unwrap_or(&Value::Null);
    let mut properties = vec![("Identifier".to_owned(), world_id.to_owned())];
    for (pointer, label) in [
        ("/groundSize", "Ground size"),
        ("/gridSize", "Grid size"),
        ("/gridDivisions", "Grid divisions"),
        ("/spawn", "Spawn"),
        ("/physics/gravity", "Gravity"),
        ("/health/max", "Max health"),
    ] {
        if let Some(value) = settings.pointer(pointer).and_then(compact_value) {
            properties.push((label.to_owned(), value));
        }
    }

    let mut children = Vec::new();
    if settings.is_object() {
        let mut environment_children = Vec::new();
        if let Some(spawn) = settings.get("spawn").and_then(compact_value) {
            environment_children.push(SceneNode {
                id: format!("{id}/environment/spawn"),
                label: "Spawn".to_owned(),
                kind: "Spawn Point",
                icon: Icon::Object,
                detail: None,
                properties: vec![("Position".to_owned(), spawn)],
                children: Vec::new(),
            });
        }
        if let Some(clouds) = settings
            .get("clouds")
            .and_then(Value::as_array)
            .filter(|clouds| !clouds.is_empty())
        {
            environment_children.push(collection_node(
                &format!("{id}/environment"),
                "clouds",
                "Clouds",
                "Cloud",
                Icon::Object,
                clouds,
            ));
        }
        if !environment_children.is_empty() {
            children.push(SceneNode {
                id: format!("{id}/environment"),
                label: "Environment".to_owned(),
                kind: "Collection",
                icon: Icon::Folder,
                detail: Some(environment_children.len().to_string()),
                properties: Vec::new(),
                children: environment_children,
            });
        }
    }

    if let Some(materials) = definition
        .get("materials")
        .and_then(Value::as_object)
        .filter(|materials| !materials.is_empty())
    {
        let material_children = materials
            .iter()
            .map(|(name, value)| SceneNode {
                id: format!("{id}/materials/{name}"),
                label: humanize_identifier(name),
                kind: "Material",
                icon: Icon::Material,
                detail: None,
                properties: object_properties(value),
                children: Vec::new(),
            })
            .collect::<Vec<_>>();
        children.push(SceneNode {
            id: format!("{id}/materials"),
            label: "Materials".to_owned(),
            kind: "Collection",
            icon: Icon::Folder,
            detail: Some(material_children.len().to_string()),
            properties: Vec::new(),
            children: material_children,
        });
    }

    for (key, plural, singular, icon) in WORLD_COLLECTIONS {
        if key == "clouds" {
            continue;
        }
        if let Some(items) = definition
            .get(key)
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
        {
            children.push(collection_node(&id, key, plural, singular, icon, items));
        }
    }

    if let Some(entities) = definition
        .pointer("/server/ambientNpcs/entities")
        .and_then(Value::as_array)
        .filter(|entities| !entities.is_empty())
    {
        children.push(collection_node(
            &id,
            "characters",
            "Characters",
            "Character",
            Icon::Character,
            entities,
        ));
    }

    SceneNode {
        id,
        label: humanize_identifier(world_id),
        kind: "World",
        icon: Icon::World,
        detail: active.then(|| "Start".to_owned()),
        properties,
        children,
    }
}

fn collection_node(
    parent_id: &str,
    key: &str,
    plural: &str,
    singular: &'static str,
    icon: Icon,
    items: &[Value],
) -> SceneNode {
    let id = format!("{parent_id}/{key}");
    let children = items
        .iter()
        .enumerate()
        .map(|(index, item)| SceneNode {
            id: format!("{id}/{index}"),
            label: item_label(item, singular, index),
            kind: singular,
            icon,
            detail: None,
            properties: object_properties(item),
            children: Vec::new(),
        })
        .collect();
    SceneNode {
        id,
        label: plural.to_owned(),
        kind: "Collection",
        icon: Icon::Folder,
        detail: Some(items.len().to_string()),
        properties: Vec::new(),
        children,
    }
}

fn item_label(item: &Value, fallback: &str, index: usize) -> String {
    for key in ["label", "name", "username", "id", "text", "image"] {
        if let Some(value) = item.get(key).and_then(Value::as_str) {
            let value = value.trim();
            if !value.is_empty() {
                return if key == "id" {
                    humanize_identifier(value)
                } else {
                    value.to_owned()
                };
            }
        }
    }
    format!("{fallback} {}", index + 1)
}

fn object_properties(value: &Value) -> Vec<(String, String)> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            compact_value(value).map(|value| (humanize_identifier(key), value))
        })
        .collect()
}

fn vector_property(node: &SceneNode, label: &str) -> Option<[f32; 3]> {
    node.properties
        .iter()
        .find(|(name, _)| name == label)
        .and_then(|(_, value)| {
            value
                .split(',')
                .map(|component| component.trim().parse::<f32>().ok())
                .collect::<Option<Vec<_>>>()
        })
        .and_then(|values| values.try_into().ok())
}

fn vector_editor(ui: &mut egui::Ui, label: &str, values: &mut [f32; 3], speed: f32) -> bool {
    let colors = palette(ui);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(
            [72.0, CONTROL_HEIGHT],
            egui::Label::new(
                RichText::new(label)
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
            ),
        );
        for (axis, value) in values.iter_mut().enumerate() {
            changed |= ui
                .add(
                    egui::DragValue::new(value)
                        .speed(speed)
                        .prefix(["X ", "Y ", "Z "][axis])
                        .min_decimals(1),
                )
                .changed();
        }
    });
    changed
}

fn avatar_properties(value: &Value) -> Vec<(String, String)> {
    let mut properties = object_properties(value);
    if let Some(character) = value.get("character").and_then(Value::as_object) {
        for key in ["body", "base", "face", "outfit", "parts", "revision"] {
            if let Some(value) = character.get(key).and_then(compact_value) {
                properties.push((humanize_identifier(key), value));
            }
        }
    }
    properties
}

fn manifest_assets(manifest: &Value) -> Vec<ManifestAsset> {
    let mut assets = BTreeMap::<(String, String), ManifestAsset>::new();
    if let Some(groups) = manifest.get("assets").and_then(Value::as_object) {
        for (group, definitions) in groups {
            let Some(definitions) = definitions.as_object() else {
                continue;
            };
            let (kind, icon) = match group.as_str() {
                "images" => ("IMAGE", Icon::Image),
                "audio" => ("AUDIO", Icon::Object),
                "models" => ("MODEL", Icon::Object),
                "characters" | "morphs" => ("CHARACTER", Icon::Character),
                _ => ("ASSET", Icon::Assets),
            };
            for name in definitions.keys() {
                assets.insert(
                    (kind.to_owned(), name.clone()),
                    ManifestAsset {
                        name: name.clone(),
                        kind,
                        icon,
                    },
                );
            }
        }
    }

    for definition in std::iter::once(manifest).chain(
        manifest
            .get("worlds")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|worlds| worlds.values()),
    ) {
        if let Some(materials) = definition.get("materials").and_then(Value::as_object) {
            for name in materials.keys() {
                assets.insert(
                    ("MATERIAL".to_owned(), name.clone()),
                    ManifestAsset {
                        name: name.clone(),
                        kind: "MATERIAL",
                        icon: Icon::Material,
                    },
                );
            }
        }
    }

    assets.into_values().collect()
}

fn compact_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(if *value { "Yes" } else { "No" }.to_owned()),
        Value::Array(values)
            if values
                .iter()
                .all(|value| !value.is_array() && !value.is_object()) =>
        {
            Some(
                values
                    .iter()
                    .filter_map(compact_value)
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
        _ => None,
    }
}

fn humanize_identifier(value: &str) -> String {
    let mut output = String::new();
    let mut previous_lowercase = false;
    for character in value.chars() {
        if character == '-' || character == '_' {
            output.push(' ');
            previous_lowercase = false;
        } else {
            if character.is_uppercase() && previous_lowercase {
                output.push(' ');
            }
            output.push(character);
            previous_lowercase = character.is_lowercase() || character.is_ascii_digit();
        }
    }
    output
        .split_whitespace()
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().chain(characters).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
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
    RevealProject,
    Copy,
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

    fn label(self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Thinking => "Thinking",
            Self::Editing => "Editing the game",
            Self::Checking => "Checking the project",
            Self::Working => "Working",
            Self::Cancelling => "Stopping",
            Self::Rebuilding => "Rebuilding preview",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Thinking => "Working out the requested game change",
            Self::Editing => "Applying the requested change",
            Self::Checking => "Running a project command",
            Self::Working => "Completing the requested change",
            Self::Cancelling => "Waiting for Codex to stop safely",
            Self::Rebuilding => "Validating the change and restarting the game",
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
    UpdateBlock {
        target: String,
        position: [f32; 3],
        size: [f32; 3],
    },
    UpdateSignText {
        target: String,
        text: String,
    },
    DuplicateBlock {
        target: String,
    },
    DeleteBlock {
        target: String,
    },
}

pub(crate) struct StudioShell {
    context: egui::Context,
    state: EguiState,
    renderer: EguiRenderer,
    workspace: Workspace,
    runtime_viewport: Rect,
    scene_outline: SceneOutline,
    expanded_scene: BTreeSet<String>,
    selected_scene: String,
    scene_editor_target: String,
    scene_editor_position: [f32; 3],
    scene_editor_size: [f32; 3],
    scene_editor_text: String,
    selected_world_asset: String,
    selected_asset: &'static str,
    test_tool: &'static str,
    asset_filter: &'static str,
    playing: bool,
    project_editable: bool,
    project_dirty: bool,
    project_error: Option<String>,
    scene_edit_requested: Option<SceneEditRequest>,
    save_requested: bool,
    rebuild_and_play_requested: bool,
    restart_requested: bool,
    notice: String,
    search_query: String,
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
    codex_cancel_requested: bool,
    codex_cancel_sent: bool,
    codex_chat_error: Option<String>,
    codex_change_files: Option<Vec<String>>,
    codex_source_change_count: usize,
    codex_change_review_open: bool,
    codex_undo_requested: bool,
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
    roughness: f32,
    pending_textures_delta: egui::TexturesDelta,
}
