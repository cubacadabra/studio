use crate::shell::{SceneEditRequest, SceneObjectKind};
use cubacadabra_scene::{AuthoringNode, AuthoringScene, EditorMetadata, Transform};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug)]
pub(crate) struct ManifestSceneMigration {
    pub(crate) scene: AuthoringScene,
    pub(crate) cleaned_manifest: Value,
    pub(crate) target_map: BTreeMap<String, String>,
}

pub(crate) fn migrate_manifest_to_scene(source: &str) -> Result<ManifestSceneMigration, String> {
    let mut manifest: Value = serde_json::from_str(source)
        .map_err(|error| format!("manifest is no longer valid JSON: {error}"))?;
    let world_id = active_manifest_world_id(&manifest);
    let world = if world_id == "lobby" {
        &manifest
    } else {
        manifest
            .get("worlds")
            .and_then(Value::as_object)
            .and_then(|worlds| worlds.get(&world_id))
            .ok_or_else(|| format!("scene world `{world_id}` was not found"))?
    };
    let root_id = format!("world-{world_id}");
    let mut target_map = BTreeMap::new();
    let mut nodes = vec![AuthoringNode {
        id: root_id.clone(),
        parent_id: None,
        name: format!("{} World", world_id.replace(['-', '_'], " ")),
        transform: Transform::default(),
        components: BTreeMap::new(),
        editor: EditorMetadata::default(),
        source: None,
    }];

    if let Some(blocks) = manifest_collection(world, "blocks")? {
        for (index, block) in blocks.iter().enumerate() {
            let object = manifest_collection_object(block, "blocks", index)?;
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .map(|id| format!("block-{id}"))
                .unwrap_or_else(|| format!("block-{}", index + 1));
            let mut primitive = serde_json::Map::from_iter([
                ("shape".to_owned(), serde_json::json!("box")),
                (
                    "size".to_owned(),
                    object
                        .get("size")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([1, 1, 1])),
                ),
            ]);
            if let Some(color) = object.get("color") {
                primitive.insert("color".to_owned(), color.clone());
            }
            let mut components =
                BTreeMap::from([("primitive".to_owned(), Value::Object(primitive))]);
            if let Some(runtime_id) = object.get("id").and_then(Value::as_str)
                && let Some(primitive) = components
                    .get_mut("primitive")
                    .and_then(Value::as_object_mut)
            {
                primitive.insert("runtimeId".to_owned(), Value::String(runtime_id.to_owned()));
            }
            if let Some(primitive) = components
                .get_mut("primitive")
                .and_then(Value::as_object_mut)
            {
                if let Some(material) = object.get("material") {
                    primitive.insert("material".to_owned(), material.clone());
                }
                if let Some(outline) = object.get("outline") {
                    primitive.insert("outline".to_owned(), outline.clone());
                }
            }
            components.insert("collision".to_owned(), serde_json::json!({ "kind": "box" }));
            target_map.insert(format!("world/{world_id}/blocks/{index}"), id.clone());
            nodes.push(AuthoringNode {
                id,
                parent_id: Some(root_id.clone()),
                name: format!("Block {}", index + 1),
                transform: Transform {
                    position: scene_vector(object.get("position"), [0.0, 0.0, 0.0]),
                    ..Transform::default()
                },
                components,
                editor: EditorMetadata::default(),
                source: None,
            });
        }
    }
    if let Some(decorations) = manifest_collection(world, "decorations")? {
        for (index, decoration) in decorations.iter().enumerate() {
            let object = manifest_collection_object(decoration, "decorations", index)?;
            let Some(asset) = object.get("asset").and_then(Value::as_str) else {
                return Err(format!(
                    "cannot migrate decoration {} without an asset-backed mesh",
                    index + 1
                ));
            };
            if object
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "mesh")
            {
                return Err(format!(
                    "cannot migrate legacy decoration {} into scene.json without losing its kind",
                    index + 1
                ));
            }
            let uniform_scale = object
                .get("scale")
                .and_then(Value::as_f64)
                .map(|value| value as f32)
                .unwrap_or(1.0);
            let scale = object
                .get("scale3")
                .map(|value| scene_vector(Some(value), [uniform_scale; 3]))
                .unwrap_or([uniform_scale; 3]);
            let mut render = serde_json::json!({
                "mesh": asset,
                "color": object
                    .get("color")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!("#FFFFFF")),
            });
            if let Some(material) = object.get("material")
                && let Some(render) = render.as_object_mut()
            {
                render.insert("material".to_owned(), material.clone());
            }
            let id = format!("decoration-{}", index + 1);
            target_map.insert(format!("world/{world_id}/decorations/{index}"), id.clone());
            nodes.push(AuthoringNode {
                id,
                parent_id: Some(root_id.clone()),
                name: format!("Decoration {}", index + 1),
                transform: Transform {
                    position: scene_vector(object.get("position"), [0.0, 0.0, 0.0]),
                    rotation: [
                        0.0,
                        object.get("yaw").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        0.0,
                    ],
                    scale,
                },
                components: BTreeMap::from([("render".to_owned(), render)]),
                editor: EditorMetadata::default(),
                source: None,
            });
        }
    }
    if let Some(signs) = manifest_collection(world, "signs")? {
        for (index, sign) in signs.iter().enumerate() {
            let object = manifest_collection_object(sign, "signs", index)?;
            let mut components = BTreeMap::new();
            let mut text = serde_json::Map::from_iter([(
                "text".to_owned(),
                object
                    .get("text")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!("")),
            )]);
            for key in ["maxWidth", "color"] {
                if let Some(value) = object.get(key) {
                    text.insert(key.to_owned(), value.clone());
                }
            }
            components.insert("text".to_owned(), Value::Object(text));
            if let Some(runtime_id) = object.get("id").cloned()
                && let Some(text) = components.get_mut("text").and_then(Value::as_object_mut)
            {
                text.insert("id".to_owned(), runtime_id);
            }
            let id = format!("sign-{}", index + 1);
            target_map.insert(format!("world/{world_id}/signs/{index}"), id.clone());
            nodes.push(AuthoringNode {
                id,
                parent_id: Some(root_id.clone()),
                name: format!("Sign {}", index + 1),
                transform: Transform {
                    position: scene_vector(object.get("position"), [0.0, 0.0, 0.0]),
                    rotation: [
                        0.0,
                        object.get("yaw").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        0.0,
                    ],
                    ..Transform::default()
                },
                components,
                editor: EditorMetadata::default(),
                source: None,
            });
        }
    }
    if let Some(interactions) = manifest_collection(world, "interactions")? {
        for (index, interaction) in interactions.iter().enumerate() {
            let object = manifest_collection_object(interaction, "interactions", index)?;
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .map(|id| format!("interaction-{id}"))
                .unwrap_or_else(|| format!("interaction-{}", index + 1));
            let mut component = object.clone();
            component.remove("position");
            let mut components = BTreeMap::new();
            components.insert("interaction".to_owned(), Value::Object(component));
            target_map.insert(format!("world/{world_id}/interactions/{index}"), id.clone());
            nodes.push(AuthoringNode {
                id,
                parent_id: Some(root_id.clone()),
                name: format!("Interaction {}", index + 1),
                transform: Transform {
                    position: scene_vector(object.get("position"), [0.0, 0.0, 0.0]),
                    ..Transform::default()
                },
                components,
                editor: EditorMetadata::default(),
                source: None,
            });
        }
    }
    if let Some(actors) = manifest_collection(world, "actors")? {
        for (index, actor) in actors.iter().enumerate() {
            let object = manifest_collection_object(actor, "actors", index)?;
            let runtime_id = object
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("actor-{}", index + 1));
            let id = format!("actor-{runtime_id}");
            let mut component = object.clone();
            component.remove("position");
            let yaw = component
                .remove("yaw")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0);
            target_map.insert(format!("world/{world_id}/actors/{index}"), id.clone());
            nodes.push(AuthoringNode {
                id,
                parent_id: Some(root_id.clone()),
                name: object
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Actor {}", index + 1)),
                transform: Transform {
                    position: scene_vector(object.get("position"), [0.0, 0.0, 0.0]),
                    rotation: [0.0, yaw as f32, 0.0],
                    ..Transform::default()
                },
                components: BTreeMap::from([("actor".to_owned(), Value::Object(component))]),
                editor: EditorMetadata::default(),
                source: None,
            });
        }
    }
    for (collection, component_name, node_prefix, display_name) in [
        ("ladders", "ladder", "ladder", "Ladder"),
        ("checkpoints", "checkpoint", "checkpoint", "Checkpoint"),
        ("hazards", "hazard", "hazard", "Hazard"),
        ("safeZones", "safeZone", "safe-zone", "Safe Zone"),
    ] {
        if let Some(items) = manifest_collection(world, collection)? {
            for (index, item) in items.iter().enumerate() {
                let object = manifest_collection_object(item, collection, index)?;
                let id = object
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| format!("{node_prefix}-{id}"))
                    .unwrap_or_else(|| format!("{node_prefix}-{}", index + 1));
                let mut component = object.clone();
                component.remove("position");
                match component_name {
                    "ladder" => {
                        component
                            .entry("size".to_owned())
                            .or_insert_with(|| serde_json::json!([1.5, 6, 0.8]));
                    }
                    "checkpoint" => {
                        component
                            .entry("radius".to_owned())
                            .or_insert_with(|| serde_json::json!(2.7));
                    }
                    "hazard" if !component.contains_key("size") => {
                        return Err(format!(
                            "cannot migrate hazard {} without an explicit positive size",
                            index + 1
                        ));
                    }
                    "safeZone" => {
                        component
                            .entry("radius".to_owned())
                            .or_insert_with(|| serde_json::json!(5));
                    }
                    _ => {}
                }
                target_map.insert(format!("world/{world_id}/{collection}/{index}"), id.clone());
                nodes.push(AuthoringNode {
                    id,
                    parent_id: Some(root_id.clone()),
                    name: format!("{display_name} {}", index + 1),
                    transform: Transform {
                        position: scene_vector(object.get("position"), [0.0, 0.0, 0.0]),
                        ..Transform::default()
                    },
                    components: BTreeMap::from([(
                        component_name.to_owned(),
                        Value::Object(component),
                    )]),
                    editor: EditorMetadata::default(),
                    source: None,
                });
            }
        }
    }

    let scene = AuthoringScene {
        format_version: 1,
        world_id: Some(world_id.clone()),
        nodes,
    };
    scene.validate()?;
    let world = manifest_world_mut(&mut manifest, &world_id)?
        .as_object_mut()
        .ok_or_else(|| format!("scene world `{world_id}` is not a JSON object"))?;
    for collection in [
        "blocks",
        "decorations",
        "signs",
        "interactions",
        "actors",
        "ladders",
        "checkpoints",
        "hazards",
        "safeZones",
    ] {
        world.remove(collection);
    }
    Ok(ManifestSceneMigration {
        scene,
        cleaned_manifest: manifest,
        target_map,
    })
}

pub(crate) fn retarget_migrated_scene_edit(
    request: &mut SceneEditRequest,
    target_map: &BTreeMap<String, String>,
) {
    let target = match request {
        SceneEditRequest::SetTransform { target, .. }
        | SceneEditRequest::SetPrimitiveSize { target, .. }
        | SceneEditRequest::DuplicateObject { target }
        | SceneEditRequest::DeleteObject { target }
        | SceneEditRequest::UpdateSignText { target, .. }
        | SceneEditRequest::UpdateProperty { target, .. }
        | SceneEditRequest::RemoveProperty { target, .. }
        | SceneEditRequest::RenameObject { target, .. } => target,
        SceneEditRequest::ReparentObjects { targets, .. }
        | SceneEditRequest::GroupObjects { targets } => {
            for target in targets {
                if let Some(migrated_target) = target_map.get(target) {
                    *target = migrated_target.clone();
                }
            }
            return;
        }
        SceneEditRequest::AddObject { .. } | SceneEditRequest::UseImageAsFloor { .. } => return,
    };
    if let Some(migrated_target) = target_map.get(target) {
        *target = migrated_target.clone();
    }
}

fn manifest_collection<'a>(
    world: &'a Value,
    collection: &str,
) -> Result<Option<&'a Vec<Value>>, String> {
    world
        .get(collection)
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| format!("manifest collection `{collection}` must be an array"))
        })
        .transpose()
}

fn manifest_collection_object<'a>(
    value: &'a Value,
    collection: &str,
    index: usize,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    value.as_object().ok_or_else(|| {
        format!(
            "manifest {collection} entry {} must be a JSON object",
            index + 1
        )
    })
}

fn scene_vector(value: Option<&Value>, fallback: [f32; 3]) -> [f32; 3] {
    let Some(values) = value.and_then(Value::as_array) else {
        return fallback;
    };
    let Some(values) = values.iter().map(Value::as_f64).collect::<Option<Vec<_>>>() else {
        return fallback;
    };
    values
        .get(..3)
        .and_then(|values| values.try_into().ok())
        .map(|values: [f64; 3]| values.map(|value| value as f32))
        .unwrap_or(fallback)
}

pub(crate) fn active_manifest_world_id(manifest: &Value) -> String {
    manifest
        .pointer("/launch/destinationWorld")
        .and_then(Value::as_str)
        .or_else(|| manifest.get("startWorld").and_then(Value::as_str))
        .unwrap_or("lobby")
        .to_owned()
}

pub(crate) fn manifest_world_mut<'a>(
    manifest: &'a mut Value,
    world_id: &str,
) -> Result<&'a mut Value, String> {
    if world_id == "lobby" {
        return Ok(manifest);
    }
    manifest
        .get_mut("worlds")
        .and_then(Value::as_object_mut)
        .and_then(|worlds| worlds.get_mut(world_id))
        .ok_or_else(|| format!("scene world `{world_id}` was not found"))
}

pub(crate) fn ensure_scene_collection<'a>(
    world: &'a mut Value,
    collection: &str,
) -> Result<&'a mut Vec<Value>, String> {
    let world = world
        .as_object_mut()
        .ok_or_else(|| "scene world is not a JSON object".to_owned())?;
    let value = world
        .entry(collection.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
    value
        .as_array_mut()
        .ok_or_else(|| format!("scene collection `{collection}` is not an array"))
}

pub(crate) fn default_scene_object(kind: SceneObjectKind, index: usize) -> Value {
    let number = index + 1;
    let x = index as f32 * 5.0;
    match kind {
        SceneObjectKind::Block => serde_json::json!({
            "id": format!("block-{number}"),
            "position": [x, 1, 0],
            "size": [4, 1, 4],
            "color": "signal"
        }),
        SceneObjectKind::Sign => serde_json::json!({
            "text": format!("New sign {number}"),
            "position": [0, 2, 0],
            "maxWidth": 5,
            "color": "paper"
        }),
        SceneObjectKind::Ladder => serde_json::json!({
            "id": format!("ladder-{number}"),
            "position": [0, 3, 0],
            "size": [2, 6, 1],
            "climbAxis": "z",
            "color": "signal"
        }),
        SceneObjectKind::Actor => serde_json::json!({
            "id": format!("actor-{number}"),
            "name": format!("Actor {number}"),
            "position": [x, 0, 0],
            "yaw": 0,
            "appearance": {
                "skin": "#E8AE86",
                "shirt": "#4C3F91",
                "pants": "#24365A",
                "shoes": "#19343A"
            }
        }),
        SceneObjectKind::Interaction => serde_json::json!({
            "id": format!("interaction-{number}"),
            "label": format!("Interaction {number}"),
            "kind": "zone",
            "position": [0, 1, 0],
            "radius": 3,
            "color": "signal"
        }),
        SceneObjectKind::Checkpoint => serde_json::json!({
            "id": format!("checkpoint-{number}"),
            "position": [0, 1, 0],
            "radius": 3
        }),
        SceneObjectKind::Hazard => serde_json::json!({
            "id": format!("hazard-{number}"),
            "kind": "damage",
            "position": [0, 0.5, 0],
            "size": [4, 1, 4],
            "damagePerSecond": 10
        }),
        SceneObjectKind::SafeZone => serde_json::json!({
            "id": format!("safe-zone-{number}"),
            "position": [0, 1, 0],
            "radius": 5,
            "healPerSecond": 10
        }),
        SceneObjectKind::Group => Value::Null,
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn every_insertable_scene_kind_has_a_valid_position_and_collection() {
        let mut world = serde_json::json!({});
        for kind in SceneObjectKind::MANIFEST_KINDS {
            let collection = kind.collection().expect("manifest collection");
            ensure_scene_collection(&mut world, collection)
                .expect("collection")
                .push(default_scene_object(kind, 0));
            let object = &world[collection][0];
            assert_eq!(object["position"].as_array().map(Vec::len), Some(3));
        }

        assert_eq!(world["blocks"][0]["size"], serde_json::json!([4, 1, 4]));
        assert_eq!(world["ladders"][0]["climbAxis"], "z");
        assert_eq!(world["interactions"][0]["kind"], "zone");
        assert_eq!(world["hazards"][0]["kind"], "damage");
        assert_eq!(world["safeZones"][0]["radius"], 5);
        assert_ne!(
            default_scene_object(SceneObjectKind::Block, 0)["position"],
            default_scene_object(SceneObjectKind::Block, 1)["position"]
        );
    }

    #[test]
    fn manifest_migration_places_blocks_in_primitive_scene_nodes() {
        let source = serde_json::json!({
            "launch": { "destinationWorld": "course" },
            "worlds": {
                "course": {
                    "blocks": [{
                        "id": "platform",
                        "position": [2, 1, 3],
                        "size": [4, 1, 4],
                        "color": "signal"
                    }]
                }
            }
        })
        .to_string();
        let migrated = migrate_manifest_to_scene(&source).expect("scene migration");
        assert_eq!(
            migrated.target_map["world/course/blocks/0"],
            "block-platform"
        );
        let scene = migrated.scene;
        let block = scene
            .nodes
            .iter()
            .find(|node| node.components.contains_key("primitive"))
            .expect("primitive block");
        assert_eq!(block.transform.position, [2.0, 1.0, 3.0]);
        assert_eq!(
            block.components["primitive"]["size"],
            serde_json::json!([4, 1, 4])
        );
        assert_eq!(
            block.components["primitive"]["runtimeId"],
            serde_json::json!("platform")
        );
        assert!(
            migrated.cleaned_manifest["worlds"]["course"]
                .get("blocks")
                .is_none()
        );
    }

    #[test]
    fn manifest_migration_preserves_authored_actors() {
        let source = serde_json::json!({
            "startWorld": "course",
            "worlds": {
                "course": {
                    "actors": [{
                        "id": "guide",
                        "name": "Wizard Guide",
                        "position": [3, 0, 5],
                        "yaw": 0.75,
                        "appearance": {"shirt": "#4C3F91"}
                    }]
                }
            }
        })
        .to_string();

        let migrated = migrate_manifest_to_scene(&source).unwrap();
        let actor = migrated.scene.node("actor-guide").unwrap();
        assert_eq!(actor.name, "Wizard Guide");
        assert_eq!(actor.transform.position, [3.0, 0.0, 5.0]);
        assert_eq!(actor.transform.rotation, [0.0, 0.75, 0.0]);
        assert_eq!(actor.components["actor"]["id"], "guide");
        assert!(
            migrated.cleaned_manifest["worlds"]["course"]
                .get("actors")
                .is_none()
        );
    }

    #[test]
    fn manifest_migration_moves_every_supported_collection_and_preserves_configuration() {
        let source = serde_json::json!({
            "launch": { "destinationWorld": "course" },
            "worlds": {
                "course": {
                    "world": { "spawn": [0, 2, 0] },
                    "blocks": [{
                        "id": "platform",
                        "position": [2, 1, 3],
                        "size": [4, 1, 4],
                        "color": "signal",
                        "material": "floor",
                        "outline": false
                    }],
                    "decorations": [{
                        "kind": "mesh",
                        "asset": "tree",
                        "position": [3, 0, 4],
                        "scale": 2,
                        "yaw": 0.5,
                        "color": "#FFFFFF"
                    }],
                    "signs": [{
                        "text": "HELLO",
                        "position": [0, 2, 0],
                        "yaw": 0.75,
                        "maxWidth": 6,
                        "color": "paper"
                    }],
                    "interactions": [{
                        "id": "use",
                        "label": "USE",
                        "position": [0, 1, 0],
                        "radius": 4
                    }],
                    "ladders": [{
                        "id": "up",
                        "position": [1, 3, 1],
                        "size": [2, 6, 1],
                        "climbAxis": "z"
                    }],
                    "checkpoints": [{
                        "id": "save",
                        "position": [2, 1, 2],
                        "radius": 3
                    }],
                    "hazards": [{
                        "id": "hurt",
                        "kind": "damage",
                        "position": [3, 0.5, 3],
                        "size": [4, 1, 4],
                        "damagePerSecond": 10
                    }],
                    "safeZones": [{
                        "id": "camp",
                        "position": [4, 1, 4],
                        "radius": 5,
                        "healPerSecond": 4
                    }]
                }
            }
        })
        .to_string();

        let migrated = migrate_manifest_to_scene(&source).expect("lossless migration");
        let cleaned_world = &migrated.cleaned_manifest["worlds"]["course"];
        assert_eq!(
            cleaned_world["world"]["spawn"],
            serde_json::json!([0, 2, 0])
        );
        for collection in [
            "blocks",
            "decorations",
            "signs",
            "interactions",
            "ladders",
            "checkpoints",
            "hazards",
            "safeZones",
        ] {
            assert!(
                cleaned_world.get(collection).is_none(),
                "{collection} remained"
            );
        }

        let mut compiled = migrated.cleaned_manifest.clone();
        migrated
            .scene
            .compile_into_manifest(&mut compiled)
            .expect("migrated scene compiles");
        let world = &compiled["worlds"]["course"];
        assert_eq!(world["blocks"][0]["id"], "platform");
        assert_eq!(world["blocks"][0]["material"], "floor");
        assert_eq!(world["blocks"][0]["outline"], false);
        assert_eq!(world["decorations"][0]["asset"], "tree");
        assert!((world["signs"][0]["yaw"].as_f64().unwrap() - 0.75).abs() < 0.0001);
        assert_eq!(world["interactions"][0]["id"], "use");
        assert_eq!(world["ladders"][0]["id"], "up");
        assert_eq!(world["checkpoints"][0]["id"], "save");
        assert_eq!(world["hazards"][0]["id"], "hurt");
        assert_eq!(world["safeZones"][0]["id"], "camp");
    }

    #[test]
    fn manifest_migration_refuses_legacy_decorations_it_cannot_preserve() {
        let source = serde_json::json!({
            "launch": { "destinationWorld": "course" },
            "worlds": {
                "course": {
                    "decorations": [{ "kind": "rock", "position": [0, 0, 0] }]
                }
            }
        })
        .to_string();

        let error = migrate_manifest_to_scene(&source).expect_err("migration must be lossless");
        assert!(error.contains("without an asset-backed mesh"));
    }

    #[test]
    fn first_legacy_object_edit_is_retargeted_to_the_migrated_scene_node() {
        let mut request = SceneEditRequest::DeleteObject {
            target: "world/course/blocks/0".to_owned(),
        };
        retarget_migrated_scene_edit(
            &mut request,
            &BTreeMap::from([(
                "world/course/blocks/0".to_owned(),
                "block-platform".to_owned(),
            )]),
        );

        let SceneEditRequest::DeleteObject { target } = request else {
            panic!("request kind changed during retargeting");
        };
        assert_eq!(target, "block-platform");
    }
}
