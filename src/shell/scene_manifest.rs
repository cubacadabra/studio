use super::*;
use cubacadabra_scene::AuthoringNode;

pub(crate) fn manifest_active_world(source: &str) -> String {
    serde_json::from_str::<Value>(source)
        .ok()
        .and_then(|manifest| {
            manifest
                .pointer("/launch/destinationWorld")
                .and_then(Value::as_str)
                .or_else(|| manifest.get("startWorld").and_then(Value::as_str))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "lobby".to_owned())
}

pub(crate) fn authoring_scene_node(
    node: &AuthoringNode,
    children_by_parent: &BTreeMap<&str, Vec<&AuthoringNode>>,
    asset_bounds: &BTreeMap<String, [f32; 3]>,
) -> SceneNode {
    let (kind, icon) = if node.components.contains_key("primitive") {
        ("Block", Icon::Object)
    } else if node.components.contains_key("render") {
        ("Mesh", Icon::Object)
    } else if node.components.contains_key("text") {
        ("Sign", Icon::Object)
    } else if node.components.contains_key("interaction") {
        ("Interaction", Icon::Object)
    } else if node.components.contains_key("actor") {
        ("Actor", Icon::Character)
    } else if node.components.contains_key("ladder") {
        ("Ladder", Icon::Object)
    } else if node.components.contains_key("checkpoint") {
        ("Checkpoint", Icon::Object)
    } else if node.components.contains_key("hazard") {
        ("Hazard", Icon::Object)
    } else if node.components.contains_key("safeZone") {
        ("Safe Zone", Icon::Object)
    } else {
        ("Group", Icon::Folder)
    };
    let mut properties = vec![
        ("Authoring ID".to_owned(), node.id.clone()),
        ("Name".to_owned(), node.name.clone()),
        ("Kind".to_owned(), kind.to_owned()),
        (
            "Parent".to_owned(),
            node.parent_id.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            "Position".to_owned(),
            format_vector(node.transform.position),
        ),
        (
            "Rotation".to_owned(),
            format_vector(node.transform.rotation),
        ),
        ("Scale".to_owned(), format_vector(node.transform.scale)),
        (
            "Visible".to_owned(),
            if node.editor.visible { "Yes" } else { "No" }.to_owned(),
        ),
        (
            "Locked".to_owned(),
            if node.editor.locked { "Yes" } else { "No" }.to_owned(),
        ),
    ];
    if let Some(reason) = &node.editor.lock_reason {
        properties.push(("Lock reason".to_owned(), reason.clone()));
    }
    let mesh_asset_bounds = node
        .components
        .get("render")
        .and_then(Value::as_object)
        .and_then(|render| render.get("mesh"))
        .and_then(Value::as_str)
        .and_then(|mesh| asset_bounds.get(mesh).copied());
    let primitive_size = node
        .components
        .get("primitive")
        .and_then(Value::as_object)
        .and_then(|primitive| primitive.get("size"))
        .and_then(vector_value);
    let volume_size = node
        .components
        .get("ladder")
        .or_else(|| node.components.get("hazard"))
        .and_then(Value::as_object)
        .and_then(|component| component.get("size"))
        .and_then(vector_value);
    let render_size = node
        .components
        .get("render")
        .and_then(Value::as_object)
        .and_then(|render| render.get("bounds"))
        .and_then(vector_value)
        .or(mesh_asset_bounds);
    if let Some(bounds) = primitive_size.or(volume_size).or(render_size) {
        properties.push(("Size".to_owned(), format_vector(bounds)));
    }
    if let Some(source) = &node.source {
        properties.push(("Source".to_owned(), source.format.clone()));
        if let Some(class) = &source.class {
            properties.push(("Source class".to_owned(), class.clone()));
        }
        if let Some(path) = &source.path {
            properties.push(("Source path".to_owned(), path.clone()));
        }
        for (key, value) in &source.properties {
            if let Some(value) = compact_value(value) {
                properties.push((humanize_identifier(key), value));
            }
        }
    }
    for (component, value) in &node.components {
        if let Some(object) = value.as_object() {
            for (key, value) in object {
                if component == "actor" && key == "appearance" {
                    if let Some(appearance) = value.as_object() {
                        for (key, value) in appearance {
                            if let Some(value) = compact_value(value) {
                                properties.push((humanize_identifier(key), value));
                            }
                        }
                    }
                    continue;
                }
                if let Some(value) = compact_value(value) {
                    properties.push((format_component_property(component, key), value));
                }
            }
        }
    }
    let source_children = children_by_parent
        .get(node.id.as_str())
        .into_iter()
        .flatten()
        .filter(|child| is_imported_source_root(child))
        .copied()
        .collect::<Vec<_>>();
    let mut children = children_by_parent
        .get(node.id.as_str())
        .into_iter()
        .flatten()
        .filter(|child| !is_imported_source_root(child))
        .map(|child| authoring_scene_node(child, children_by_parent, asset_bounds))
        .collect::<Vec<_>>();
    if !source_children.is_empty() {
        let source_count = source_children
            .first()
            .and_then(|source| source.source.as_ref())
            .and_then(|source| source.properties.get("instanceCount"))
            .and_then(Value::as_u64)
            .map(|count| format!("{count} source nodes"));
        children.push(SceneNode {
            id: format!("{}/imported-source", node.id),
            label: "Imported Source".to_owned(),
            kind: "Collection",
            icon: Icon::Folder,
            detail: source_count,
            properties: vec![
                ("Kind".to_owned(), "Source/reference data".to_owned()),
                (
                    "Editing".to_owned(),
                    "Source nodes are preserved for inspection".to_owned(),
                ),
            ],
            children: source_children
                .into_iter()
                .map(|child| authoring_scene_node(child, children_by_parent, asset_bounds))
                .collect(),
        });
    }
    SceneNode {
        id: node.id.clone(),
        label: imported_model_display_name(node, children_by_parent),
        kind,
        icon,
        detail: node.editor.locked.then(|| "Locked".to_owned()),
        properties,
        children,
    }
}

fn imported_model_display_name(
    node: &AuthoringNode,
    children_by_parent: &BTreeMap<&str, Vec<&AuthoringNode>>,
) -> String {
    if node
        .source
        .as_ref()
        .and_then(|source| source.class.as_deref())
        != Some("Model")
    {
        return node.name.clone();
    }
    let Some(siblings) = node
        .parent_id
        .as_deref()
        .and_then(|parent| children_by_parent.get(parent))
    else {
        return node.name.clone();
    };
    let mut matching = siblings.iter().filter(|sibling| {
        sibling.name == node.name
            && sibling
                .source
                .as_ref()
                .and_then(|source| source.class.as_deref())
                == Some("Model")
    });
    let duplicate_count = matching.clone().count();
    if duplicate_count < 2 {
        return node.name.clone();
    }
    let ordinal = matching
        .position(|sibling| sibling.id == node.id)
        .map_or(1, |index| index + 1);
    format!("{} {ordinal}", node.name)
}

fn is_imported_source_root(node: &AuthoringNode) -> bool {
    node.source
        .as_ref()
        .and_then(|source| source.class.as_deref())
        == Some("SourceHierarchy")
}

pub(crate) fn is_authoring_node(node: &SceneNode) -> bool {
    node.properties
        .iter()
        .any(|(label, _)| label == "Authoring ID")
}

pub(crate) fn is_authoring_placeable(node: &SceneNode) -> bool {
    is_authoring_node(node)
        && matches!(
            node.kind,
            "Block"
                | "Mesh"
                | "Sign"
                | "Actor"
                | "Interaction"
                | "Ladder"
                | "Checkpoint"
                | "Hazard"
                | "Safe Zone"
        )
}

pub(crate) fn scene_node_locked(node: &SceneNode) -> bool {
    node.properties
        .iter()
        .find(|(label, _)| label == "Locked")
        .is_some_and(|(_, value)| value == "Yes")
}

pub(crate) fn format_vector(values: [f32; 3]) -> String {
    values
        .into_iter()
        .map(|value| format_scene_number(value))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn vector_value(value: &Value) -> Option<[f32; 3]> {
    let values = value.as_array()?;
    Some([
        values.first()?.as_f64()? as f32,
        values.get(1)?.as_f64()? as f32,
        values.get(2)?.as_f64()? as f32,
    ])
}

pub(crate) fn format_component_property(component: &str, key: &str) -> String {
    let label = humanize_identifier(key);
    match component {
        "primitive" => match key {
            "material" => "Color".to_owned(),
            "runtimeMaterial" => "Material".to_owned(),
            _ => label,
        },
        "render" => format!("Render {label}"),
        "text" => label,
        "interaction" => label,
        "ladder" | "checkpoint" | "hazard" | "safeZone" => label,
        _ => format!("{component} {label}"),
    }
}

pub(crate) fn scene_world_id(node_id: &str) -> Option<&str> {
    let mut parts = node_id.split('/');
    (parts.next() == Some("world"))
        .then(|| parts.next())
        .flatten()
        .filter(|world| !world.is_empty())
}

pub(crate) fn is_scene_object(node_id: &str) -> bool {
    let mut parts = node_id.split('/');
    if parts.next() != Some("world") || parts.next().is_none() {
        return false;
    }
    let Some(collection) = parts.next() else {
        return false;
    };
    parts.next().is_some()
        && parts.next().is_none()
        && SceneObjectKind::MANIFEST_KINDS
            .iter()
            .any(|kind| kind.collection() == Some(collection))
}

pub(crate) const WORLD_COLLECTIONS: [(&str, &str, &str, Icon); 12] = [
    ("launchPads", "Launch Pads", "Launch Pad", Icon::Object),
    ("blocks", "Blocks", "Block", Icon::Object),
    ("portals", "Portals", "Portal", Icon::Object),
    ("signs", "Signs", "Sign", Icon::Object),
    ("billboards", "Billboards", "Billboard", Icon::Image),
    ("interactions", "Interactions", "Interaction", Icon::Object),
    ("actors", "Actors", "Actor", Icon::Character),
    ("ladders", "Ladders", "Ladder", Icon::Object),
    ("checkpoints", "Checkpoints", "Checkpoint", Icon::Object),
    ("hazards", "Hazards", "Hazard", Icon::Object),
    ("safeZones", "Safe Zones", "Safe Zone", Icon::Object),
    ("clouds", "Clouds", "Cloud", Icon::Object),
];

pub(crate) fn world_node(world_id: &str, definition: &Value, active: bool) -> SceneNode {
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

pub(crate) fn collection_node(
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

pub(crate) fn item_label(item: &Value, fallback: &str, index: usize) -> String {
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

pub(crate) fn object_properties(value: &Value) -> Vec<(String, String)> {
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
    texts: &mut [String; 3],
) -> bool {
    let colors = palette(ui);
    let mut changed = false;
    ui.label(
        RichText::new(label)
            .size(TYPE.secondary)
            .color(colors.secondary_text),
    );
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let field_width = ((ui.available_width() - 36.0) / 3.0).max(38.0);
        for axis in 0..3 {
            ui.label(
                RichText::new(["X", "Y", "Z"][axis])
                    .size(TYPE.meta)
                    .color(colors.muted),
            );
            let response = ui.add_sized(
                [field_width, CONTROL_HEIGHT],
                egui::TextEdit::singleline(&mut texts[axis])
                    .id_salt((label, axis))
                    .horizontal_align(Align::RIGHT),
            );
            if response.lost_focus()
                && let Ok(value) = texts[axis].trim().parse::<f32>()
                && value.is_finite()
                && values[axis] != value
            {
                values[axis] = value;
                changed = true;
            }
            if response.lost_focus() {
                texts[axis] = format_scene_number(values[axis]);
            }
        }
    });
    changed
}

pub(crate) fn scene_vector_text(values: [f32; 3]) -> [String; 3] {
    values.map(format_scene_number)
}

pub(crate) fn format_scene_number(value: f32) -> String {
    let formatted = format!("{value:.3}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

pub(crate) fn avatar_properties(value: &Value) -> Vec<(String, String)> {
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

pub(crate) fn manifest_assets(manifest: &Value) -> Vec<ManifestAsset> {
    let mut assets = BTreeMap::<(String, String), ManifestAsset>::new();
    if let Some(groups) = manifest.get("assets").and_then(Value::as_object) {
        for (group, definitions) in groups {
            let Some(definitions) = definitions.as_object() else {
                continue;
            };
            let kind = match group.as_str() {
                "images" => "IMAGE",
                "audio" => "AUDIO",
                "models" => "MODEL",
                "characters" | "morphs" => "CHARACTER",
                _ => "ASSET",
            };
            for (name, definition) in definitions {
                assets.insert(
                    (kind.to_owned(), name.clone()),
                    ManifestAsset {
                        name: name.clone(),
                        kind,
                        bounds: (kind == "MODEL")
                            .then(|| definition.get("bounds"))
                            .flatten()
                            .and_then(vector_value),
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
                        bounds: None,
                    },
                );
            }
        }
    }

    assets.into_values().collect()
}

pub(crate) fn compact_value(value: &Value) -> Option<String> {
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

pub(crate) fn humanize_identifier(value: &str) -> String {
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
