use super::*;

pub(super) const NEW_BLOCK_SIZE: [f32; 3] = [2.0, 2.0, 2.0];
pub(super) const NEW_BLOCK_GAP: f32 = 0.5;

pub(super) fn next_block_position(scene: &AuthoringScene) -> [f32; 3] {
    let blocks = scene
        .nodes
        .iter()
        .filter_map(|node| {
            primitive_size(node).map(|size| {
                let scale = node.transform.scale;
                (
                    node.transform.position,
                    [size[0] * scale[0].abs(), size[2] * scale[2].abs()],
                )
            })
        })
        .collect::<Vec<_>>();
    for radius in 0..=32 {
        let radius = radius as f32;
        let mut candidates = if radius == 0.0 {
            vec![[0.0, 0.0]]
        } else {
            vec![[radius, 0.0], [-radius, 0.0], [0.0, radius], [0.0, -radius]]
        };
        if radius > 0.0 {
            let edge = radius as i32;
            for x in -edge..=edge {
                for z in -edge..=edge {
                    if x != 0 && z != 0 && (x.abs() == edge || z.abs() == edge) {
                        candidates.push([x as f32, z as f32]);
                    }
                }
            }
        }
        for [grid_x, grid_z] in candidates {
            let candidate = [grid_x * 2.5, NEW_BLOCK_SIZE[1] * 0.5, grid_z * 2.5];
            let clear = blocks.iter().all(|(position, size)| {
                let required_x = (NEW_BLOCK_SIZE[0] + size[0]) * 0.5 + NEW_BLOCK_GAP;
                let required_z = (NEW_BLOCK_SIZE[2] + size[1]) * 0.5 + NEW_BLOCK_GAP;
                (candidate[0] - position[0]).abs() >= required_x
                    || (candidate[2] - position[2]).abs() >= required_z
            });
            if clear {
                return candidate;
            }
        }
    }
    [blocks.len() as f32 * 2.5, NEW_BLOCK_SIZE[1] * 0.5, 0.0]
}

pub(super) fn primitive_size(node: &AuthoringNode) -> Option<[f32; 3]> {
    let size = node
        .components
        .get("primitive")?
        .as_object()?
        .get("size")?
        .as_array()?;
    Some([
        size.first()?.as_f64()? as f32,
        size.get(1)?.as_f64()? as f32,
        size.get(2)?.as_f64()? as f32,
    ])
}

pub(super) fn offset_duplicate(node: &mut AuthoringNode) {
    let offset = primitive_size(node)
        .map(|size| size[0] * node.transform.scale[0].abs())
        .unwrap_or(1.0)
        + NEW_BLOCK_GAP;
    node.transform.position[0] += offset;
}

pub(super) fn top_level_scene_targets(scene: &AuthoringScene, targets: &[String]) -> Vec<String> {
    let selected = targets
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    targets
        .iter()
        .filter(|target| {
            let Some(node) = scene.node(target) else {
                return false;
            };
            if node.parent_id.is_none() {
                return false;
            }
            let mut ancestor = node.parent_id.as_deref();
            while let Some(id) = ancestor {
                if selected.contains(id) {
                    return false;
                }
                ancestor = scene
                    .node(id)
                    .and_then(|candidate| candidate.parent_id.as_deref());
            }
            true
        })
        .cloned()
        .collect()
}

pub(super) fn scene_node_has_ancestor(
    scene: &AuthoringScene,
    node: &AuthoringNode,
    ancestor_id: &str,
) -> bool {
    let mut parent = node.parent_id.as_deref();
    while let Some(id) = parent {
        if id == ancestor_id {
            return true;
        }
        parent = scene
            .node(id)
            .and_then(|candidate| candidate.parent_id.as_deref());
    }
    false
}

pub(super) fn record_scene_history(
    undo: &mut Vec<SceneHistoryEntry>,
    redo: &mut Vec<SceneHistoryEntry>,
    before: String,
    after: String,
    target: &str,
) {
    undo.push(SceneHistoryEntry {
        kind: SceneHistoryKind::Snapshot {
            before,
            after,
            target: target.to_owned(),
        },
    });
    redo.clear();
}

pub(super) fn set_authoring_component_property(
    node: &mut AuthoringNode,
    key: &str,
    value: Value,
) -> Result<(), String> {
    if let Some((component, nested_key)) = key.split_once('.') {
        let object = node
            .components
            .get_mut(component)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| format!("scene node {} has no {component} component", node.id))?;
        if component == "primitive" && matches!(nested_key, "color" | "material") {
            normalize_legacy_primitive_appearance(object);
        }
        if component == "actor" && matches!(nested_key, "skin" | "shirt" | "pants" | "shoes") {
            let nested = object
                .entry("appearance".to_owned())
                .or_insert_with(|| Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .ok_or_else(|| format!("scene node {} has invalid appearance", node.id))?;
            nested.insert(nested_key.to_owned(), value);
        } else {
            object.insert(nested_key.to_owned(), value);
        }
        return Ok(());
    }
    let matches = node
        .components
        .iter()
        .filter_map(|(component, value)| {
            value
                .as_object()
                .filter(|properties| properties.contains_key(key))
                .map(|_| component.clone())
        })
        .collect::<Vec<_>>();
    let component = match matches.as_slice() {
        [component] => component.clone(),
        [] => {
            return Err(format!(
                "scene node {} has no component property `{key}`",
                node.id
            ));
        }
        _ => {
            return Err(format!(
                "scene node {} has ambiguous component property `{key}`",
                node.id
            ));
        }
    };
    node.components
        .get_mut(&component)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            format!(
                "scene node {} has an invalid {component} component",
                node.id
            )
        })?
        .insert(key.to_owned(), value);
    Ok(())
}

pub(super) fn remove_authoring_component_property(
    node: &mut AuthoringNode,
    key: &str,
) -> Result<Value, String> {
    let (component, property) = key
        .split_once('.')
        .ok_or_else(|| format!("component property path `{key}` is required"))?;
    let component = node
        .components
        .get_mut(component)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("scene node {} has no component `{component}`", node.id))?;
    if component.contains_key("shape") && matches!(property, "color" | "material") {
        normalize_legacy_primitive_appearance(component);
    }
    component
        .remove(property)
        .ok_or_else(|| format!("scene node {} has no component property `{key}`", node.id))
}

pub(super) fn normalize_legacy_primitive_appearance(
    primitive: &mut serde_json::Map<String, Value>,
) {
    if primitive.contains_key("color") {
        return;
    }
    let legacy_color = primitive.remove("material");
    let legacy_surface = primitive.remove("runtimeMaterial");
    primitive.insert(
        "color".to_owned(),
        legacy_color.unwrap_or_else(|| Value::String("#FFFFFF".to_owned())),
    );
    if let Some(material) = legacy_surface {
        primitive.insert("material".to_owned(), material);
    }
}

pub(super) fn is_roblox_source_linked(node: &AuthoringNode) -> bool {
    node.source
        .as_ref()
        .is_some_and(|source| source.format == "roblox")
}
