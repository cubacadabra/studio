use super::*;

impl StudioApp {
    pub(super) fn apply_scene_source_snapshot(
        &mut self,
        source: String,
        target: &str,
        notice: &str,
    ) {
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

    pub(super) fn commit_authoring_scene_transaction(
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

    pub(super) fn scene_geometry_state(&self, target: &str) -> Option<SceneGeometryState> {
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

    pub(super) fn update_scene_geometry_cache(&mut self, target: &str) -> Result<(), String> {
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

    pub(super) fn restore_scene_geometry_state(
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

    pub(super) fn apply_live_geometry_edit(
        &mut self,
        request: &SceneEditRequest,
    ) -> Result<(), String> {
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
}
