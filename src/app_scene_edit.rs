use super::*;
use crate::app_scene_migration::{
    active_manifest_world_id, default_scene_object, ensure_scene_collection, manifest_world_mut,
    migrate_manifest_to_scene, retarget_migrated_scene_edit,
};
use cubacadabra_scene::{
    AuthoringNode, AuthoringScene, EditorMetadata, Transform, parse_authoring_scene,
    serialize_authoring_scene,
};
use serde_json::Value;

mod history;
mod support;
use support::*;

impl StudioApp {
    pub(crate) fn apply_scene_edit(&mut self, mut request: SceneEditRequest) -> Result<(), String> {
        if !self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable)
        {
            return Err("Open a raw source project to edit scene objects.".to_owned());
        }
        if self.authored_scene_source.is_none() {
            let migrated = migrate_manifest_to_scene(&self.authored_manifest_source)?;
            let scene_source = serialize_authoring_scene(&migrated.scene)?;
            let manifest_source = serde_json::to_string_pretty(&migrated.cleaned_manifest)
                .map_err(|error| format!("could not serialize the migrated manifest: {error}"))?
                + "\n";
            retarget_migrated_scene_edit(&mut request, &migrated.target_map);
            self.authored_manifest_source = manifest_source.clone();
            self.authored_scene_source = Some(scene_source.clone());
            self.authoring_scene = Some(migrated.scene.clone());
            self.authoring_scene_indices = authoring_scene_indices(&migrated.scene);
            if let Some(shell) = &mut self.shell {
                shell.set_source_manifest(&manifest_source, true);
                shell.set_source_scene(Some(&scene_source), true);
                shell.set_notice("Upgraded this project to the scene format".to_owned());
            }
        }
        if matches!(
            request,
            SceneEditRequest::SetTransform { .. } | SceneEditRequest::SetPrimitiveSize { .. }
        ) {
            return self.apply_live_geometry_edit(&request);
        }
        let scene_source = self
            .authoring_scene
            .as_ref()
            .map(serialize_authoring_scene)
            .transpose()?
            .or_else(|| self.authored_scene_source.clone());
        if let Some(scene_source) = scene_source {
            match &request {
                SceneEditRequest::AddObject { kind, .. } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    let root_id = scene
                        .nodes
                        .iter()
                        .find(|node| node.parent_id.is_none())
                        .map(|node| node.id.clone())
                        .ok_or_else(|| "component scene has no world root".to_owned())?;
                    let (parent_id, base_id, name, component_name, components, position) =
                        match kind {
                            SceneObjectKind::Block => (
                                root_id.clone(),
                                "block".to_owned(),
                                "Block".to_owned(),
                                "primitive".to_owned(),
                                serde_json::json!({
                                    "shape": "box",
                                    "size": [2, 2, 2],
                                    "color": "signal"
                                }),
                                next_block_position(&scene),
                            ),
                            SceneObjectKind::Group => (
                                root_id.clone(),
                                "group".to_owned(),
                                "Group".to_owned(),
                                String::new(),
                                serde_json::json!({}),
                                [0.0, 0.0, 0.0],
                            ),
                            SceneObjectKind::Sign => (
                                scene
                                    .node("signs")
                                    .map_or_else(|| root_id.clone(), |node| node.id.clone()),
                                "sign-new".to_owned(),
                                "New Sign".to_owned(),
                                "text".to_owned(),
                                serde_json::json!({
                                    "text": "New sign",
                                    "maxWidth": 5,
                                    "color": "paper"
                                }),
                                [0.0, 2.0, 0.0],
                            ),
                            SceneObjectKind::Ladder => (
                                root_id.clone(),
                                "ladder-new".to_owned(),
                                "New Ladder".to_owned(),
                                "ladder".to_owned(),
                                serde_json::json!({
                                    "id": "ladder-new",
                                    "size": [2, 6, 1],
                                    "climbAxis": "z",
                                    "color": "signal"
                                }),
                                [0.0, 3.0, 0.0],
                            ),
                            SceneObjectKind::Actor => (
                                root_id.clone(),
                                "actor".to_owned(),
                                "Actor 1".to_owned(),
                                "actor".to_owned(),
                                serde_json::json!({
                                    "id": "actor",
                                    "name": "Actor 1",
                                    "yaw": 0,
                                    "appearance": {
                                        "skin": "#E8AE86",
                                        "shirt": "#4C3F91",
                                        "pants": "#24365A",
                                        "shoes": "#19343A"
                                    }
                                }),
                                [0.0, 0.0, 0.0],
                            ),
                            SceneObjectKind::Interaction => (
                                scene
                                    .node("interactions")
                                    .map_or_else(|| root_id.clone(), |node| node.id.clone()),
                                "interaction-new".to_owned(),
                                "New Interaction".to_owned(),
                                "interaction".to_owned(),
                                serde_json::json!({
                                    "id": "interaction-new",
                                    "kind": "zone",
                                    "label": "New Interaction",
                                    "radius": 3,
                                    "color": "signal",
                                    "visual": "checkpoint"
                                }),
                                [0.0, 1.0, 0.0],
                            ),
                            SceneObjectKind::Checkpoint => (
                                root_id.clone(),
                                "checkpoint-new".to_owned(),
                                "New Checkpoint".to_owned(),
                                "checkpoint".to_owned(),
                                serde_json::json!({
                                    "id": "checkpoint-new",
                                    "radius": 3
                                }),
                                [0.0, 1.0, 0.0],
                            ),
                            SceneObjectKind::Hazard => (
                                root_id.clone(),
                                "hazard-new".to_owned(),
                                "New Hazard".to_owned(),
                                "hazard".to_owned(),
                                serde_json::json!({
                                    "id": "hazard-new",
                                    "kind": "damage",
                                    "size": [4, 1, 4],
                                    "damagePerSecond": 10
                                }),
                                [0.0, 0.5, 0.0],
                            ),
                            SceneObjectKind::SafeZone => (
                                root_id.clone(),
                                "safe-zone-new".to_owned(),
                                "New Safe Zone".to_owned(),
                                "safeZone".to_owned(),
                                serde_json::json!({
                                    "id": "safe-zone-new",
                                    "radius": 5,
                                    "healPerSecond": 10
                                }),
                                [0.0, 1.0, 0.0],
                            ),
                        };
                    if scene.node(&parent_id).is_none() {
                        return Err(format!("component scene group {parent_id:?} was not found"));
                    }
                    let mut number = 1;
                    let id = loop {
                        let candidate = format!("{base_id}-{number}");
                        if scene.node(&candidate).is_none() {
                            break candidate;
                        }
                        number += 1;
                    };
                    let display_name = format!("{name} {number}");
                    let mut component = components;
                    if let Some(object) = component.as_object_mut() {
                        if object.contains_key("id") {
                            object.insert("id".to_owned(), Value::String(id.clone()));
                        }
                        if component_name == "interaction" {
                            object.insert("label".to_owned(), Value::String(display_name.clone()));
                        } else if component_name == "actor" {
                            object.insert("name".to_owned(), Value::String(display_name.clone()));
                        }
                    }
                    let mut component_map = BTreeMap::new();
                    if !component_name.is_empty() {
                        component_map.insert(component_name.to_owned(), component);
                    }
                    scene.nodes.push(AuthoringNode {
                        id: id.clone(),
                        parent_id: Some(parent_id),
                        name: display_name,
                        transform: Transform {
                            position,
                            ..Transform::default()
                        },
                        components: component_map,
                        editor: EditorMetadata::default(),
                        source: None,
                    });
                    let updated = serialize_authoring_scene(&scene)?;
                    let notice = if *kind == SceneObjectKind::Block {
                        "Block added — Craft mode is ready; right-click to switch tools".to_owned()
                    } else {
                        format!("{} added — press Play to preview it", kind.label())
                    };
                    self.commit_authoring_scene_transaction(scene_source, updated, &id, &notice);
                    if *kind == SceneObjectKind::Block
                        && let Some(shell) = &mut self.shell
                    {
                        shell.set_playing(false);
                        shell.set_scene_tool(SceneTool::Craft);
                    }
                    return Ok(());
                }
                SceneEditRequest::SetTransform {
                    target,
                    position,
                    rotation,
                    scale,
                } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if scene.node(target).is_some() {
                        scene.set_position(target, *position)?;
                        if let Some(rotation) = rotation {
                            let node = scene
                                .node_mut(target)
                                .expect("scene node existed before transform update");
                            if rotation.iter().any(|value| !value.is_finite()) {
                                return Err(format!(
                                    "scene node {target} rotation must contain finite values"
                                ));
                            }
                            node.transform.rotation = *rotation;
                        }
                        if let Some(scale) = scale {
                            scene.set_scale(target, *scale)?;
                        }
                        let updated = serialize_authoring_scene(&scene)?;
                        self.commit_authoring_scene_transaction(
                            scene_source,
                            updated,
                            target,
                            "Position changed — save to keep it",
                        );
                        return Ok(());
                    }
                }
                SceneEditRequest::SetPrimitiveSize {
                    target,
                    position,
                    size,
                } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if let Some(node) = scene.node_mut(target) {
                        if node.editor.locked {
                            return Err(format!("scene node {target} is locked"));
                        }
                        node.transform.position = *position;
                        set_authoring_component_property(node, "size", serde_json::json!(size))?;
                        let updated = serialize_authoring_scene(&scene)?;
                        self.commit_authoring_scene_transaction(
                            scene_source,
                            updated,
                            target,
                            "Primitive size changed — save to keep it",
                        );
                        return Ok(());
                    }
                }
                SceneEditRequest::UpdateSignText { target, text } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if let Some(node) = scene.node_mut(target) {
                        if node.editor.locked {
                            return Err(format!("scene node {target} is locked"));
                        }
                        let component = node
                            .components
                            .get_mut("text")
                            .and_then(Value::as_object_mut)
                            .ok_or_else(|| format!("scene node {target} has no text component"))?;
                        component.insert("text".to_owned(), Value::String(text.clone()));
                        let updated = serialize_authoring_scene(&scene)?;
                        self.commit_authoring_scene_transaction(
                            scene_source,
                            updated,
                            target,
                            "Sign text changed — save to keep it",
                        );
                        return Ok(());
                    }
                }
                SceneEditRequest::UpdateProperty { target, key, value } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if let Some(node) = scene.node_mut(target) {
                        if node.editor.locked {
                            return Err(format!("scene node {target} is locked"));
                        }
                        set_authoring_component_property(node, key, value.clone())?;
                        let updated = serialize_authoring_scene(&scene)?;
                        self.commit_authoring_scene_transaction(
                            scene_source,
                            updated,
                            target,
                            "Property changed — save to keep it",
                        );
                        return Ok(());
                    }
                }
                SceneEditRequest::RemoveProperty { target, key } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if let Some(node) = scene.node_mut(target) {
                        if node.editor.locked {
                            return Err(format!("scene node {target} is locked"));
                        }
                        remove_authoring_component_property(node, key)?;
                        let updated = serialize_authoring_scene(&scene)?;
                        self.commit_authoring_scene_transaction(
                            scene_source,
                            updated,
                            target,
                            "Property reset — save to keep it",
                        );
                        return Ok(());
                    }
                }
                SceneEditRequest::RenameObject { target, name } => {
                    let name = name.trim();
                    if name.is_empty() {
                        return Err("Scene object name cannot be empty".to_owned());
                    }
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if let Some(node) = scene.node_mut(target) {
                        if node.editor.locked {
                            return Err(format!("scene node {target} is locked"));
                        }
                        node.name = name.to_owned();
                        if let Some(actor) = node
                            .components
                            .get_mut("actor")
                            .and_then(Value::as_object_mut)
                        {
                            actor.insert("name".to_owned(), Value::String(name.to_owned()));
                        }
                        let updated = serialize_authoring_scene(&scene)?;
                        self.commit_authoring_scene_transaction(
                            scene_source,
                            updated,
                            target,
                            "Scene object renamed — save to keep it",
                        );
                        return Ok(());
                    }
                }
                SceneEditRequest::ReparentObjects { targets, parent_id } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    let targets = top_level_scene_targets(&scene, targets);
                    if targets.is_empty() {
                        return Err("Select at least one movable scene object".to_owned());
                    }
                    for target in &targets {
                        if scene
                            .node(target)
                            .and_then(|node| node.parent_id.as_deref())
                            != Some(parent_id)
                        {
                            scene.reparent_preserving_world_transform(target, parent_id)?;
                        }
                    }
                    let updated = serialize_authoring_scene(&scene)?;
                    let primary = targets.last().expect("targets are not empty").clone();
                    self.commit_authoring_scene_transaction(
                        scene_source,
                        updated,
                        &primary,
                        "Scene object moved in the hierarchy — save to keep it",
                    );
                    if let Some(shell) = &mut self.shell {
                        shell.select_scene_nodes(&targets);
                    }
                    return Ok(());
                }
                SceneEditRequest::GroupObjects { targets } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    let targets = top_level_scene_targets(&scene, targets);
                    if targets.is_empty() {
                        return Err("Select at least one movable scene object".to_owned());
                    }
                    let root_id = scene
                        .nodes
                        .iter()
                        .find(|node| node.parent_id.is_none())
                        .map(|node| node.id.clone())
                        .ok_or_else(|| "component scene has no world root".to_owned())?;
                    let world_positions = targets
                        .iter()
                        .map(|target| scene.world_transform(target).map(|world| world.position))
                        .collect::<Result<Vec<_>, _>>()?;
                    let count = world_positions.len() as f32;
                    let center = world_positions
                        .iter()
                        .fold([0.0; 3], |mut center, position| {
                            for axis in 0..3 {
                                center[axis] += position[axis] / count;
                            }
                            center
                        });
                    let mut number = 1;
                    let group_id = loop {
                        let candidate = format!("group-{number}");
                        if scene.node(&candidate).is_none() {
                            break candidate;
                        }
                        number += 1;
                    };
                    scene.nodes.push(AuthoringNode {
                        id: group_id.clone(),
                        parent_id: Some(root_id),
                        name: format!("Group {number}"),
                        transform: Transform::default(),
                        components: BTreeMap::new(),
                        editor: EditorMetadata::default(),
                        source: None,
                    });
                    let local_center = scene.local_position_for_world(&group_id, center)?;
                    scene.set_position(&group_id, local_center)?;
                    for target in &targets {
                        scene.reparent_preserving_world_transform(target, &group_id)?;
                    }
                    let updated = serialize_authoring_scene(&scene)?;
                    self.commit_authoring_scene_transaction(
                        scene_source,
                        updated,
                        &group_id,
                        "Selection grouped — save to keep it",
                    );
                    return Ok(());
                }
                SceneEditRequest::DuplicateObject { target } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if let Some(node) = scene.node(target).cloned() {
                        if node.editor.locked {
                            return Err(format!("scene node {target} is locked"));
                        }
                        let subtree = scene
                            .nodes
                            .iter()
                            .filter(|candidate| {
                                candidate.id == node.id
                                    || scene_node_has_ancestor(&scene, candidate, &node.id)
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        let mut number = 1;
                        let mapping = loop {
                            let mapping = subtree
                                .iter()
                                .map(|candidate| {
                                    (
                                        candidate.id.clone(),
                                        format!("{}-copy-{number}", candidate.id),
                                    )
                                })
                                .collect::<BTreeMap<_, _>>();
                            if mapping.values().all(|id| scene.node(id).is_none()) {
                                break mapping;
                            }
                            number += 1;
                        };
                        let id = mapping[&node.id].clone();
                        let mut copies = subtree
                            .into_iter()
                            .map(|mut copy| {
                                let source_id = copy.id.clone();
                                copy.id = mapping[&source_id].clone();
                                if source_id == node.id {
                                    copy.name = format!("{} Copy {number}", copy.name);
                                    offset_duplicate(&mut copy);
                                }
                                // A duplicate is new authored content. Keeping the
                                // original Roblox source link would make both scene
                                // nodes edit the same preserved DOM instance.
                                copy.source = None;
                                if let Some(parent_id) = copy.parent_id.as_deref()
                                    && let Some(remapped) = mapping.get(parent_id)
                                {
                                    copy.parent_id = Some(remapped.clone());
                                }
                                for component in copy.components.values_mut() {
                                    if let Some(component) = component.as_object_mut() {
                                        for key in ["id", "runtimeId"] {
                                            if component.contains_key(key) {
                                                component.insert(
                                                    key.to_owned(),
                                                    Value::String(copy.id.clone()),
                                                );
                                            }
                                        }
                                    }
                                }
                                if source_id == node.id
                                    && let Some(actor) = copy
                                        .components
                                        .get_mut("actor")
                                        .and_then(Value::as_object_mut)
                                {
                                    actor.insert(
                                        "name".to_owned(),
                                        Value::String(copy.name.clone()),
                                    );
                                }
                                copy
                            })
                            .collect::<Vec<_>>();
                        scene.nodes.append(&mut copies);
                        let updated = serialize_authoring_scene(&scene)?;
                        self.commit_authoring_scene_transaction(
                            scene_source,
                            updated,
                            &id,
                            "Copy added beside the original — drag it into place",
                        );
                        if let Some(shell) = &mut self.shell {
                            shell.set_playing(false);
                        }
                        return Ok(());
                    }
                }
                SceneEditRequest::DeleteObject { target } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if let Some(node) = scene.node(target) {
                        if is_roblox_source_linked(node) {
                            return Err(
                                "Deleting imported Roblox objects is not supported yet; the source is preserved for export"
                                    .to_owned(),
                            );
                        }
                        if node.editor.locked {
                            return Err(format!("scene node {target} is locked"));
                        }
                        if scene
                            .nodes
                            .iter()
                            .any(|candidate| candidate.parent_id.as_deref() == Some(target))
                        {
                            return Err(format!(
                                "scene node {target} has children; reparent or delete them first"
                            ));
                        }
                        let parent_id = node.parent_id.clone().unwrap_or_else(|| target.clone());
                        scene.nodes.retain(|node| node.id != *target);
                        let updated = serialize_authoring_scene(&scene)?;
                        self.commit_authoring_scene_transaction(
                            scene_source,
                            updated,
                            &parent_id,
                            "Scene object deleted — save to keep it",
                        );
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        self.invalidate_scene_history();
        let mut manifest: Value = serde_json::from_str(&self.authored_manifest_source)
            .map_err(|error| format!("manifest is no longer valid JSON: {error}"))?;
        if let SceneEditRequest::UpdateSignText {
            ref target,
            ref text,
        } = request
        {
            update_manifest_sign_text(&mut manifest, target, text.clone())?;
            let source = serde_json::to_string_pretty(&manifest)
                .map_err(|error| format!("could not serialize the scene manifest: {error}"))?
                + "\n";
            self.authored_manifest_source = source.clone();
            if let Some(shell) = &mut self.shell {
                shell.set_source_manifest(&source, true);
                shell.set_notice("Sign text changed — save to keep it".to_owned());
            }
            return Ok(());
        }
        if let SceneEditRequest::UseImageAsFloor { asset_path } = &request {
            let (_, world_id) = set_image_as_ground_material(&mut manifest, asset_path)?;
            let source = serde_json::to_string_pretty(&manifest)
                .map_err(|error| format!("could not serialize the scene manifest: {error}"))?
                + "\n";
            self.authored_manifest_source = source.clone();
            if let Some(shell) = &mut self.shell {
                shell.set_source_manifest(&source, true);
                shell.set_notice(format!(
                    "Floor changed in {world_id} — press Play to preview it"
                ));
            }
            return Ok(());
        }
        if let SceneEditRequest::AddObject { ref world_id, kind } = request {
            let world_id = world_id
                .clone()
                .unwrap_or_else(|| active_manifest_world_id(&manifest));
            let collection = kind
                .collection()
                .ok_or_else(|| format!("{} requires the authoring scene format", kind.label()))?;
            let world = manifest_world_mut(&mut manifest, &world_id)?;
            let items = ensure_scene_collection(world, collection)?;
            let index = items.len();
            items.push(default_scene_object(kind, index));
            let source = serde_json::to_string_pretty(&manifest)
                .map_err(|error| format!("could not serialize the scene manifest: {error}"))?
                + "\n";
            self.authored_manifest_source = source.clone();
            if let Some(shell) = &mut self.shell {
                shell.set_source_manifest(&source, true);
                let scene_id = format!("world/{world_id}/{collection}/{index}");
                shell.select_scene_node(&scene_id);
                if kind == SceneObjectKind::Block {
                    shell.set_playing(false);
                    shell.set_scene_tool(SceneTool::Craft);
                    shell.set_notice(
                        "Block added — Craft mode is ready; right-click to switch tools".to_owned(),
                    );
                } else {
                    shell.set_notice(format!("{} added — press Play to preview it", kind.label()));
                }
            }
            return Ok(());
        }
        let (target, operation) = match request {
            SceneEditRequest::AddObject { .. } => {
                return Err("new scene object edit was not handled".to_owned());
            }
            SceneEditRequest::UseImageAsFloor { .. } => {
                return Err("floor image edit was not handled".to_owned());
            }
            SceneEditRequest::SetTransform {
                target,
                position,
                rotation: None,
                scale: None,
            } => (target, SceneEditOperation::SetTransform { position }),
            SceneEditRequest::SetTransform { .. } => {
                return Err("rotation and scale edits require an authoring scene".to_owned());
            }
            SceneEditRequest::SetPrimitiveSize {
                target,
                position,
                size,
            } => (
                target,
                SceneEditOperation::SetPrimitiveSize { position, size },
            ),
            SceneEditRequest::DuplicateObject { target } => (target, SceneEditOperation::Duplicate),
            SceneEditRequest::DeleteObject { target } => (target, SceneEditOperation::Delete),
            SceneEditRequest::UpdateSignText { .. } => {
                return Err("sign text edit was not handled".to_owned());
            }
            SceneEditRequest::UpdateProperty { target, key, value } => {
                (target, SceneEditOperation::UpdateProperty { key, value })
            }
            SceneEditRequest::RemoveProperty { .. }
            | SceneEditRequest::RenameObject { .. }
            | SceneEditRequest::ReparentObjects { .. }
            | SceneEditRequest::GroupObjects { .. } => {
                return Err("this edit requires an authoring scene".to_owned());
            }
        };
        let (world_id, collection, index) = parse_scene_object_target(&target)?;
        let world = manifest_world_mut(&mut manifest, &world_id)?;
        let items = world
            .get_mut(&collection)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| format!("scene world `{world_id}` has no {collection}"))?;
        let object = items
            .get_mut(index)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| format!("scene object `{target}` was not found"))?;
        let mut selection_after_edit = None;
        match &operation {
            SceneEditOperation::SetTransform { position } => {
                object.insert("position".to_owned(), serde_json::json!(position));
            }
            SceneEditOperation::SetPrimitiveSize { position, size } => {
                object.insert("position".to_owned(), serde_json::json!(position));
                object.insert("size".to_owned(), serde_json::json!(size));
            }
            SceneEditOperation::UpdateProperty { key, value } => {
                object.insert(key.clone(), value.clone());
            }
            SceneEditOperation::Duplicate => {
                let mut copy = Value::Object(object.clone());
                let copy_object = copy
                    .as_object_mut()
                    .ok_or_else(|| "scene object could not be duplicated".to_owned())?;
                let horizontal_offset = copy_object
                    .get("size")
                    .and_then(Value::as_array)
                    .and_then(|size| size.first())
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0) as f32
                    + 0.5;
                if let Some(position) = copy_object
                    .get_mut("position")
                    .and_then(Value::as_array_mut)
                    && let Some(x) = position.first_mut()
                    && let Some(value) = x.as_f64()
                {
                    *x = serde_json::json!(value as f32 + horizontal_offset);
                }
                if let Some(base_id) = copy_object
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                {
                    copy_object.insert(
                        "id".to_owned(),
                        Value::String(format!("{base_id}-copy-{}", items.len() + 1)),
                    );
                }
                items.push(copy);
                selection_after_edit =
                    Some(format!("world/{world_id}/{collection}/{}", items.len() - 1));
            }
            SceneEditOperation::Delete => {
                items.remove(index);
                selection_after_edit = Some(format!("world/{world_id}/{collection}"));
            }
        }
        let source = serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("could not serialize the scene manifest: {error}"))?
            + "\n";
        self.authored_manifest_source = source.clone();
        if let Some(shell) = &mut self.shell {
            shell.set_source_manifest(&source, true);
            if let Some(selection) = selection_after_edit {
                shell.select_scene_node(&selection);
            }
            if matches!(&operation, SceneEditOperation::Duplicate) {
                shell.set_playing(false);
            }
            shell.set_notice(match &operation {
                SceneEditOperation::SetTransform { .. }
                | SceneEditOperation::SetPrimitiveSize { .. } => {
                    "Scene object changed — save to keep it".to_owned()
                }
                SceneEditOperation::UpdateProperty { .. } => {
                    "Property changed — save to keep it".to_owned()
                }
                SceneEditOperation::Duplicate => {
                    "Copy added beside the original — drag it into place".to_owned()
                }
                SceneEditOperation::Delete => "Scene object deleted — save to keep it".to_owned(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "app_scene_edit/tests.rs"]
mod scene_edit_tests;
