use super::*;
use cubacadabra_scene::AuthoringWorldTransform;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Workspace {
    #[default]
    World,
    Scripts,
    Assets,
    Materials,
    Morphs,
    Test,
}

impl Workspace {
    pub(crate) const ALL: [Self; 3] = [Self::World, Self::Scripts, Self::Morphs];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::World => "World",
            Self::Scripts => "Files",
            Self::Assets => "Assets",
            Self::Materials => "Materials",
            Self::Morphs => "Morphs",
            Self::Test => "Test",
        }
    }

    pub(crate) fn command(self) -> StudioCommand {
        match self {
            Self::World => StudioCommand::ShowWorld,
            Self::Scripts => StudioCommand::ShowScripts,
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

    pub(crate) fn collect_ancestor_ids(&self, target: &str, path: &mut Vec<String>) -> bool {
        if self.id == target {
            return true;
        }
        for child in &self.children {
            if child.collect_ancestor_ids(target, path) {
                path.push(self.id.clone());
                return true;
            }
        }
        false
    }

    fn collect_placeable_objects(
        &self,
        objects: &mut Vec<SceneObjectGeometry>,
        world_transforms: &BTreeMap<String, AuthoringWorldTransform>,
    ) {
        if (is_scene_object(&self.id) || is_authoring_placeable(self))
            && !scene_node_locked(self)
            && let Some(position) = vector_property(self, "Position")
        {
            objects.push(SceneObjectGeometry {
                id: self.id.clone(),
                position: world_transforms
                    .get(&self.id)
                    .map(|transform| transform.position)
                    .unwrap_or(position),
                size: vector_property(self, "Size"),
                scale: is_authoring_node(self).then(|| {
                    world_transforms
                        .get(&self.id)
                        .map(|transform| transform.scale)
                        .unwrap_or_else(|| vector_property(self, "Scale").unwrap_or([1.0; 3]))
                }),
                primitive_size: self.kind == "Block",
                editable: !scene_node_locked(self),
            });
        }
        for child in &self.children {
            child.collect_placeable_objects(objects, world_transforms);
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SceneOutline {
    pub(crate) root: SceneNode,
    pub(crate) assets: Vec<ManifestAsset>,
    pub(crate) initial_selection: String,
    pub(crate) initial_expanded: BTreeSet<String>,
    pub(crate) authoring_world_transforms: BTreeMap<String, AuthoringWorldTransform>,
    pub(crate) placeable_objects: Vec<SceneObjectGeometry>,
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
    pub(crate) bounds: Option<[f32; 3]>,
}

impl SceneOutline {
    fn rebuild_placeable_objects(&mut self) {
        self.placeable_objects.clear();
        self.root.collect_placeable_objects(
            &mut self.placeable_objects,
            &self.authoring_world_transforms,
        );
    }

    pub(crate) fn placeable_object_geometries(&self) -> &[SceneObjectGeometry] {
        &self.placeable_objects
    }

    pub(crate) fn search_matches(&self, query: &str) -> BTreeSet<String> {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return BTreeSet::new();
        }
        fn visit(node: &SceneNode, query: &str, matches: &mut BTreeSet<String>) -> bool {
            let direct = node.label.to_ascii_lowercase().contains(query)
                || node.id.to_ascii_lowercase().contains(query)
                || node.kind.to_ascii_lowercase().contains(query);
            let descendant = node
                .children
                .iter()
                .map(|child| visit(child, query, matches))
                .any(|matched| matched);
            if direct || descendant {
                matches.insert(node.id.clone());
            }
            direct || descendant
        }
        let mut matches = BTreeSet::new();
        visit(&self.root, &query, &mut matches);
        matches
    }

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

        let mut outline = Self {
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
            authoring_world_transforms: BTreeMap::new(),
            placeable_objects: Vec::new(),
        };
        outline.rebuild_placeable_objects();
        Ok(outline)
    }

    pub(crate) fn parse_with_authoring_scene(
        source: &str,
        scene: &AuthoringScene,
    ) -> Result<Self, String> {
        let mut outline = Self::parse(source).map_err(|error| error.to_string())?;
        scene.validate()?;
        let active_world = manifest_active_world(source);
        if let Some(world_id) = scene.world_id.as_deref()
            && world_id != active_world
        {
            return Err(format!(
                "scene.json worldId {world_id:?} does not match active manifest world {active_world:?}"
            ));
        }
        let roots = scene
            .nodes
            .iter()
            .filter(|node| node.parent_id.is_none())
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return Err("scene.json must contain at least one root node".to_owned());
        }
        let children = scene
            .nodes
            .iter()
            .filter_map(|node| node.parent_id.as_deref().map(|parent| (parent, node)))
            .fold(
                BTreeMap::<&str, Vec<&AuthoringNode>>::new(),
                |mut children, (parent, node)| {
                    children.entry(parent).or_default().push(node);
                    children
                },
            );
        let asset_bounds = outline
            .assets
            .iter()
            .filter(|asset| asset.kind == "MODEL")
            .filter_map(|asset| asset.bounds.map(|bounds| (asset.name.clone(), bounds)))
            .collect::<BTreeMap<_, _>>();
        let authoring_roots = roots
            .into_iter()
            .map(|node| authoring_scene_node(node, &children, &asset_bounds))
            .collect::<Vec<_>>();
        let authoring_world_transforms = scene.world_transforms()?;
        let manifest_world_id = format!("world/{active_world}");
        let active_world_index = outline
            .root
            .children
            .iter()
            .position(|child| child.id == manifest_world_id);
        let matching_root = authoring_roots.iter().find(|root| {
            root.id == format!("world-{active_world}")
                || root.label == humanize_identifier(&active_world)
        });
        let mut replaced = false;
        if let Some(index) = active_world_index
            && let Some(root) =
                matching_root.or_else(|| (authoring_roots.len() == 1).then(|| &authoring_roots[0]))
        {
            let root_id = root.id.clone();
            outline.root.children[index] = root.clone();
            outline.root.children.extend(
                authoring_roots
                    .iter()
                    .filter(|root| root.id != root_id)
                    .cloned(),
            );
            replaced = true;
        }
        if !replaced {
            outline.root.children.extend(authoring_roots);
        }
        outline.initial_selection = outline
            .root
            .children
            .iter()
            .find(|child| child.id == format!("world/{active_world}") || is_authoring_node(child))
            .map(|child| child.id.clone())
            .unwrap_or_else(|| outline.root.id.clone());
        outline.initial_expanded =
            BTreeSet::from([outline.root.id.clone(), outline.initial_selection.clone()]);
        for node in &scene.nodes {
            if children.contains_key(node.id.as_str()) {
                outline.initial_expanded.insert(node.id.clone());
            }
        }
        outline.authoring_world_transforms = authoring_world_transforms;
        outline.rebuild_placeable_objects();
        Ok(outline)
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
            authoring_world_transforms: BTreeMap::new(),
            placeable_objects: Vec::new(),
        }
    }

    pub(crate) fn set_runtime_ui_nodes(&mut self, nodes: &[cubacadabra_client::StudioUiNode]) {
        self.root
            .children
            .retain(|child| child.id != "game/interface");
        if nodes.is_empty() {
            return;
        }
        let children = nodes
            .iter()
            .map(|node| SceneNode {
                id: format!("game/interface/{}", node.id),
                label: node.id.clone(),
                kind: match node.kind.as_str() {
                    "Text" => "Text",
                    "Button" => "Button",
                    "Panel" => "Panel",
                    "Stack" => "Stack",
                    "Menu" => "Menu",
                    "Modal" => "Modal",
                    "Toggle" => "Toggle",
                    "Slider" => "Slider",
                    "Joystick" => "Joystick",
                    _ => "UI Element",
                },
                icon: Icon::Object,
                detail: (!node.text.is_empty()).then(|| node.text.clone()),
                properties: vec![
                    ("Text".to_owned(), node.text.clone()),
                    ("Runtime id".to_owned(), node.id.clone()),
                    (
                        "Editing".to_owned(),
                        "Ask Codex to change this UI".to_owned(),
                    ),
                ],
                children: Vec::new(),
            })
            .collect::<Vec<_>>();
        self.root.children.push(SceneNode {
            id: "game/interface".to_owned(),
            label: "Game UI".to_owned(),
            kind: "Collection",
            icon: Icon::Folder,
            detail: Some(nodes.len().to_string()),
            properties: Vec::new(),
            children,
        });
    }
}
