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

impl StudioApp {
    fn apply_scene_source_snapshot(&mut self, source: String, target: &str, notice: &str) {
        self.authored_scene_source = Some(source.clone());
        if let Some(shell) = &mut self.shell {
            shell.set_source_scene(Some(&source), true);
            shell.select_scene_node(target);
            shell.set_notice(notice.to_owned());
        }
    }

    fn commit_authoring_scene_transaction(
        &mut self,
        before: String,
        after: String,
        target: &str,
        notice: &str,
    ) {
        if before == after {
            return;
        }
        if self.scene_drag_snapshot.is_some() {
            self.apply_scene_source_snapshot(after, target, notice);
            return;
        }
        record_scene_history(
            &mut self.scene_undo,
            &mut self.scene_redo,
            before,
            after.clone(),
            target,
        );
        self.apply_scene_source_snapshot(after, target, notice);
    }

    pub(crate) fn apply_scene_viewport_edit(
        &mut self,
        edit: SceneEditRequest,
        phase: SceneViewportEditPhase,
    ) -> Result<(), String> {
        let target = match &edit {
            SceneEditRequest::SetTransform { target, .. }
            | SceneEditRequest::SetPrimitiveSize { target, .. } => target.clone(),
            _ => return Err("viewport edits must update scene geometry".to_owned()),
        };
        if phase != SceneViewportEditPhase::Begin
            && self
                .scene_drag_cancelled_target
                .as_deref()
                .is_some_and(|cancelled| cancelled == target)
        {
            if phase == SceneViewportEditPhase::Commit {
                self.scene_drag_cancelled_target = None;
            }
            return Ok(());
        }
        if phase == SceneViewportEditPhase::Begin {
            self.scene_drag_cancelled_target = None;
        }
        if self.scene_drag_snapshot.is_none() {
            self.scene_drag_snapshot = Some(SceneDragSnapshot {
                scene_before: self.authored_scene_source.clone(),
                target: target.clone(),
            });
        }
        if phase == SceneViewportEditPhase::Begin {
            return Ok(());
        }
        self.apply_scene_edit(edit)?;
        if phase == SceneViewportEditPhase::Commit {
            if let Some(snapshot) = self.scene_drag_snapshot.take()
                && let (Some(before), Some(after)) =
                    (snapshot.scene_before, self.authored_scene_source.clone())
                && before != after
            {
                record_scene_history(
                    &mut self.scene_undo,
                    &mut self.scene_redo,
                    before,
                    after,
                    &snapshot.target,
                );
            }
        }
        Ok(())
    }

    pub(crate) fn cancel_scene_viewport_edit(&mut self) {
        let Some(snapshot) = self.scene_drag_snapshot.take() else {
            return;
        };
        self.scene_drag_cancelled_target = Some(snapshot.target.clone());
        if let Some(before) = snapshot.scene_before {
            self.apply_scene_source_snapshot(
                before,
                &snapshot.target,
                "Transform cancelled — source restored",
            );
        }
    }

    pub(crate) fn invalidate_scene_history(&mut self) {
        self.scene_undo.clear();
        self.scene_redo.clear();
        self.scene_drag_snapshot = None;
        self.scene_drag_cancelled_target = None;
    }

    pub(crate) fn undo_scene_edit(&mut self) -> Result<(), String> {
        let entry = self
            .scene_undo
            .pop()
            .ok_or_else(|| "Nothing to undo".to_owned())?;
        self.scene_redo.push(entry.clone());
        self.apply_scene_source_snapshot(
            entry.before,
            &entry.target,
            "Position restored — save to keep it",
        );
        Ok(())
    }

    pub(crate) fn redo_scene_edit(&mut self) -> Result<(), String> {
        let entry = self
            .scene_redo
            .pop()
            .ok_or_else(|| "Nothing to redo".to_owned())?;
        self.scene_undo.push(entry.clone());
        self.apply_scene_source_snapshot(
            entry.after,
            &entry.target,
            "Position reapplied — save to keep it",
        );
        Ok(())
    }

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
            if let Some(shell) = &mut self.shell {
                shell.set_source_manifest(&manifest_source, true);
                shell.set_source_scene(Some(&scene_source), true);
                shell.set_notice("Upgraded this project to the scene format".to_owned());
            }
        }
        if let Some(scene_source) = self.authored_scene_source.clone() {
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
                                    "material": "signal"
                                }),
                                next_block_position(&scene),
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
                        }
                    }
                    let mut component_map = BTreeMap::new();
                    component_map.insert(component_name.to_owned(), component);
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
                        "Block added — drag to move, use the handles to resize".to_owned()
                    } else {
                        format!("{} added — press Play to preview it", kind.label())
                    };
                    self.commit_authoring_scene_transaction(scene_source, updated, &id, &notice);
                    if *kind == SceneObjectKind::Block
                        && let Some(shell) = &mut self.shell
                    {
                        shell.set_playing(false);
                    }
                    return Ok(());
                }
                SceneEditRequest::SetTransform {
                    target,
                    position,
                    scale,
                } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if scene.node(target).is_some() {
                        scene.set_position(target, *position)?;
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
                SceneEditRequest::DuplicateObject { target } => {
                    let mut scene = parse_authoring_scene(&scene_source)?;
                    if let Some(node) = scene.node(target).cloned() {
                        if node.editor.locked {
                            return Err(format!("scene node {target} is locked"));
                        }
                        let mut number = 1;
                        let id = loop {
                            let candidate = format!("{}-copy-{number}", node.id);
                            if scene.node(&candidate).is_none() {
                                break candidate;
                            }
                            number += 1;
                        };
                        let mut copy = node;
                        copy.id = id.clone();
                        copy.name = format!("{} Copy {number}", copy.name);
                        offset_duplicate(&mut copy);
                        for component in copy.components.values_mut() {
                            if let Some(component) = component.as_object_mut() {
                                for key in ["id", "runtimeId"] {
                                    if component.contains_key(key) {
                                        component.insert(key.to_owned(), Value::String(id.clone()));
                                    }
                                }
                            }
                        }
                        scene.nodes.push(copy);
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
            let collection = kind.collection();
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
                    shell.set_notice(
                        "Block added — drag to move, use the handles to resize".to_owned(),
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
                scale: None,
            } => (target, SceneEditOperation::SetTransform { position }),
            SceneEditRequest::SetTransform { scale: Some(_), .. } => {
                return Err("non-uniform transform edits require an authoring scene".to_owned());
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

const NEW_BLOCK_SIZE: [f32; 3] = [2.0, 2.0, 2.0];
const NEW_BLOCK_GAP: f32 = 0.5;

fn next_block_position(scene: &AuthoringScene) -> [f32; 3] {
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

fn primitive_size(node: &AuthoringNode) -> Option<[f32; 3]> {
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

fn offset_duplicate(node: &mut AuthoringNode) {
    let offset = primitive_size(node)
        .map(|size| size[0] * node.transform.scale[0].abs())
        .unwrap_or(1.0)
        + NEW_BLOCK_GAP;
    node.transform.position[0] += offset;
}

fn record_scene_history(
    undo: &mut Vec<SceneHistoryEntry>,
    redo: &mut Vec<SceneHistoryEntry>,
    before: String,
    after: String,
    target: &str,
) {
    undo.push(SceneHistoryEntry {
        before,
        after,
        target: target.to_owned(),
    });
    redo.clear();
}

fn set_authoring_component_property(
    node: &mut AuthoringNode,
    key: &str,
    value: Value,
) -> Result<(), String> {
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

#[cfg(test)]
mod scene_edit_tests {
    use super::*;

    fn block_node(id: &str, position: [f32; 3], size: [f32; 3]) -> AuthoringNode {
        AuthoringNode {
            id: id.to_owned(),
            parent_id: Some("world".to_owned()),
            name: id.to_owned(),
            transform: Transform {
                position,
                ..Transform::default()
            },
            components: BTreeMap::from([(
                "primitive".to_owned(),
                serde_json::json!({ "shape": "box", "size": size }),
            )]),
            editor: EditorMetadata::default(),
            source: None,
        }
    }

    fn scene_with(nodes: Vec<AuthoringNode>) -> AuthoringScene {
        let mut all_nodes = vec![AuthoringNode {
            id: "world".to_owned(),
            parent_id: None,
            name: "World".to_owned(),
            transform: Transform::default(),
            components: BTreeMap::new(),
            editor: EditorMetadata::default(),
            source: None,
        }];
        all_nodes.extend(nodes);
        AuthoringScene {
            format_version: 1,
            world_id: Some("starter-world".to_owned()),
            nodes: all_nodes,
        }
    }

    #[test]
    fn scene_history_records_one_transaction_and_invalidates_redo() {
        let mut undo = vec![SceneHistoryEntry {
            before: "old".to_owned(),
            after: "middle".to_owned(),
            target: "node".to_owned(),
        }];
        let mut redo = vec![SceneHistoryEntry {
            before: "middle".to_owned(),
            after: "new".to_owned(),
            target: "node".to_owned(),
        }];
        record_scene_history(
            &mut undo,
            &mut redo,
            "middle".to_owned(),
            "latest".to_owned(),
            "node",
        );
        assert_eq!(undo.len(), 2);
        assert!(redo.is_empty());
        assert_eq!(undo[1].before, "middle");
        assert_eq!(undo[1].after, "latest");
    }

    #[test]
    fn new_blocks_use_the_nearest_open_ground_slot() {
        let empty = scene_with(Vec::new());
        assert_eq!(next_block_position(&empty), [0.0, 1.0, 0.0]);

        let one_block = scene_with(vec![block_node("block-1", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE)]);
        assert_eq!(next_block_position(&one_block), [2.5, 1.0, 0.0]);

        let wide_block = scene_with(vec![block_node(
            "block-wide",
            [0.0, 1.0, 0.0],
            [6.0, 2.0, 2.0],
        )]);
        assert_eq!(next_block_position(&wide_block), [0.0, 1.0, 2.5]);
    }

    #[test]
    fn duplicated_blocks_are_offset_by_their_visible_width() {
        let mut block = block_node("block-1", [3.0, 1.0, 4.0], [2.0, 2.0, 2.0]);
        block.transform.scale = [1.5, 1.0, 1.0];

        offset_duplicate(&mut block);

        assert_eq!(block.transform.position, [6.5, 1.0, 4.0]);
    }
}
