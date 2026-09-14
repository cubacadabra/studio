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
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor, wgpu};
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
#[cfg(test)]
#[path = "shell/morph_ui_tests.rs"]
mod morph_ui_tests;
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

#[derive(Clone, Debug)]
struct SceneNode {
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

impl StudioShell {
    pub(crate) fn new(
        window: &Window,
        game_renderer: &GameRenderer,
        manifest_source: &str,
    ) -> Self {
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
        #[cfg(target_os = "macos")]
        install_system_icon_textures(&context);
        Self::from_egui_with_manifest(context, state, renderer, manifest_source)
    }

    #[cfg(test)]
    fn from_egui(context: egui::Context, state: EguiState, renderer: EguiRenderer) -> Self {
        Self::from_egui_with_manifest(context, state, renderer, "{}")
    }

    fn from_egui_with_manifest(
        context: egui::Context,
        state: EguiState,
        renderer: EguiRenderer,
        manifest_source: &str,
    ) -> Self {
        let logo_texture = load_logo_texture(&context);
        let scene_outline =
            SceneOutline::parse(manifest_source).unwrap_or_else(|_| SceneOutline::empty());
        let selected_scene = scene_outline.initial_selection.clone();
        let expanded_scene = scene_outline.initial_expanded.clone();
        let selected_world_asset = scene_outline
            .assets
            .first()
            .map(|asset| asset.name.clone())
            .unwrap_or_default();
        Self {
            context,
            state,
            renderer,
            workspace: Workspace::default(),
            runtime_viewport: Rect::NOTHING,
            scene_outline,
            expanded_scene,
            selected_scene,
            scene_editor_target: String::new(),
            scene_editor_position: [0.0; 3],
            scene_editor_size: [1.0; 3],
            scene_editor_text: String::new(),
            selected_world_asset,
            selected_asset: "forest-grass",
            test_tool: "Sessions",
            asset_filter: "All",
            // Studio historically launched directly into its live runtime.
            // Keep that behavior now that the shell has a Play/Stop toggle so
            // keyboard and engine-owned pointer controls work immediately.
            playing: true,
            project_editable: false,
            project_dirty: false,
            project_error: None,
            scene_edit_requested: None,
            save_requested: false,
            rebuild_and_play_requested: false,
            restart_requested: false,
            notice: "Ready".to_owned(),
            search_query: String::new(),
            morph_query: String::new(),
            morph_catalog: parse_catalog(include_str!(
                "../../rust/assets/characters/morph_catalog.json"
            ))
            .expect("bundled morph catalog must be valid"),
            morph_artifacts: BTreeMap::new(),
            morph_request: None,
            morph_starters: true,
            morph_loading: false,
            morph_catalog_ready: false,
            morph_catalog_error: None,
            morph_thumbnails: BTreeMap::new(),
            active_loadout: crate::default_morph_loadout(),
            active_morphs: BTreeSet::new(),
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
            morph_attachment_translation: [0.0; 3],
            morph_attachment_rotation: [0.0, 0.0, 0.0, 1.0],
            morph_attachment_scale: [1.0; 3],
            morph_lod_nodes: [String::new(), String::new(), String::new()],
            morph_draft_status: None,
            morph_draft_export_requested: false,
            morph_sidecar_export_requested: false,
            morph_project_add_requested: false,
            morph_pack_import_requested: false,
            morph_publish_requested: false,
            morph_thumbnail_requested: false,
            morph_import_error: None,
            project_asset_available: true,
            auth_requested: false,
            auth_pending: false,
            auth_user: None,
            chatgpt_auth_requested: false,
            chatgpt_pending: false,
            chatgpt_available: true,
            chatgpt_account: None,
            chatgpt_error: None,
            codex_project_root: PathBuf::new(),
            codex_chat_open: false,
            codex_chat_open_requested: false,
            codex_chat_send_requested: None,
            codex_chat_messages: Vec::new(),
            codex_chat_draft: String::new(),
            codex_chat_model: CODEX_CHAT_CURRENT_MODEL,
            codex_chat_reasoning_effort: CODEX_CHAT_DEFAULT_EFFORT,
            codex_chat_ready: false,
            codex_activity: CodexActivity::Idle,
            codex_cancel_requested: false,
            codex_cancel_sent: false,
            codex_chat_error: None,
            codex_change_files: None,
            codex_source_change_count: 0,
            codex_change_review_open: false,
            codex_undo_requested: false,
            open_project_requested: false,
            project_loading: None,
            new_project_dialog_open: false,
            new_project_title: String::new(),
            new_project_parent: PathBuf::from("."),
            #[cfg(not(target_os = "macos"))]
            new_project_folder_requested: false,
            #[cfg(not(target_os = "macos"))]
            new_project_create_requested: false,
            new_project_error: None,
            #[cfg(not(target_os = "macos"))]
            new_project_title_focus_requested: false,
            logo_texture,
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

    pub(crate) fn is_morphs_workspace(&self) -> bool {
        self.workspace == Workspace::Morphs
    }

    pub(crate) fn set_notice(&mut self, notice: String) {
        self.notice = notice;
    }

    pub(crate) fn set_project_editable(&mut self, editable: bool) {
        self.project_editable = editable;
    }

    pub(crate) fn project_is_editable(&self) -> bool {
        self.project_editable
    }

    pub(crate) fn project_is_dirty(&self) -> bool {
        self.project_dirty
    }

    pub(crate) fn set_source_manifest(&mut self, source: &str, dirty: bool) {
        let Ok(outline) = SceneOutline::parse(source) else {
            return;
        };
        let selected = self
            .scene_outline
            .root
            .find(&self.selected_scene)
            .is_some_and(|_| outline.root.find(&self.selected_scene).is_some())
            .then(|| self.selected_scene.clone())
            .unwrap_or_else(|| outline.initial_selection.clone());
        self.scene_outline = outline;
        self.selected_scene = selected;
        self.project_dirty = dirty;
        self.scene_editor_target.clear();
        self.scene_editor_text.clear();
    }

    pub(crate) fn set_project_error(&mut self, message: String) {
        self.project_error = Some(message.clone());
        self.notice = message;
    }

    pub(crate) fn finish_project_loading(&mut self) {
        self.project_loading = None;
        self.project_error = None;
        self.playing = true;
    }

    pub(crate) fn begin_game_rebuild(&mut self) {
        if self.project_loading.is_some() {
            return;
        }
        self.project_loading = Some(ProjectLoadingState {
            progress: 0.0,
            previous_outline: self.scene_outline.clone(),
            previous_expanded: self.expanded_scene.clone(),
            previous_selection: self.selected_scene.clone(),
            previous_world_asset: self.selected_world_asset.clone(),
            previous_workspace: self.workspace,
        });
        self.project_error = None;
        self.notice = "Rebuilding preview…".to_owned();
    }

    pub(crate) fn take_scene_edit_request(&mut self) -> Option<SceneEditRequest> {
        self.scene_edit_requested.take()
    }

    pub(crate) fn take_save_request(&mut self) -> bool {
        std::mem::take(&mut self.save_requested)
    }

    pub(crate) fn take_rebuild_and_play_request(&mut self) -> bool {
        std::mem::take(&mut self.rebuild_and_play_requested)
    }

    pub(crate) fn take_restart_request(&mut self) -> bool {
        std::mem::take(&mut self.restart_requested)
    }

    pub(crate) fn take_auth_request(&mut self) -> bool {
        std::mem::take(&mut self.auth_requested)
    }

    pub(crate) fn set_auth_pending(&mut self, pending: bool) {
        self.auth_pending = pending;
    }

    pub(crate) fn set_auth_completed(&mut self, user: crate::network::AuthUser) {
        self.auth_pending = false;
        self.auth_user = Some(user.clone());
        self.notice = format!("Signed in as {}", user.name);
    }

    pub(crate) fn set_auth_error(&mut self, message: String) {
        self.auth_pending = false;
        self.notice = message;
    }

    pub(crate) fn take_chatgpt_auth_request(&mut self) -> bool {
        std::mem::take(&mut self.chatgpt_auth_requested)
    }

    pub(crate) fn set_codex_project_root(&mut self, project_root: PathBuf) {
        self.codex_project_root = project_root;
    }

    pub(crate) fn take_codex_chat_open_request(&mut self) -> bool {
        std::mem::take(&mut self.codex_chat_open_requested)
    }

    pub(crate) fn take_codex_chat_send_request(&mut self) -> Option<CodexChatSendRequest> {
        self.codex_chat_send_requested.take()
    }

    pub(crate) fn set_codex_chat_ready(&mut self) {
        self.codex_chat_ready = true;
        self.codex_chat_error = None;
    }

    pub(crate) fn set_codex_chat_delta(&mut self, _delta: String) {
        self.codex_activity = self.codex_activity.after_agent_progress();
    }

    pub(crate) fn set_codex_work_status(&mut self, status: CodexWorkStatus) {
        if !self.codex_activity.is_cancellable() {
            return;
        }
        self.codex_activity = match status {
            CodexWorkStatus::Thinking => CodexActivity::Thinking,
            CodexWorkStatus::Editing => CodexActivity::Editing,
            CodexWorkStatus::Checking => CodexActivity::Checking,
            CodexWorkStatus::Working => CodexActivity::Working,
        };
    }

    pub(crate) fn set_codex_chat_message(&mut self, _text: String) {
        // An agent-message item can complete while the turn continues with
        // more tool work. Only turn/completed advances Studio to rebuilding.
        self.codex_activity = self.codex_activity.after_agent_progress();
    }

    pub(crate) fn set_codex_chat_completed(&mut self) {
        self.codex_activity = CodexActivity::Rebuilding;
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
    }

    pub(crate) fn set_codex_preview_rebuilt(&mut self) {
        self.finish_codex_activity("Done — preview rebuilt and playing.");
    }

    pub(crate) fn set_codex_preview_rebuild_failed(&mut self, message: &str) {
        self.finish_codex_activity("The change was made, but the preview could not be rebuilt.");
        self.codex_chat_error = Some(format!("Rebuild failed: {message}"));
    }

    pub(crate) fn set_codex_chat_error(&mut self, message: String) {
        self.finish_codex_activity("The request could not be completed.");
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
        self.codex_chat_error = Some(message);
    }

    pub(crate) fn set_codex_chat_cancelling(&mut self) {
        self.codex_cancel_requested = true;
        self.codex_activity = CodexActivity::Cancelling;
        self.notice = "Stopping Codex…".to_owned();
    }

    pub(crate) fn set_codex_chat_cancelled(&mut self) {
        self.finish_codex_activity("Request cancelled. The preview was not rebuilt.");
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
    }

    fn finish_codex_activity(&mut self, message: &str) {
        let was_active = self.codex_activity.is_active();
        self.codex_activity = CodexActivity::Idle;
        if was_active {
            self.codex_chat_messages.push(CodexChatMessage {
                role: CodexChatRole::Assistant,
                text: message.to_owned(),
            });
        }
    }

    pub(crate) fn set_codex_changes(&mut self, files: Vec<String>) {
        let source_change_count = files.iter().filter(|file| file.ends_with(".luau")).count();
        let visible_files = files
            .into_iter()
            .filter(|file| !file.ends_with(".luau"))
            .collect::<Vec<_>>();
        self.codex_source_change_count = source_change_count;
        self.codex_change_files = (!visible_files.is_empty()).then_some(visible_files);
        self.codex_change_review_open = false;
    }

    pub(crate) fn clear_codex_changes(&mut self) {
        self.codex_change_files = None;
        self.codex_source_change_count = 0;
        self.codex_change_review_open = false;
    }

    pub(crate) fn take_codex_undo_request(&mut self) -> bool {
        std::mem::take(&mut self.codex_undo_requested)
    }

    pub(crate) fn take_codex_cancel_request(&mut self) -> bool {
        if !self.codex_activity.is_active()
            || !self.codex_cancel_requested
            || self.codex_cancel_sent
        {
            return false;
        }
        self.codex_cancel_sent = true;
        true
    }

    pub(crate) fn set_chatgpt_pending(&mut self) {
        self.chatgpt_pending = true;
        self.chatgpt_available = true;
        self.chatgpt_error = None;
        self.notice = "Opening browser for ChatGPT sign-in…".to_owned();
    }

    pub(crate) fn set_chatgpt_account(&mut self, account: Option<ChatGptAccount>) {
        if self.chatgpt_pending {
            return;
        }
        self.chatgpt_available = true;
        self.chatgpt_account = account;
        self.chatgpt_error = None;
    }

    pub(crate) fn set_chatgpt_browser_opened(&mut self) {
        self.chatgpt_pending = true;
        self.notice =
            "Finish signing in with ChatGPT in your browser. Studio will continue automatically."
                .to_owned();
    }

    pub(crate) fn set_chatgpt_connected(&mut self, account: ChatGptAccount) {
        self.chatgpt_pending = false;
        self.chatgpt_available = true;
        self.chatgpt_error = None;
        self.notice = account
            .email
            .as_deref()
            .map(|email| format!("ChatGPT connected as {email}"))
            .unwrap_or_else(|| "ChatGPT connected".to_owned());
        self.chatgpt_account = Some(account);
    }

    pub(crate) fn set_chatgpt_error(&mut self, message: String) {
        self.chatgpt_pending = false;
        self.chatgpt_available = true;
        self.chatgpt_error = Some(message.clone());
        self.notice = message;
    }

    pub(crate) fn set_chatgpt_unavailable(&mut self, message: String) {
        let was_pending = self.chatgpt_pending;
        self.chatgpt_pending = false;
        self.chatgpt_available = false;
        self.chatgpt_account = None;
        self.chatgpt_error = Some(message.clone());
        if was_pending {
            self.notice = message;
        }
    }

    pub(crate) fn set_project_asset_available(&mut self, available: bool) {
        self.project_asset_available = available;
    }

    pub(crate) fn take_open_project_request(&mut self) -> bool {
        std::mem::take(&mut self.open_project_requested)
    }

    pub(crate) fn begin_project_loading(&mut self) {
        if self.project_loading.is_some() {
            return;
        }
        let empty = SceneOutline::empty();
        let empty_selection = empty.initial_selection.clone();
        let empty_expanded = empty.initial_expanded.clone();
        self.project_loading = Some(ProjectLoadingState {
            progress: 0.0,
            previous_outline: std::mem::replace(&mut self.scene_outline, empty),
            previous_expanded: std::mem::replace(&mut self.expanded_scene, empty_expanded),
            previous_selection: std::mem::replace(&mut self.selected_scene, empty_selection),
            previous_world_asset: std::mem::take(&mut self.selected_world_asset),
            previous_workspace: std::mem::replace(&mut self.workspace, Workspace::World),
        });
        self.notice = "Loading project…".to_owned();
    }

    pub(crate) fn set_project_loading_progress(&mut self, progress: f32) {
        if let Some(loading) = &mut self.project_loading {
            loading.progress = loading.progress.max(progress.clamp(0.0, 1.0));
        }
    }

    pub(crate) fn cancel_project_loading(&mut self) {
        let Some(loading) = self.project_loading.take() else {
            return;
        };
        self.scene_outline = loading.previous_outline;
        self.expanded_scene = loading.previous_expanded;
        self.selected_scene = loading.previous_selection;
        self.selected_world_asset = loading.previous_world_asset;
        self.workspace = loading.previous_workspace;
    }

    pub(crate) fn is_project_loading(&self) -> bool {
        self.project_loading.is_some()
    }

    pub(crate) fn set_new_project_parent(&mut self, parent: PathBuf) {
        self.new_project_parent = parent;
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn take_native_new_project_dialog(
        &mut self,
    ) -> Option<(String, PathBuf, Option<String>)> {
        if !std::mem::take(&mut self.new_project_dialog_open) {
            return None;
        }
        Some((
            self.new_project_title.clone(),
            self.new_project_parent.clone(),
            self.new_project_error.take(),
        ))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_new_project_draft(&mut self, title: String, parent: PathBuf) {
        self.new_project_title = title;
        self.new_project_parent = parent;
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn take_new_project_folder_request(&mut self) -> bool {
        std::mem::take(&mut self.new_project_folder_requested)
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn take_new_project_request(&mut self) -> Option<(String, PathBuf)> {
        if !std::mem::take(&mut self.new_project_create_requested) {
            return None;
        }
        Some((
            self.new_project_title.trim().to_owned(),
            self.new_project_parent.clone(),
        ))
    }

    pub(crate) fn set_new_project_error(&mut self, message: String) {
        self.new_project_dialog_open = true;
        self.new_project_error = Some(message);
        #[cfg(not(target_os = "macos"))]
        {
            self.new_project_title_focus_requested = true;
        }
    }

    pub(crate) fn set_new_project_created(&mut self, project: &Path) {
        self.new_project_dialog_open = false;
        self.new_project_error = None;
        self.notice = format!("Created {}", project.display());
    }

    pub(crate) fn set_remote_morph_catalog(&mut self, source: &str) -> Result<usize, String> {
        let published = crate::wardrobe::PublishedCatalog::parse(source)?;
        let count = published.catalog.assets.len();
        self.morph_catalog = published.catalog;
        self.morph_artifacts = published.artifacts;
        self.morph_catalog_ready = true;
        self.morph_catalog_error = None;
        Ok(count)
    }

    pub(crate) fn set_local_morph_catalog(&mut self, catalog: MorphCatalog) {
        self.morph_catalog = catalog;
        self.morph_artifacts.clear();
        self.morph_catalog_ready = true;
        self.morph_catalog_error = None;
    }

    pub(crate) fn morph_artifact(&self, id: &MorphAssetId) -> Option<&crate::wardrobe::Artifact> {
        self.morph_artifacts.get(id)
    }

    pub(crate) fn set_catalog_error(&mut self, message: String) {
        self.morph_catalog_error = Some(message);
    }

    pub(crate) fn set_morph_thumbnail(&mut self, url: String, bytes: &[u8]) {
        if let Ok(image) = image::load_from_memory(bytes) {
            if image.width() > 1024 || image.height() > 1024 {
                return;
            }
            let rgba = image.to_rgba8();
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            );
            let texture = self
                .context
                .load_texture(&url, image, egui::TextureOptions::LINEAR);
            self.morph_thumbnails.insert(url, texture);
        }
    }

    pub(crate) fn set_morph_loading(&mut self, loading: bool) {
        self.morph_loading = loading;
    }

    pub(crate) fn select_morph(&mut self, id: MorphAssetId) {
        self.selected_morph = id;
    }

    pub(crate) fn upsert_morph_asset(
        &mut self,
        definition: cubacadabra_morphs::MorphAssetDefinition,
    ) {
        self.morph_catalog
            .assets
            .retain(|asset| asset.id != definition.id);
        self.morph_catalog.assets.push(definition);
    }

    pub(crate) fn morph_catalog(&self) -> &MorphCatalog {
        &self.morph_catalog
    }

    pub(crate) fn take_morph_request(&mut self) -> Option<crate::wardrobe::Request> {
        self.morph_request.take()
    }

    pub(crate) fn set_active_morph_loadout(&mut self, loadout: &cubacadabra_morphs::MorphLoadout) {
        self.active_morphs = crate::wardrobe::selected_ids(loadout)
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        self.active_loadout = loadout.clone();
    }

    pub(crate) fn take_morph_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_import_requested)
    }

    pub(crate) fn take_morph_sidecar_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_sidecar_import_requested)
    }

    pub(crate) fn take_morph_project_add_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_project_add_requested)
    }

    pub(crate) fn take_morph_pack_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_pack_import_requested)
    }

    pub(crate) fn take_morph_sidecar_export_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_sidecar_export_requested)
    }

    pub(crate) fn take_morph_draft_export_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_draft_export_requested)
    }

    pub(crate) fn take_morph_publish_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_publish_requested)
    }

    pub(crate) fn take_morph_thumbnail_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_thumbnail_requested)
    }

    fn morph_attachment(&self) -> MorphAttachment {
        MorphAttachment {
            mode: MorphAttachmentMode::Rigid,
            joint: self.morph_attachment_joint.trim().to_owned(),
            translation: self.morph_attachment_translation,
            rotation: self.morph_attachment_rotation,
            scale: self.morph_attachment_scale,
        }
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
            self.morph_attachment(),
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

    pub(crate) fn morph_project_payload(
        &self,
    ) -> Result<(String, String, MorphGlbPreviewMesh), String> {
        let path = self
            .morph_preview_path
            .clone()
            .ok_or_else(|| "Import a GLB before adding it to this game.".to_owned())?;
        let (_, manifest_json) = self.morph_sidecar_payload()?;
        let preview = self
            .morph_preview
            .clone()
            .ok_or_else(|| "The imported GLB has no preview mesh to add.".to_owned())?;
        Ok((path, manifest_json, preview))
    }

    pub(crate) fn morph_draft_payload(&self) -> Result<(String, String), String> {
        let path = self
            .morph_preview_path
            .as_deref()
            .ok_or_else(|| "Import a GLB before saving its draft.".to_owned())?;
        let asset = self
            .morph_draft_asset
            .as_ref()
            .or_else(|| self.morph_catalog.asset(&self.selected_morph))
            .ok_or_else(|| "Select a catalog asset before saving its draft.".to_owned())?;
        let geometry_file = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "The imported GLB needs a safe filename for its draft.".to_owned())?
            .to_owned();
        let summary = self
            .morph_source_summary
            .as_ref()
            .ok_or_else(|| "The imported GLB has no source summary to save.".to_owned())?;
        let fallback_triangle_count = self
            .morph_preview
            .as_ref()
            .map(|preview| preview.indices.len() / 3)
            .and_then(|count| u32::try_from(count).ok())
            .ok_or_else(|| "The imported GLB has no preview triangle count.".to_owned())?;
        let lod_nodes = [
            self.morph_lod_nodes[0].trim(),
            self.morph_lod_nodes[1].trim(),
            self.morph_lod_nodes[2].trim(),
        ];
        let triangle_counts = lod_nodes.map(|node| {
            summary
                .node_triangle_counts
                .get(node)
                .copied()
                .unwrap_or_else(|| {
                    if node.is_empty() {
                        0
                    } else {
                        fallback_triangle_count
                    }
                })
        });
        let json = build_morph_draft_json(
            asset,
            geometry_file,
            self.morph_attachment(),
            lod_nodes,
            triangle_counts,
        )?;
        let suggested_name = format!(
            "{}.morph.draft.json",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, json))
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

    pub(crate) fn set_morph_draft_export_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    false,
                    "Draft saved. Complete the LOD mapping before exporting or publishing."
                        .to_owned(),
                ));
                self.notice = "Morph draft saved".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_publish_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status =
                    Some((true, format!("Published {asset_id} ({byte_len} bytes).")));
                self.notice = "Morph pack published".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_project_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status = Some((
                    true,
                    format!("Added {asset_id} to this game ({byte_len} bytes)."),
                ));
                self.notice = "Character asset added to game".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_runtime_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status = Some((
                    true,
                    format!("Loaded {asset_id} for the live player ({byte_len} bytes)."),
                ));
                self.notice = "Morph pack loaded".to_owned();
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
        self.morph_attachment_translation = [0.0; 3];
        self.morph_attachment_rotation = [0.0, 0.0, 0.0, 1.0];
        self.morph_attachment_scale = [1.0; 3];
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

    pub(crate) fn set_morph_lod_preview(&mut self, level: usize, preview: MorphGlbPreviewMesh) {
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
        let attachment = manifest.attachment.clone();
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
        self.morph_attachment_joint = attachment.joint;
        self.morph_attachment_translation = attachment.translation;
        self.morph_attachment_rotation = attachment.rotation;
        self.morph_attachment_scale = attachment.scale;
        self.morph_lod_nodes = lod_nodes;
        self.morph_draft_status = Some((
            true,
            "Sidecar reimported and GLB contract validated.".to_owned(),
        ));
        self.notice = "Morph sidecar reimported".to_owned();
    }

    pub(crate) fn set_morph_draft_preview(
        &mut self,
        glb_path: String,
        draft: MorphDraftDocument,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        self.set_morph_preview(glb_path, preview, summary);
        self.morph_draft_asset = Some(draft.asset);
        self.morph_attachment_joint = draft.attachment.joint;
        self.morph_attachment_translation = draft.attachment.translation;
        self.morph_attachment_rotation = draft.attachment.rotation;
        self.morph_attachment_scale = draft.attachment.scale;
        self.morph_lod_nodes = draft.lod_nodes;
        self.validate_morph_draft();
        self.notice = "Morph draft reimported".to_owned();
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
        if !self
            .morph_attachment_translation
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 10.0)
        {
            issues.push("Attachment offset must stay within +/-10 units.".to_owned());
        }
        if !self
            .morph_attachment_scale
            .iter()
            .all(|value| value.is_finite() && (0.01..=100.0).contains(value))
        {
            issues.push("Attachment scale must stay within 0.01–100.".to_owned());
        }
        let rotation_length = self
            .morph_attachment_rotation
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if !rotation_length.is_finite() || !(0.99..=1.01).contains(&rotation_length) {
            issues.push("Attachment rotation must be a normalized quaternion.".to_owned());
        }
        if self
            .morph_draft_asset
            .as_ref()
            .is_some_and(|asset| asset.kind == MorphAssetKind::Headwear)
            && let Some(mesh) = self
                .morph_lod_previews
                .first()
                .and_then(Option::as_ref)
                .or(self.morph_preview.as_ref())
            && let Some((minimum, maximum)) = morph_mesh_bounds(mesh)
        {
            let runtime_width = ((maximum[0] - minimum[0]) * self.morph_attachment_scale[0].abs())
                .max((maximum[2] - minimum[2]) * self.morph_attachment_scale[2].abs());
            if !(0.25..=2.20).contains(&runtime_width) {
                issues.push(format!(
                    "Headwear runtime width is {runtime_width:.2} units; use Fit to person head or adjust attachment scale."
                ));
            }
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

    fn fit_current_morph_to_person(&mut self) {
        let result = self
            .morph_lod_previews
            .first()
            .and_then(Option::as_ref)
            .or(self.morph_preview.as_ref())
            .ok_or_else(|| "Import a headwear mesh before fitting it.".to_owned())
            .and_then(|mesh| {
                fit_rigid_headwear_to_person(mesh, self.morph_attachment_joint.trim())
            });
        match result {
            Ok(attachment) => {
                self.morph_attachment_translation = attachment.translation;
                self.morph_attachment_rotation = attachment.rotation;
                self.morph_attachment_scale = attachment.scale;
                self.morph_draft_status = Some((
                    true,
                    format!(
                        "Fit to person head at {:.3}× scale. Validate and republish the pack.",
                        attachment.scale[0]
                    ),
                ));
                self.notice = "Headwear attachment fitted".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn execute_command(&mut self, command: StudioCommand) {
        if self.project_loading.is_some() {
            return;
        }
        match command {
            StudioCommand::NewProject => {
                self.new_project_dialog_open = true;
                self.new_project_title.clear();
                self.new_project_error = None;
                #[cfg(not(target_os = "macos"))]
                {
                    self.new_project_title_focus_requested = true;
                }
            }
            StudioCommand::OpenProject => {
                self.open_project_requested = true;
                self.notice = "Choose a project folder…".to_owned();
            }
            StudioCommand::Save => {
                if self.project_editable {
                    self.save_requested = true;
                    self.notice = "Saving project…".to_owned();
                } else {
                    self.notice = "This preview is read-only".to_owned();
                }
            }
            StudioCommand::RevealProject => {
                self.notice = "Reveal Project is not connected yet".to_owned();
            }
            StudioCommand::Copy => {
                self.state.egui_input_mut().events.push(egui::Event::Copy);
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
        if self.codex_chat_open {
            self.show_codex_chat(ui);
        }
        match self.workspace {
            Workspace::World => self.show_world(ui),
            Workspace::Assets => self.show_assets(ui),
            Workspace::Materials => self.show_materials(ui),
            Workspace::Morphs => self.show_morphs(ui),
            Workspace::Test => self.show_test(ui),
        }
        #[cfg(not(target_os = "macos"))]
        self.show_new_project_dialog(ui.ctx());
        self.show_project_loading(ui.ctx());
        self.show_project_error(ui.ctx());
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    fn show_project_loading(&self, context: &egui::Context) {
        let Some(loading) = &self.project_loading else {
            return;
        };
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        egui::Modal::new(egui::Id::new("project_loading"))
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.border_strong))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(20)),
            )
            .show(context, |ui| {
                ui.set_width(280.0);
                ui.label(
                    RichText::new("Loading...")
                        .font(semibold_font(TYPE.primary))
                        .color(colors.text),
                );
                ui.add_space(10.0);
                ui.add(
                    egui::ProgressBar::new(loading.progress)
                        .desired_width(ui.available_width())
                        .show_percentage(),
                );
            });
    }

    fn show_project_error(&mut self, context: &egui::Context) {
        let Some(mut error) = self.project_error.clone() else {
            return;
        };
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        egui::Window::new("Preview build failed")
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.axis_x))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(14)),
            )
            .show(context, |ui| {
                ui.label(
                    RichText::new("The last working preview is still running.")
                        .size(TYPE.secondary)
                        .color(colors.text),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::multiline(&mut error)
                        .desired_rows(7)
                        .interactive(false)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                if ui.button("Dismiss").clicked() {
                    self.project_error = None;
                }
            });
    }

    fn show_codex_chat(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::right("codex_chat_panel")
            .resizable(true)
            .default_size(360.0)
            .size_range(280.0..=480.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Codex chat")
                                .font(semibold_font(TYPE.primary))
                                .color(colors.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .button(RichText::new("×").size(TYPE.primary))
                                .on_hover_text("Close Codex chat")
                                .clicked()
                            {
                                self.codex_chat_open = false;
                            }
                        });
                    });
                    let project_root = self.codex_project_root.display().to_string();
                    ui.label(
                        RichText::new(format!("Working in {project_root}"))
                            .size(TYPE.meta)
                            .color(colors.muted),
                    )
                    .on_hover_text(project_root);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Model").size(TYPE.meta).color(colors.muted));
                        let selected_model = self.codex_chat_model;
                        egui::ComboBox::from_id_salt("codex_chat_model")
                            .selected_text(codex_chat_model_label(selected_model, selected_model))
                            .width(156.0)
                            .show_ui(ui, |ui| {
                                for &(model, description) in &CODEX_CHAT_MODELS {
                                    ui.selectable_value(
                                        &mut self.codex_chat_model,
                                        model,
                                        codex_chat_model_label(model, selected_model),
                                    )
                                    .on_hover_text(description);
                                }
                            });
                        ui.label(RichText::new("Level").size(TYPE.meta).color(colors.muted));
                        egui::ComboBox::from_id_salt("codex_chat_reasoning_effort")
                            .selected_text(self.codex_chat_reasoning_effort)
                            .width(72.0)
                            .show_ui(ui, |ui| {
                                for &(effort, description) in &CODEX_CHAT_EFFORTS {
                                    ui.selectable_value(
                                        &mut self.codex_chat_reasoning_effort,
                                        effort,
                                        effort,
                                    )
                                    .on_hover_text(description);
                                }
                            });
                    });
                    ui.label(
                        RichText::new(codex_chat_model_description(self.codex_chat_model))
                            .size(TYPE.meta)
                            .color(colors.muted),
                    );
                    ui.separator();

                    // Keep the composer in the panel's visible region. With
                    // `auto_shrink(false)`, an unconstrained scroll area
                    // consumes all remaining height and lays the composer
                    // out below the panel clip rect.
                    let reserved_chat_controls_height = 150.0;
                    let messages_height =
                        (ui.available_height() - reserved_chat_controls_height).max(64.0);
                    egui::ScrollArea::vertical()
                        .id_salt("codex_chat_messages")
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .max_height(messages_height)
                        .show(ui, |ui| {
                            if self.codex_chat_messages.is_empty() {
                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new(
                                        "Describe a code change. Codex applies it, then Studio rebuilds the preview.",
                                    )
                                    .size(TYPE.secondary)
                                    .color(colors.secondary_text),
                                );
                            }
                            for message in &self.codex_chat_messages {
                                let (label, color) = match message.role {
                                    CodexChatRole::User => ("You", colors.accent),
                                    CodexChatRole::Assistant => ("Codex", colors.secondary_text),
                                };
                                ui.label(
                                    RichText::new(label)
                                        .font(semibold_font(TYPE.meta))
                                        .color(color),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&message.text)
                                            .size(TYPE.secondary)
                                            .color(colors.text),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(12.0);
                            }
                            if self.codex_activity.is_active() {
                                ui.label(
                                    RichText::new("Codex")
                                        .font(semibold_font(TYPE.meta))
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Spinner::new()
                                            .size(16.0)
                                            .color(colors.accent),
                                    );
                                    ui.label(
                                        RichText::new(self.codex_activity.label())
                                            .font(semibold_font(TYPE.secondary))
                                            .color(colors.text),
                                    );
                                    if self.codex_activity.is_cancellable()
                                        && ui.small_button("Cancel").clicked()
                                    {
                                        self.codex_cancel_requested = true;
                                        self.codex_activity = CodexActivity::Cancelling;
                                    }
                                });
                                ui.label(
                                    RichText::new(self.codex_activity.detail())
                                        .size(TYPE.meta)
                                        .color(colors.muted),
                                );
                                ui.add_space(12.0);
                                ui.ctx().request_repaint_after(Duration::from_millis(16));
                            }
                            if let Some(error) = &self.codex_chat_error {
                                ui.label(
                                    RichText::new(error).size(TYPE.meta).color(colors.axis_x),
                                );
                                ui.add_space(12.0);
                            }
                        });

                    if !self.codex_chat_ready {
                        ui.label(
                            RichText::new("Starting Codex…")
                                .size(TYPE.meta)
                            .color(colors.muted),
                        );
                    }
                    if self.codex_source_change_count > 0 || self.codex_change_files.is_some() {
                        let files = self.codex_change_files.clone().unwrap_or_default();
                        let changed_count = if self.codex_source_change_count > 0 {
                            self.codex_source_change_count
                        } else {
                            files.len()
                        };
                        let changed_label = if self.codex_source_change_count > 0 {
                            format!(
                                "Codex changed {} source file{}",
                                changed_count,
                                if changed_count == 1 { "" } else { "s" }
                            )
                        } else {
                            format!(
                                "Changed {} file{}",
                                changed_count,
                                if changed_count == 1 { "" } else { "s" }
                            )
                        };
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(changed_label)
                                .font(semibold_font(TYPE.meta))
                                .color(colors.text),
                            );
                            if !files.is_empty()
                                && ui
                                    .button(if self.codex_change_review_open {
                                        "Hide files"
                                    } else {
                                        "Show files"
                                    })
                                    .clicked()
                            {
                                self.codex_change_review_open = !self.codex_change_review_open;
                            }
                            let undo_enabled =
                                self.project_loading.is_none() && !self.codex_activity.is_active();
                            if ui
                                .add_enabled(undo_enabled, egui::Button::new("Undo this change"))
                                .on_disabled_hover_text("Undo is available after the rebuild finishes")
                                .clicked()
                            {
                                self.codex_undo_requested = true;
                            }
                        });
                        if self.codex_change_review_open {
                            Frame::NONE
                                .fill(colors.surface)
                                .inner_margin(Margin::symmetric(8, 5))
                                .show(ui, |ui| {
                                    for file in &files {
                                        ui.label(
                                            RichText::new(file)
                                                .size(TYPE.meta)
                                                .color(colors.secondary_text),
                                        );
                                    }
                                });
                        }
                    }
                    ui.separator();
                    let can_send = self.codex_chat_ready
                        && !self.codex_activity.is_active()
                        && self.chatgpt_account.is_some()
                        && !self.project_dirty
                        && self.project_loading.is_none();
                    ui.add_enabled_ui(can_send, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.codex_chat_draft)
                                .desired_rows(3)
                                .hint_text("Ask Codex to make a change…")
                                .desired_width(f32::INFINITY),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if self.project_dirty {
                                    "Save scene changes before asking Codex to edit code"
                                } else {
                                    "Codex edits code; Studio rebuilds when it finishes"
                                })
                                    .size(TYPE.meta)
                                    .color(colors.muted),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add_sized([68.0, 28.0], egui::Button::new("Send"))
                                    .clicked()
                                {
                                    let message = self.codex_chat_draft.trim().to_owned();
                                    if !message.is_empty() {
                                        self.codex_chat_messages.push(CodexChatMessage {
                                            role: CodexChatRole::User,
                                            text: message.clone(),
                                        });
                                        self.codex_chat_draft.clear();
                                        self.codex_activity = CodexActivity::Thinking;
                                        self.codex_cancel_requested = false;
                                        self.codex_cancel_sent = false;
                                        self.codex_chat_error = None;
                                        self.codex_chat_send_requested =
                                            Some(CodexChatSendRequest {
                                                message,
                                                model: self.codex_chat_model,
                                                reasoning_effort: self.codex_chat_reasoning_effort,
                                            });
                                    }
                                }
                            });
                        });
                    });
                });
            });
    }

    #[cfg(not(target_os = "macos"))]
    fn show_new_project_dialog(&mut self, context: &egui::Context) {
        if !self.new_project_dialog_open {
            return;
        }
        let mut close_requested = false;
        let dialog_width = (context.content_rect().width() - 40.0).clamp(280.0, 440.0);
        let response = egui::Modal::new(egui::Id::new("new_project_dialog"))
            .backdrop_color(Color32::from_black_alpha(128))
            .frame(
                Frame::NONE
                    .fill(if context.style_of(context.theme()).visuals.dark_mode {
                        DARK_PALETTE.panel_raised
                    } else {
                        LIGHT_PALETTE.surface_deep
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if context.style_of(context.theme()).visuals.dark_mode {
                            DARK_PALETTE.border_strong
                        } else {
                            LIGHT_PALETTE.border
                        },
                    ))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(20)),
            )
            .show(context, |ui| {
                let colors = palette(ui);
                ui.set_width(dialog_width);
                ui.label(
                    RichText::new("New Project")
                        .font(semibold_font(18.0))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Create a starter game with its manifest, Luau entry point, and local SDK.",
                    )
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
                );
                ui.add_space(18.0);
                ui.label(
                    RichText::new("Game title")
                        .font(medium_font(TYPE.secondary))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                let title_response = ui.add(
                    egui::TextEdit::singleline(&mut self.new_project_title)
                        .hint_text("The Wild West")
                        .desired_width(ui.available_width())
                        .min_size(egui::vec2(0.0, 28.0))
                        .vertical_align(Align::Center),
                );
                if std::mem::take(&mut self.new_project_title_focus_requested) {
                    title_response.request_focus();
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Location")
                        .font(medium_font(TYPE.secondary))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                Frame::NONE
                    .fill(colors.field)
                    .stroke(Stroke::new(1.0, colors.border))
                    .corner_radius(4.0)
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_sized([78.0, 26.0], egui::Button::new("Choose…"))
                                .clicked()
                            {
                                self.new_project_folder_requested = true;
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.new_project_parent.display().to_string())
                                        .size(TYPE.secondary)
                                        .color(colors.secondary_text),
                                )
                                .truncate(),
                            )
                            .on_hover_text(self.new_project_parent.display().to_string());
                        });
                    });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "A new folder is created from the title. Existing projects are never overwritten.",
                    )
                    .size(TYPE.meta)
                    .color(colors.muted),
                );
                if let Some(error) = &self.new_project_error {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(error)
                            .size(TYPE.secondary)
                            .color(colors.axis_x),
                    );
                }
                ui.add_space(20.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_sized([82.0, 28.0], egui::Button::new("Cancel"))
                        .clicked()
                    {
                        close_requested = true;
                    }
                    let enabled = !self.new_project_title.trim().is_empty();
                    let button_text = if ui.visuals().dark_mode {
                        colors.surface_deep
                    } else {
                        Color32::WHITE
                    };
                    let create = ui.add_enabled_ui(enabled, |ui| {
                        ui.add_sized(
                            [110.0, 28.0],
                            egui::Button::new(
                                RichText::new("Create project")
                                    .font(medium_font(TYPE.secondary))
                                    .color(button_text),
                            )
                            .fill(colors.accent)
                            .stroke(Stroke::NONE)
                            .corner_radius(4.0),
                        )
                    });
                    let enter_pressed = title_response.has_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if create.inner.clicked() || (enabled && enter_pressed) {
                        self.new_project_create_requested = true;
                        close_requested = true;
                    }
                });
            });
        let escape_pressed = response.is_top_modal
            && !response.any_popup_open
            && context
                .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if close_requested || escape_pressed {
            self.new_project_dialog_open = false;
        }
    }

    fn show_top_bar(&mut self, root: &mut egui::Ui, project_name: &str) {
        let colors = palette(root);
        egui::Panel::top("studio_top_bar")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(editor_frame(colors.surface).inner_margin(Margin::symmetric(8, 0)))
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
                            if menu_entry(ui, Icon::Plus, "New Project…", "Ctrl+N", true).clicked()
                            {
                                self.execute_command(StudioCommand::NewProject);
                                ui.close();
                            }
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
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if self.auth_pending {
                            ui.label(
                                RichText::new("Signing in…")
                                    .size(TYPE.meta)
                                    .color(colors.muted),
                            );
                        } else if let Some(user) = &self.auth_user {
                            ui.label(
                                RichText::new(&user.name)
                                    .size(TYPE.meta)
                                    .color(colors.secondary_text),
                            );
                        } else if toolbar_button(ui, Icon::Character, "Sign in", false).clicked() {
                            self.auth_requested = true;
                        }
                        vertical_separator(ui, 14.0);
                        self.show_chatgpt_control(ui, colors);
                        let play_label = if self.playing { "Stop" } else { "Play" };
                        let play_width = toolbar_button_width(ui, play_label);
                        if ui.available_width() >= play_width + 48.0 {
                            let live = ui.allocate_response(
                                egui::vec2(40.0, CONTROL_HEIGHT),
                                Sense::hover(),
                            );
                            paint_status_label(ui, live.rect, colors.live, "Live");
                        }
                        let play_icon = if self.playing { Icon::Stop } else { Icon::Play };
                        if toolbar_button(ui, play_icon, play_label, self.playing).clicked() {
                            if self.playing {
                                self.playing = false;
                                self.notice = "Play session stopped".to_owned();
                            } else {
                                self.playing = true;
                                self.restart_requested = true;
                                self.notice = "Restarting preview…".to_owned();
                            }
                        }
                        if self.project_editable
                            && toolbar_button(ui, Icon::Play, "Rebuild & Play", false).clicked()
                        {
                            self.rebuild_and_play_requested = true;
                            self.notice = "Saving and rebuilding preview…".to_owned();
                        }
                        if self.project_editable
                            && toolbar_button(ui, Icon::Play, "Restart", false).clicked()
                        {
                            self.restart_requested = true;
                            self.notice = "Restarting preview…".to_owned();
                        }
                        let project_width = (ui.available_width() - 17.0).min(180.0);
                        if project_width >= 72.0 {
                            vertical_separator(ui, 14.0);
                            ui.add_sized(
                                [project_width, CONTROL_HEIGHT],
                                egui::Label::new(
                                    RichText::new(project_name)
                                        .size(TYPE.secondary)
                                        .color(colors.secondary_text),
                                )
                                .truncate(),
                            )
                            .on_hover_text(project_name);
                        }
                    });
                });
            });
    }

    fn show_chatgpt_control(&mut self, ui: &mut egui::Ui, colors: Palette) {
        if self.chatgpt_pending {
            toolbar_status(ui, Icon::Sparkles, "Connecting ChatGPT…", colors.muted)
                .on_hover_text("Finish signing in with ChatGPT in your browser");
            return;
        }

        if let Some(account) = &self.chatgpt_account {
            let label = chatgpt_account_label(account);
            let tooltip = account
                .email
                .as_deref()
                .map(|email| format!("ChatGPT connected as {email}"))
                .unwrap_or_else(|| "ChatGPT connected".to_owned());
            if toolbar_button(ui, Icon::Sparkles, &label, self.codex_chat_open)
                .on_hover_text(format!("{tooltip}. Open Codex chat"))
                .clicked()
            {
                self.codex_chat_open = true;
                self.codex_chat_open_requested = true;
                self.codex_chat_error = None;
            }
            return;
        }

        if self.chatgpt_available {
            if toolbar_button(ui, Icon::Sparkles, "Connect ChatGPT", false)
                .on_hover_text("Use your ChatGPT subscription with Codex in Studio")
                .clicked()
            {
                self.chatgpt_auth_requested = true;
                self.set_chatgpt_pending();
            }
            return;
        }

        toolbar_status(ui, Icon::Sparkles, "ChatGPT unavailable", colors.faint).on_hover_text(
            self.chatgpt_error
                .as_deref()
                .unwrap_or("Codex App Server is unavailable"),
        );
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
                    let status_color = if self.project_error.is_some() {
                        colors.axis_x
                    } else if self.project_dirty {
                        colors.accent
                    } else {
                        colors.muted
                    };
                    inline_icon(
                        ui,
                        if self.project_error.is_some() {
                            Icon::Stop
                        } else {
                            Icon::Check
                        },
                        status_color,
                    );
                    let notice_width = (ui.available_width() - 124.0).max(40.0);
                    ui.add_sized(
                        [notice_width, 16.0],
                        egui::Label::new(
                            RichText::new(&self.notice)
                                .size(TYPE.meta)
                                .color(status_color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.notice);
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
                        if navigation_row(ui, icon, filter, self.asset_filter == filter, false)
                            .clicked()
                        {
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
                            let (icon, kind) = asset_kind(asset);
                            if asset_tile(ui, asset, icon, kind, self.selected_asset == asset)
                                .clicked()
                            {
                                self.selected_asset = asset;
                            }
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
                            false,
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
        self.show_morph_library(root);

        if root.available_width() >= 650.0 {
            egui::Panel::right("morph_inspector")
            .resizable(true)
            .default_size(280.0)
            .size_range(236.0..=360.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                panel_header(ui, Icon::Sliders, "Morph inspector", |ui| {
                    icon_button(ui, Icon::More, "Morph options", false);
                });
                content_frame().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                        if ui.button("Import character asset…").clicked() {
                            self.morph_import_requested = true;
                            self.morph_import_error = None;
                        }
                        if ui.button("Open .morph.json").clicked() {
                            self.morph_sidecar_import_requested = true;
                            self.morph_import_error = None;
                        }
                        if ui.button("Load .morphpack").clicked() {
                            self.morph_pack_import_requested = true;
                            self.morph_import_error = None;
                        }
                    });
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
                                if let Some((minimum, maximum)) = morph_mesh_bounds(preview) {
                                    let size: [f32; 3] = std::array::from_fn(|axis| {
                                        maximum[axis] - minimum[axis]
                                    });
                                    property_row(
                                        ui,
                                        "Source size",
                                        &format!(
                                            "{:.2} × {:.2} × {:.2}",
                                            size[0], size[1], size[2]
                                        ),
                                    );
                                    property_row(
                                        ui,
                                        "Runtime size",
                                        &format!(
                                            "{:.2} × {:.2} × {:.2}",
                                            size[0] * self.morph_attachment_scale[0],
                                            size[1] * self.morph_attachment_scale[1],
                                            size[2] * self.morph_attachment_scale[2]
                                        ),
                                    );
                                }
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
                                ui.label(
                                    RichText::new("Attachment offset X / Y / Z")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for (axis, value) in
                                        self.morph_attachment_translation.iter_mut().enumerate()
                                    {
                                        ui.add(
                                            egui::DragValue::new(value)
                                                .speed(0.01)
                                                .range(-10.0..=10.0)
                                                .prefix(["X ", "Y ", "Z "][axis]),
                                        );
                                    }
                                });
                                ui.label(
                                    RichText::new("Attachment scale X / Y / Z")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for (axis, value) in
                                        self.morph_attachment_scale.iter_mut().enumerate()
                                    {
                                        ui.add(
                                            egui::DragValue::new(value)
                                                .speed(0.01)
                                                .range(0.01..=100.0)
                                                .prefix(["X ", "Y ", "Z "][axis]),
                                        );
                                    }
                                });
                                if ui.button("Fit to person head").clicked() {
                                    self.fit_current_morph_to_person();
                                }
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
                                if ui.button("Save draft").clicked() {
                                    self.morph_draft_export_requested = true;
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
                                let add_to_game = ui
                                    .add_enabled(
                                        self.project_asset_available,
                                        egui::Button::new("Add to this game"),
                                    )
                                    .on_disabled_hover_text(
                                        "Open a game project before adding an asset.",
                                    );
                                if add_to_game.clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_project_add_requested = true;
                                    }
                                }
                                if ui.button("Export .morphpack").clicked() {
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
                    if let Some(preset) = self.morph_catalog.presets.iter().find(|preset| preset.id == self.selected_morph) {
                        selected_object_header(ui, &preset.display_name, "Starter");
                        property_section(ui, "Appearance", |ui| {
                            for id in std::iter::once(&preset.base).chain(preset.parts.iter()) {
                                if let Some(asset) = self.morph_catalog.asset(id) {
                                    property_row(ui, morph_kind_label(asset.kind), &asset.display_name);
                                }
                            }
                            if let Some(cubacadabra_morphs::MorphParameterValue::Text(skin)) = preset.parameters.get("skin") {
                                property_row(ui, "Skin", skin);
                            }
                        });
                    } else if let Some(asset) = self.morph_catalog.asset(&self.selected_morph) {
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
        }
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
                        RichText::new(if self.morph_preview.is_some() {
                            "Imported GLB"
                        } else {
                            self.morph_catalog
                                .presets
                                .iter()
                                .find(|preset| {
                                    crate::wardrobe::matches_preset(&self.active_loadout, preset)
                                })
                                .map_or("Custom appearance", |preset| preset.display_name.as_str())
                        })
                        .size(TYPE.secondary)
                        .color(colors.secondary_text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, Icon::More, "Preview options", false);
                        icon_button(ui, Icon::Camera, "Camera view", false);
                    });
                });
                let preview_rect = Rect::from_min_max(
                    egui::pos2(
                        available.min.x + 1.0,
                        available.min.y + EDITOR_HEADER_HEIGHT + 2.0,
                    ),
                    egui::pos2(available.max.x - 1.0, available.max.y - 1.0),
                );
                self.runtime_viewport = preview_rect;
                ui.allocate_rect(preview_rect, Sense::hover());
                if !self.morph_catalog_ready || self.morph_loading {
                    // The engine starts with its bundled appearance while the
                    // Studio catalog and initial loadout are arriving. Keep
                    // that implementation fallback out of the preview so it
                    // cannot flash before the requested appearance is ready.
                    ui.painter().rect_filled(preview_rect, 0.0, colors.surface);
                    ui.painter().text(
                        preview_rect.center(),
                        Align2::CENTER_CENTER,
                        "Loading appearance…",
                        FontId::proportional(TYPE.secondary),
                        colors.muted,
                    );
                }
                ui.painter().rect_stroke(
                    available,
                    0.0,
                    Stroke::new(1.0, colors.border),
                    StrokeKind::Inside,
                );
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
                if let Some(selected) = self
                    .scene_outline
                    .root
                    .find(&self.selected_scene)
                    .filter(|node| node.kind == "Block")
                {
                    let badge = Rect::from_min_size(
                        self.runtime_viewport.min + egui::vec2(12.0, 12.0),
                        egui::vec2(220.0, 42.0),
                    );
                    ui.painter()
                        .rect_filled(badge, UI.radius, colors.panel_raised);
                    ui.painter().rect_stroke(
                        badge,
                        UI.radius,
                        Stroke::new(1.0, colors.asset_selection_stroke),
                        StrokeKind::Inside,
                    );
                    ui.painter().text(
                        badge.min + egui::vec2(10.0, 13.0),
                        Align2::LEFT_CENTER,
                        format!("Selected · {}", selected.label),
                        semibold_font(TYPE.meta),
                        colors.text,
                    );
                    ui.painter().text(
                        badge.min + egui::vec2(10.0, 29.0),
                        Align2::LEFT_CENTER,
                        "Edit position or size in Inspector",
                        FontId::proportional(TYPE.meta - 1.0),
                        colors.secondary_text,
                    );
                }
                ui.allocate_rect(self.runtime_viewport, Sense::hover());
            });
    }

    fn scene_tree(&mut self, ui: &mut egui::Ui) {
        if self.project_loading.is_some() {
            return;
        }
        panel_header(ui, Icon::World, "Scene", |ui| {
            icon_button(ui, Icon::Filter, "Filter scene", false);
        });
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::NONE
                    .inner_margin(Margin::symmetric(0, 4))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        show_scene_node(
                            ui,
                            &self.scene_outline.root,
                            0,
                            &mut self.expanded_scene,
                            &mut self.selected_scene,
                        );
                    });
            });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        panel_header(ui, Icon::Sliders, "Inspector", |ui| {
            icon_button(ui, Icon::Lock, "Lock inspector", false);
            icon_button(ui, Icon::More, "Inspector options", false);
        });
        let selected = self
            .scene_outline
            .root
            .find(&self.selected_scene)
            .cloned()
            .unwrap_or_else(|| self.scene_outline.root.clone());
        content_frame().show(ui, |ui| {
            selected_object_header(ui, &selected.label, selected.kind);
            ui.add_space(2.0);
            if selected.kind == "Block" {
                self.block_inspector(ui, &selected);
            } else if selected.kind == "Sign" {
                self.sign_inspector(ui, &selected);
            } else if selected.properties.is_empty() {
                ui.label(
                    RichText::new("No properties")
                        .size(TYPE.secondary)
                        .color(palette(ui).muted),
                );
            } else {
                property_section(ui, "Manifest", |ui| {
                    for (label, value) in &selected.properties {
                        property_row(ui, label, value);
                    }
                });
            }
        });
    }

    fn block_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        if self.scene_editor_target != selected.id {
            self.scene_editor_target = selected.id.clone();
            self.scene_editor_position = vector_property(selected, "Position").unwrap_or([0.0; 3]);
            self.scene_editor_size = vector_property(selected, "Size").unwrap_or([1.0; 3]);
        }

        property_section(ui, "Transform", |ui| {
            let position_changed =
                vector_editor(ui, "Position", &mut self.scene_editor_position, 0.1);
            let size_changed = vector_editor(ui, "Size", &mut self.scene_editor_size, 0.1);
            if position_changed || size_changed {
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = Some(SceneEditRequest::UpdateBlock {
                    target: selected.id.clone(),
                    position: self.scene_editor_position,
                    size: self.scene_editor_size,
                });
                self.notice = "Platform changed — save to keep it".to_owned();
            }
        });
        property_section(ui, "Actions", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.project_editable, egui::Button::new("Duplicate"))
                    .on_disabled_hover_text("Open a raw source project to edit the scene")
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DuplicateBlock {
                        target: selected.id.clone(),
                    });
                    self.notice = "Duplicating platform…".to_owned();
                }
                if ui
                    .add_enabled(self.project_editable, egui::Button::new("Delete"))
                    .on_disabled_hover_text("Open a raw source project to edit the scene")
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DeleteBlock {
                        target: selected.id.clone(),
                    });
                    self.notice = "Deleting platform…".to_owned();
                }
            });
        });
        property_section(ui, "Manifest", |ui| {
            for (label, value) in &selected.properties {
                if label != "Position" && label != "Size" {
                    property_row(ui, label, value);
                }
            }
        });
        if !self.project_editable {
            ui.label(
                RichText::new("Open a raw source project to edit scene objects.")
                    .size(TYPE.meta)
                    .color(colors.muted),
            );
        }
    }

    fn sign_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        if self.scene_editor_target != selected.id {
            self.scene_editor_target = selected.id.clone();
            self.scene_editor_text = selected
                .properties
                .iter()
                .find(|(label, _)| label == "Text")
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
        }

        property_section(ui, "Content", |ui| {
            ui.label(
                RichText::new("Text")
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
            );
            if ui
                .add(
                    egui::TextEdit::multiline(&mut self.scene_editor_text)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text("Sign text"),
                )
                .changed()
            {
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = Some(SceneEditRequest::UpdateSignText {
                    target: selected.id.clone(),
                    text: self.scene_editor_text.clone(),
                });
                self.notice = "Sign text changed — save to keep it".to_owned();
            }
        });
        ui.label(
            RichText::new("Save, then Rebuild & Play to see the updated sign in the game.")
                .size(TYPE.meta)
                .color(colors.muted),
        );
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
                let query = self.search_query.trim().to_lowercase();
                let assets = self
                    .scene_outline
                    .assets
                    .iter()
                    .filter(|asset| match self.asset_filter {
                        "Images" => asset.kind == "IMAGE",
                        "Materials" => asset.kind == "MATERIAL",
                        "Characters" => asset.kind == "CHARACTER",
                        _ => true,
                    })
                    .filter(|asset| query.is_empty() || asset.name.to_lowercase().contains(&query))
                    .cloned()
                    .collect::<Vec<_>>();
                ui.horizontal_wrapped(|ui| {
                    if assets.is_empty() {
                        ui.label(
                            RichText::new(if query.is_empty() {
                                "No assets in this game."
                            } else {
                                "No matching assets."
                            })
                            .size(TYPE.secondary)
                            .color(colors.muted),
                        );
                    }
                    for asset in assets {
                        if asset_tile(
                            ui,
                            &asset.name,
                            asset.icon,
                            asset.kind,
                            self.selected_world_asset == asset.name,
                        )
                        .clicked()
                        {
                            self.selected_world_asset = asset.name;
                        }
                    }
                });
            });
    }
}

#[derive(Clone, Copy)]
struct MorphPreviewProjection {
    rect: Rect,
    center: [f32; 3],
    pixels_per_unit: f32,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

impl MorphPreviewProjection {
    fn project_world(self, vertex: [f32; 3]) -> egui::Pos2 {
        egui::pos2(
            self.rect.center().x
                + (vertex[0] - self.center[0]) * self.pixels_per_unit
                + (vertex[2] - self.center[2]) * self.pixels_per_unit * 0.12,
            self.rect.center().y - (vertex[1] - self.center[1]) * self.pixels_per_unit
                + (vertex[2] - self.center[2]) * self.pixels_per_unit * 0.06,
        )
    }

    fn transform_mesh(self, vertex: [f32; 3]) -> [f32; 3] {
        let scaled = [
            vertex[0] * self.scale[0],
            vertex[1] * self.scale[1],
            vertex[2] * self.scale[2],
        ];
        let [qx, qy, qz, qw] = self.rotation;
        let q = [qx, qy, qz];
        let twice_cross = cross3(q, scaled).map(|value| value * 2.0);
        let rotated = add3(
            scaled,
            add3(twice_cross.map(|value| value * qw), cross3(q, twice_cross)),
        );
        add3(rotated, self.translation)
    }

    fn project_mesh(self, vertex: [f32; 3]) -> egui::Pos2 {
        self.project_world(self.transform_mesh(vertex))
    }
}

fn morph_preview_projection(
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
    attachment: &MorphAttachment,
) -> Option<MorphPreviewProjection> {
    if mesh.vertices.is_empty() {
        return None;
    }
    // Keep the standard person head in frame so attachment scale is visible
    // instead of being hidden by an isolated auto-fit preview.
    let mut min: [f32; 3] = [-0.55, -0.46, -0.39];
    let mut max: [f32; 3] = [0.55, 0.46, 0.39];
    let projection = MorphPreviewProjection {
        rect,
        center: [0.0; 3],
        pixels_per_unit: 1.0,
        translation: attachment.translation,
        rotation: attachment.rotation,
        scale: attachment.scale,
    };
    for vertex in &mesh.vertices {
        let vertex = projection.transform_mesh(*vertex);
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let span = (max[0] - min[0])
        .max(max[1] - min[1])
        .max(max[2] - min[2])
        .max(0.0001);
    let pixels_per_unit = (rect.width().min(rect.height()) * 0.78) / span;
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    Some(MorphPreviewProjection {
        center,
        pixels_per_unit,
        ..projection
    })
}

fn paint_morph_head_reference(
    ui: &egui::Ui,
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
    attachment: &MorphAttachment,
    colors: Palette,
) {
    let Some(projection) = morph_preview_projection(rect, mesh, attachment) else {
        return;
    };
    let minimum = projection.project_world([-0.55, 0.46, 0.0]);
    let maximum = projection.project_world([0.55, -0.46, 0.0]);
    let head = Rect::from_min_max(minimum, maximum);
    ui.painter()
        .rect_filled(head, 18.0, colors.secondary_text.linear_multiply(0.16));
    ui.painter().rect_stroke(
        head,
        18.0,
        Stroke::new(1.0, colors.secondary_text.linear_multiply(0.45)),
        StrokeKind::Inside,
    );
}

fn paint_morph_surface(
    ui: &egui::Ui,
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
    attachment: &MorphAttachment,
    color: Color32,
) {
    if mesh.indices.len() < 3 {
        return;
    }
    let Some(projection) = morph_preview_projection(rect, mesh, attachment) else {
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
        let a = projection.transform_mesh(a);
        let b = projection.transform_mesh(b);
        let c = projection.transform_mesh(c);
        let points = [
            projection.project_world(a),
            projection.project_world(b),
            projection.project_world(c),
        ];
        // Low-detail exports can contain faces that are valid in 3D but
        // collapse to a sub-pixel sliver in this fixed front preview. egui's
        // polygon fill turns those into distracting bars, so leave them to
        // the wireframe inspection instead.
        if projected_triangle_area(points) < 0.5 {
            continue;
        }
        let normal = cross3(sub3(b, a), sub3(c, a));
        let brightness = (dot3(normalize3(normal), light).abs() * 0.55 + 0.45).clamp(0.0, 1.0);
        triangles.push(((a[2] + b[2] + c[2]) / 3.0, points, brightness));
    }
    triangles.sort_by(|first, second| first.0.total_cmp(&second.0));
    for (_, points, brightness) in triangles {
        let fill = Color32::from_rgba_unmultiplied(
            (base_color[0] * brightness * 255.0) as u8,
            (base_color[1] * brightness * 255.0) as u8,
            (base_color[2] * brightness * 255.0) as u8,
            (base_color[3] * 185.0) as u8,
        );
        ui.painter().add(egui::Shape::convex_polygon(
            points.to_vec(),
            fill,
            Stroke::NONE,
        ));
    }
}

fn projected_triangle_area(points: [egui::Pos2; 3]) -> f32 {
    ((points[1].x - points[0].x) * (points[2].y - points[0].y)
        - (points[1].y - points[0].y) * (points[2].x - points[0].x))
        .abs()
        * 0.5
}

fn paint_morph_wireframe(
    ui: &egui::Ui,
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
    attachment: &MorphAttachment,
    color: Color32,
) {
    if mesh.vertices.is_empty() || mesh.indices.len() < 3 {
        return;
    }
    let Some(projection) = morph_preview_projection(rect, mesh, attachment) else {
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
        let points = [
            projection.project_mesh(a),
            projection.project_mesh(b),
            projection.project_mesh(c),
        ];
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

fn add3(first: [f32; 3], second: [f32; 3]) -> [f32; 3] {
    [
        first[0] + second[0],
        first[1] + second[1],
        first[2] + second[2],
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
    )
    .on_hover_text(value);
}

fn property_field(ui: &mut egui::Ui, label: &str) -> Rect {
    let colors = palette(ui);
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CONTROL_HEIGHT),
        Sense::hover(),
    );
    let label_width = 84.0;
    ui.painter().text(
        egui::pos2(row.min.x + label_width - 8.0, row.center().y),
        Align2::RIGHT_CENTER,
        label,
        FontId::proportional(TYPE.secondary),
        colors.secondary_text,
    );
    Rect::from_min_max(row.min + egui::vec2(label_width, 0.0), row.max)
}

fn show_scene_node(
    ui: &mut egui::Ui,
    node: &SceneNode,
    depth: usize,
    expanded_nodes: &mut BTreeSet<String>,
    selected: &mut String,
) {
    let expanded = expanded_nodes.contains(&node.id);
    let has_children = !node.children.is_empty();
    let colors = palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), UI.row), Sense::click());
    let is_selected = *selected == node.id;
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            is_selected,
            &node.label,
        )
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
    let disclosure_rect =
        Rect::from_center_size(egui::pos2(x + 5.0, rect.center().y), Vec2::splat(14.0));
    let disclosure = has_children.then(|| {
        ui.interact(
            disclosure_rect,
            ui.id().with(("scene-disclosure", &node.id)),
            Sense::click(),
        )
    });
    if has_children {
        paint_icon(
            ui.painter(),
            disclosure_rect.shrink(2.0),
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
        node.icon,
        if is_selected {
            colors.text
        } else {
            colors.faint
        },
    );
    let label_font = if is_selected || has_children {
        medium_font(TYPE.primary)
    } else {
        FontId::proportional(TYPE.primary)
    };
    let detail_width = node.detail.as_ref().map_or(0.0, |detail| {
        ui.painter()
            .layout_no_wrap(
                detail.clone(),
                FontId::proportional(TYPE.meta),
                colors.faint,
            )
            .size()
            .x
            + 12.0
    });
    let label_left = x + 28.0;
    let label_right = (rect.max.x - detail_width).max(label_left);
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            egui::pos2(label_left, rect.min.y),
            egui::pos2(label_right, rect.max.y),
        ))
        .text(
            egui::pos2(x + 30.0, rect.center().y),
            Align2::LEFT_CENTER,
            &node.label,
            label_font,
            if is_selected {
                colors.text
            } else {
                colors.secondary_text
            },
        );
    if let Some(detail) = &node.detail {
        ui.painter().text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            detail,
            FontId::proportional(TYPE.meta),
            if is_selected {
                colors.text
            } else {
                colors.faint
            },
        );
    }
    if disclosure.is_some_and(|response| response.clicked()) {
        if expanded {
            expanded_nodes.remove(&node.id);
        } else {
            expanded_nodes.insert(node.id.clone());
        }
    } else if response.clicked() {
        *selected = node.id.clone();
        if has_children && response.double_clicked() {
            if expanded {
                expanded_nodes.remove(&node.id);
            } else {
                expanded_nodes.insert(node.id.clone());
            }
        }
    }
    if expanded {
        for child in &node.children {
            show_scene_node(ui, child, depth + 1, expanded_nodes, selected);
        }
    }
}

fn asset_tile(
    ui: &mut egui::Ui,
    name: &str,
    asset_icon: Icon,
    kind: &'static str,
    selected: bool,
) -> egui::Response {
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
    paint_icon(
        ui.painter(),
        Rect::from_center_size(preview.center(), Vec2::splat(22.0)),
        asset_icon,
        colors.muted,
    );
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            egui::pos2(rect.min.x + 4.0, rect.max.y - 16.0),
            egui::pos2(rect.max.x - 4.0, rect.max.y),
        ))
        .text(
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
    response.on_hover_text(format!("{name} · {} preview", kind.to_lowercase()))
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

fn navigation_row(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    active: bool,
) -> egui::Response {
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
    if active {
        ui.painter().text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            "ON",
            FontId::new(TYPE.meta, FontFamily::Name(MEDIUM_FONT_FAMILY.into())),
            colors.live,
        );
    }
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
    let width = toolbar_button_width(ui, label);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, CONTROL_HEIGHT), Sense::click());
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

fn toolbar_status(ui: &mut egui::Ui, icon: Icon, label: &str, color: Color32) -> egui::Response {
    let width = toolbar_button_width(ui, label);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, CONTROL_HEIGHT), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(12.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        color,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(22.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        medium_font(TYPE.secondary),
        color,
    );
    response
}

fn chatgpt_account_label(account: &ChatGptAccount) -> String {
    let plan = match account.plan_type.as_deref() {
        Some("free") => Some("Free"),
        Some("go") => Some("Go"),
        Some("plus") => Some("Plus"),
        Some("pro" | "prolite") => Some("Pro"),
        Some("team" | "self_serve_business_prolite" | "self_serve_business_usage_based") => {
            Some("Business")
        }
        Some("business") => Some("Business"),
        Some("ent26" | "enterprise_cbp_automation" | "enterprise_cbp_usage_based") => {
            Some("Enterprise")
        }
        Some("enterprise") => Some("Enterprise"),
        Some("edu" | "edu_plus" | "edu_pro") => Some("Edu"),
        _ => None,
    };
    plan.map_or_else(|| "ChatGPT".to_owned(), |plan| format!("ChatGPT · {plan}"))
}

fn codex_chat_model_label(model: &str, selected_model: &str) -> String {
    let marker = match (model == CODEX_CHAT_DEFAULT_MODEL, model == selected_model) {
        (true, true) => " (default · current)",
        (true, false) => " (default)",
        (false, true) => " (current)",
        (false, false) => "",
    };
    format!("{model}{marker}")
}

fn codex_chat_model_description(model: &str) -> &'static str {
    CODEX_CHAT_MODELS
        .iter()
        .find_map(|(candidate, description)| (*candidate == model).then_some(*description))
        .unwrap_or("Select a Codex model.")
}

fn toolbar_button_width(ui: &egui::Ui, label: &str) -> f32 {
    let label_width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            medium_font(TYPE.secondary),
            Color32::WHITE,
        )
        .size()
        .x;
    (label_width + 30.0).ceil().max(44.0)
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
        Icon::Sparkles => {
            let large = c - egui::vec2(r * 0.2, r * 0.12);
            painter.line_segment(
                [
                    egui::pos2(large.x, top),
                    egui::pos2(large.x, bottom - r * 0.08),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(left + r * 0.08, large.y),
                    egui::pos2(right - r * 0.35, large.y),
                ],
                stroke,
            );
            let small = c + egui::vec2(r * 0.55, r * 0.5);
            painter.line_segment(
                [
                    small - egui::vec2(0.0, r * 0.26),
                    small + egui::vec2(0.0, r * 0.26),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    small - egui::vec2(r * 0.26, 0.0),
                    small + egui::vec2(r * 0.26, 0.0),
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
