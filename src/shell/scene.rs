use super::*;
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
    pub(crate) const ALL: [Self; 5] = [
        Self::World,
        Self::Assets,
        Self::Materials,
        Self::Morphs,
        Self::Test,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::World => "World",
            Self::Assets => "Assets",
            Self::Materials => "Materials",
            Self::Morphs => "Morphs",
            Self::Test => "Test",
        }
    }

    pub(crate) fn command(self) -> StudioCommand {
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
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: &'static str,
    pub(crate) icon: Icon,
    pub(crate) detail: Option<String>,
    pub(crate) properties: Vec<(String, String)>,
    pub(crate) children: Vec<SceneNode>,
}

impl SceneNode {
    pub(crate) fn find(&self, id: &str) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SceneOutline {
    pub(crate) root: SceneNode,
    pub(crate) assets: Vec<ManifestAsset>,
    pub(crate) initial_selection: String,
    pub(crate) initial_expanded: BTreeSet<String>,
}

pub(crate) struct ProjectLoadingState {
    pub(crate) progress: f32,
    pub(crate) rebuilding: bool,
    pub(crate) previous_preview_stale: bool,
    pub(crate) previous_outline: SceneOutline,
    pub(crate) previous_expanded: BTreeSet<String>,
    pub(crate) previous_selection: String,
    pub(crate) previous_world_asset: String,
    pub(crate) previous_workspace: Workspace,
}

#[derive(Clone, Debug)]
pub(crate) struct ManifestAsset {
    pub(crate) name: String,
    pub(crate) kind: &'static str,
    pub(crate) icon: Icon,
}

impl SceneOutline {
    pub(crate) fn parse(source: &str) -> Result<Self, serde_json::Error> {
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

    pub(crate) fn empty() -> Self {
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

pub(crate) const WORLD_COLLECTIONS: [(&str, &str, &str, Icon); 11] = [
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

pub(crate) fn vector_property(node: &SceneNode, label: &str) -> Option<[f32; 3]> {
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

pub(crate) fn vector_editor(
    ui: &mut egui::Ui,
    label: &str,
    values: &mut [f32; 3],
    speed: f32,
) -> bool {
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
