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
        self.authoring_scene = parse_authoring_scene(&source).ok();
        self.authoring_scene_indices = self
            .authoring_scene
            .as_ref()
            .map(authoring_scene_indices)
            .unwrap_or_default();
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
            let before = self
                .scene_geometry_state(&target)
                .ok_or_else(|| format!("scene node {target} was not found"))?;
            self.scene_drag_snapshot = Some(SceneDragSnapshot {
                target: target.clone(),
                before,
            });
        }
        if phase == SceneViewportEditPhase::Begin {
            return Ok(());
        }
        self.apply_scene_edit(edit)?;
        if phase == SceneViewportEditPhase::Commit {
            if let Some(snapshot) = self.scene_drag_snapshot.take()
                && let Some(after) = self.scene_geometry_state(&snapshot.target)
                && snapshot.before != after
            {
                self.scene_undo.push(SceneHistoryEntry {
                    kind: SceneHistoryKind::Geometry {
                        target: snapshot.target,
                        before: snapshot.before,
                        after,
                    },
                });
                self.scene_redo.clear();
            }
        }
        Ok(())
    }

    pub(crate) fn cancel_scene_viewport_edit(&mut self) {
        let Some(snapshot) = self.scene_drag_snapshot.take() else {
            return;
        };
        self.scene_drag_cancelled_target = Some(snapshot.target.clone());
        if let Err(message) = self.restore_scene_geometry_state(&snapshot.target, &snapshot.before)
            && let Some(shell) = &mut self.shell
        {
            shell.set_project_error(message);
        }
        if let Some(shell) = &mut self.shell {
            shell.set_notice("Transform cancelled".to_owned());
        }
    }

    pub(crate) fn invalidate_scene_history(&mut self) {
        self.scene_undo.clear();
        self.scene_redo.clear();
        self.scene_drag_snapshot = None;
        self.scene_drag_cancelled_target = None;
    }

    fn scene_geometry_state(&self, target: &str) -> Option<SceneGeometryState> {
        let scene = self.authoring_scene.as_ref()?;
        let index = self.authoring_scene_indices.get(target)?;
        let node = scene.nodes.get(*index)?;
        let primitive_size = node
            .components
            .get("primitive")
            .and_then(Value::as_object)
            .and_then(|component| component.get("size"))
            .and_then(crate::shell::vector_value);
        Some(SceneGeometryState {
            transform: node.transform.clone(),
            primitive_size,
        })
    }

    fn update_scene_geometry_cache(&mut self, target: &str) -> Result<(), String> {
        let Some(scene) = self.authoring_scene.as_ref() else {
            return Ok(());
        };
        let affected = scene
            .nodes
            .iter()
            .filter(|node| node.id == target || scene_node_has_ancestor(scene, node, target))
            .map(|node| {
                let world = scene.world_transform(&node.id)?;
                let primitive_size = node
                    .components
                    .get("primitive")
                    .and_then(Value::as_object)
                    .and_then(|component| component.get("size"))
                    .and_then(crate::shell::vector_value);
                Ok((
                    node.id.clone(),
                    world,
                    node.transform.rotation,
                    primitive_size,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if let Some(shell) = &mut self.shell {
            for (id, world, local_rotation, primitive_size) in affected {
                shell.update_scene_object_geometry(
                    &id,
                    world.position,
                    world.rotation,
                    local_rotation,
                    world.scale,
                    primitive_size,
                );
            }
        }
        Ok(())
    }

    fn restore_scene_geometry_state(
        &mut self,
        target: &str,
        state: &SceneGeometryState,
    ) -> Result<(), String> {
        let Some(scene) = self.authoring_scene.as_mut() else {
            return Err("No authoring scene is loaded".to_owned());
        };
        let index = *self
            .authoring_scene_indices
            .get(target)
            .ok_or_else(|| format!("scene node {target} was not found"))?;
        let node = scene
            .nodes
            .get_mut(index)
            .ok_or_else(|| format!("scene node {target} was not found"))?;
        node.transform = state.transform.clone();
        if let Some(size) = state.primitive_size {
            set_authoring_component_property(node, "size", serde_json::json!(size))?;
        }
        self.update_scene_geometry_cache(target)?;
        if let Some(shell) = &mut self.shell {
            shell.mark_scene_dirty();
        }
        Ok(())
    }

    fn apply_live_geometry_edit(&mut self, request: &SceneEditRequest) -> Result<(), String> {
        let target = match request {
            SceneEditRequest::SetTransform { target, .. }
            | SceneEditRequest::SetPrimitiveSize { target, .. } => target,
            _ => return Ok(()),
        };
        let before = self
            .scene_geometry_state(target)
            .ok_or_else(|| format!("scene node {target} was not found"))?;
        let Some(scene) = self.authoring_scene.as_mut() else {
            return Err("No authoring scene is loaded".to_owned());
        };
        let index = *self
            .authoring_scene_indices
            .get(target)
            .ok_or_else(|| format!("scene node {target} was not found"))?;
        match request {
            SceneEditRequest::SetTransform {
                position,
                rotation,
                scale,
                ..
            } => {
                let node = scene
                    .nodes
                    .get_mut(index)
                    .ok_or_else(|| format!("scene node {target} was not found"))?;
                if node.editor.locked {
                    return Err(format!("scene node {} is locked", node.id));
                }
                if position.iter().any(|value| !value.is_finite()) {
                    return Err(format!(
                        "scene node {target} position must contain finite values"
                    ));
                }
                node.transform.position = *position;
                if let Some(rotation) = rotation {
                    if rotation.iter().any(|value| !value.is_finite()) {
                        return Err(format!(
                            "scene node {target} rotation must contain finite values"
                        ));
                    }
                    node.transform.rotation = *rotation;
                }
                if let Some(scale) = scale {
                    if scale
                        .iter()
                        .any(|value| !value.is_finite() || *value < 0.05)
                    {
                        return Err(format!(
                            "scene node {target} scale must contain finite values of at least 0.05"
                        ));
                    }
                    node.transform.scale = *scale;
                }
            }
            SceneEditRequest::SetPrimitiveSize { position, size, .. } => {
                let node = scene
                    .nodes
                    .get_mut(index)
                    .ok_or_else(|| format!("scene node {target} was not found"))?;
                if node.editor.locked {
                    return Err(format!("scene node {target} is locked"));
                }
                node.transform.position = *position;
                set_authoring_component_property(node, "size", serde_json::json!(size))?;
            }
            _ => unreachable!(),
        }
        let after = self
            .scene_geometry_state(target)
            .ok_or_else(|| format!("scene node {target} was not found"))?;
        self.update_scene_geometry_cache(target)?;
        if let Some(shell) = &mut self.shell {
            shell.mark_scene_dirty();
        }
        if self.scene_drag_snapshot.is_none() && before != after {
            self.scene_undo.push(SceneHistoryEntry {
                kind: SceneHistoryKind::Geometry {
                    target: target.clone(),
                    before,
                    after,
                },
            });
            self.scene_redo.clear();
        }
        if let Some(shell) = &mut self.shell {
            shell.set_notice(
                if matches!(request, SceneEditRequest::SetPrimitiveSize { .. }) {
                    "Primitive size changed — save to keep it".to_owned()
                } else if matches!(
                    request,
                    SceneEditRequest::SetTransform {
                        rotation: Some(_),
                        ..
                    }
                ) {
                    "Orientation changed — save to keep it".to_owned()
                } else {
                    "Position changed — save to keep it".to_owned()
                },
            );
        }
        Ok(())
    }

    pub(crate) fn undo_scene_edit(&mut self) -> Result<(), String> {
        let entry = self
            .scene_undo
            .pop()
            .ok_or_else(|| "Nothing to undo".to_owned())?;
        match &entry.kind {
            SceneHistoryKind::Snapshot { before, target, .. } => {
                self.apply_scene_source_snapshot(
                    before.clone(),
                    target,
                    "Position restored — save to keep it",
                );
            }
            SceneHistoryKind::Geometry { target, before, .. } => {
                self.restore_scene_geometry_state(target, before)?;
                if let Some(shell) = &mut self.shell {
                    shell.select_scene_node(target);
                    shell.set_notice("Position restored — save to keep it".to_owned());
                }
            }
        }
        self.scene_redo.push(entry.clone());
        Ok(())
    }

    pub(crate) fn redo_scene_edit(&mut self) -> Result<(), String> {
        let entry = self
            .scene_redo
            .pop()
            .ok_or_else(|| "Nothing to redo".to_owned())?;
        match &entry.kind {
            SceneHistoryKind::Snapshot { after, target, .. } => {
                self.apply_scene_source_snapshot(
                    after.clone(),
                    target,
                    "Position reapplied — save to keep it",
                );
            }
            SceneHistoryKind::Geometry { target, after, .. } => {
                self.restore_scene_geometry_state(target, after)?;
                if let Some(shell) = &mut self.shell {
                    shell.select_scene_node(target);
                    shell.set_notice("Position reapplied — save to keep it".to_owned());
                }
            }
        }
        self.scene_undo.push(entry.clone());
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

fn top_level_scene_targets(scene: &AuthoringScene, targets: &[String]) -> Vec<String> {
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

fn scene_node_has_ancestor(
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

fn record_scene_history(
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

fn set_authoring_component_property(
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

fn remove_authoring_component_property(
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

fn normalize_legacy_primitive_appearance(primitive: &mut serde_json::Map<String, Value>) {
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

fn is_roblox_source_linked(node: &AuthoringNode) -> bool {
    node.source
        .as_ref()
        .is_some_and(|source| source.format == "roblox")
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
            kind: SceneHistoryKind::Snapshot {
                before: "old".to_owned(),
                after: "middle".to_owned(),
                target: "node".to_owned(),
            },
        }];
        let mut redo = vec![SceneHistoryEntry {
            kind: SceneHistoryKind::Snapshot {
                before: "middle".to_owned(),
                after: "new".to_owned(),
                target: "node".to_owned(),
            },
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
        let SceneHistoryKind::Snapshot { before, after, .. } = &undo[1].kind else {
            panic!("expected snapshot history entry");
        };
        assert_eq!(before, "middle");
        assert_eq!(after, "latest");
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

    #[test]
    fn imported_roblox_nodes_are_not_deletable_yet() {
        let mut block = block_node("block-1", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE);
        block.source = Some(cubacadabra_scene::SourceMetadata {
            format: "roblox".to_owned(),
            class: Some("Part".to_owned()),
            path: Some("Workspace:Workspace[1]/Part:Block[1]".to_owned()),
            properties: BTreeMap::new(),
        });
        assert!(is_roblox_source_linked(&block));
    }

    #[test]
    fn structured_component_properties_can_be_added_and_removed() {
        let mut block = block_node("block-1", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE);
        set_authoring_component_property(
            &mut block,
            "primitive.material",
            Value::String("builtin:grass".to_owned()),
        )
        .unwrap();
        assert_eq!(block.components["primitive"]["material"], "builtin:grass");

        let removed =
            remove_authoring_component_property(&mut block, "primitive.material").unwrap();
        assert_eq!(removed, "builtin:grass");
        assert!(block.components["primitive"].get("material").is_none());
    }

    #[test]
    fn editing_legacy_primitive_appearance_writes_canonical_fields() {
        let mut block = block_node("block-1", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE);
        block.components.insert(
            "primitive".to_owned(),
            serde_json::json!({
                "shape": "box",
                "size": NEW_BLOCK_SIZE,
                "material": "#767F91",
                "runtimeMaterial": "builtin:rock"
            }),
        );

        set_authoring_component_property(
            &mut block,
            "primitive.color",
            Value::String("#62A85A".to_owned()),
        )
        .unwrap();

        assert_eq!(block.components["primitive"]["color"], "#62A85A");
        assert_eq!(block.components["primitive"]["material"], "builtin:rock");
        assert!(
            block.components["primitive"]
                .get("runtimeMaterial")
                .is_none()
        );
    }

    #[test]
    fn multi_selection_keeps_only_top_level_targets() {
        let mut parent = block_node("parent", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE);
        parent.parent_id = Some("world".to_owned());
        let mut child = block_node("child", [1.0, 0.0, 0.0], NEW_BLOCK_SIZE);
        child.parent_id = Some("parent".to_owned());
        let sibling = block_node("sibling", [4.0, 1.0, 0.0], NEW_BLOCK_SIZE);
        let scene = scene_with(vec![parent, child, sibling]);

        let targets = top_level_scene_targets(
            &scene,
            &[
                "parent".to_owned(),
                "child".to_owned(),
                "sibling".to_owned(),
            ],
        );

        assert_eq!(targets, vec!["parent", "sibling"]);
    }
}
