use super::*;
use crate::app_scene_migration::{
    active_manifest_world_id, default_scene_object, ensure_scene_collection, manifest_world_mut,
    migrate_manifest_to_scene, retarget_migrated_scene_edit,
};
use cubacadabra_scene::{
    AuthoringNode, EditorMetadata, Transform, parse_authoring_scene, serialize_authoring_scene,
};
impl StudioApp {
    pub(crate) fn create_new_project(&mut self, title: &str, parent: &Path) {
        let result = game_creator::create_game(title, parent);
        match result {
            Ok(created) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_new_project_created(&created.project);
                }
                self.start_project_load(created.project);
            }
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_new_project_error(error);
                }
            }
        }
        self.request_redraw();
    }

    pub(crate) fn import_source_images(&mut self, files: Vec<PathBuf>, target: PathBuf) {
        if !self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable)
        {
            return;
        }
        if target != Path::new("assets/images") {
            if let Some(shell) = &mut self.shell {
                shell.set_notice("Images can only be added to assets/images".to_owned());
            }
            return;
        }

        let destination = self.project_root.join(&target);
        if let Err(error) = fs::create_dir_all(&destination) {
            if let Some(shell) = &mut self.shell {
                shell.set_project_error(format!("Could not create image asset directory: {error}"));
            }
            return;
        }
        let mut manifest: Value = match serde_json::from_str(&self.authored_manifest_source) {
            Ok(manifest) => manifest,
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(format!("Manifest is no longer valid JSON: {error}"));
                }
                return;
            }
        };
        let mut imported = Vec::new();
        let mut errors = Vec::new();
        for file in files {
            let is_png = file
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
            if !is_png {
                errors.push(format!("{} is not a PNG file", file.display()));
                continue;
            }
            let Some(file_name) = file.file_name().and_then(|name| name.to_str()) else {
                errors.push(format!("{} has no valid filename", file.display()));
                continue;
            };
            let destination_file = unique_asset_path(&destination, file_name);
            let bytes = match fs::read(&file) {
                Ok(bytes) => bytes,
                Err(error) => {
                    errors.push(format!("could not read {}: {error}", file.display()));
                    continue;
                }
            };
            if bytes.len() > 8 * 1024 * 1024 {
                errors.push(format!("{} exceeds the 8 MiB image limit", file.display()));
                continue;
            }
            if image::load_from_memory(&bytes).is_err() {
                errors.push(format!("{} is not a valid PNG image", file.display()));
                continue;
            }
            if let Err(error) = write_atomic(&destination_file, &bytes) {
                errors.push(error);
                continue;
            }
            let Some(destination_name) = destination_file.file_name() else {
                errors.push(format!(
                    "could not determine the destination filename for {file_name}"
                ));
                continue;
            };
            let relative = target.join(destination_name);
            match add_image_asset(&mut manifest, &relative) {
                Ok(_) => imported.push(relative),
                Err(error) => {
                    let _ = fs::remove_file(&destination_file);
                    errors.push(error);
                }
            }
        }

        if imported.is_empty() {
            if let Some(shell) = &mut self.shell {
                shell.set_notice(if errors.is_empty() {
                    "No image files were selected".to_owned()
                } else {
                    errors.join("; ")
                });
            }
            return;
        }
        let source = match serde_json::to_string_pretty(&manifest) {
            Ok(source) => source + "\n",
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(format!(
                        "Could not update the project manifest: {error}"
                    ));
                }
                return;
            }
        };
        self.authored_manifest_source = source.clone();
        if let Some(shell) = &mut self.shell {
            shell.set_source_manifest(&source, true);
            shell.set_source_assets(load_source_assets(&self.project_root));
            shell.set_source_directories(load_source_directories(&self.project_root));
            for relative in &imported {
                shell.record_imported_asset(self.project_root.join(relative));
            }
            let message = if errors.is_empty() {
                format!(
                    "Imported {} image{} — save to keep the project change",
                    imported.len(),
                    if imported.len() == 1 { "" } else { "s" }
                )
            } else {
                format!(
                    "Imported {} image{}; {}",
                    imported.len(),
                    if imported.len() == 1 { "" } else { "s" },
                    errors.join("; ")
                )
            };
            shell.set_notice(message);
        }
        self.request_redraw();
    }

    pub(crate) fn save_project_source(&mut self) -> bool {
        let editable = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable);
        if !editable {
            return false;
        }
        let mut source_files = self
            .shell
            .as_ref()
            .map(StudioShell::source_files_for_save)
            .unwrap_or_default();
        if !source_files
            .iter()
            .any(|(relative_path, _)| relative_path == Path::new("manifest.json"))
        {
            source_files.push((
                PathBuf::from("manifest.json"),
                self.authored_manifest_source.clone(),
            ));
        }
        if let Some((_, manifest_source)) = source_files
            .iter()
            .find(|(relative_path, _)| relative_path == Path::new("manifest.json"))
            && let Err(error) = serde_json::from_str::<Value>(manifest_source)
        {
            if let Some(shell) = &mut self.shell {
                shell.set_project_error(format!("Cannot save manifest: {error}"));
            }
            return false;
        }
        if let Some((_, scene_source)) = source_files
            .iter()
            .find(|(relative_path, _)| relative_path == Path::new("scene.json"))
            && let Err(error) = parse_authoring_scene(scene_source)
        {
            if let Some(shell) = &mut self.shell {
                shell.set_project_error(format!("Cannot save scene.json: {error}"));
            }
            return false;
        }
        let mut saved = Ok(());
        for (relative_path, source) in &source_files {
            if let Err(message) =
                write_atomic(&self.project_root.join(relative_path), source.as_bytes())
            {
                saved = Err(message);
                break;
            }
            if relative_path == Path::new("manifest.json") {
                self.authored_manifest_source = source.clone();
            } else if relative_path == Path::new("scene.json") {
                self.authored_scene_source = Some(source.clone());
            }
        }
        match saved {
            Ok(()) => {
                if let Some(shell) = &mut self.shell {
                    let preview_stale = shell.preview_is_stale();
                    shell.mark_source_files_saved();
                    shell.set_source_manifest(&self.authored_manifest_source, false);
                    shell.set_source_scene(self.authored_scene_source.as_deref(), false);
                    shell.set_notice(if preview_stale {
                        "Project saved — Rebuild & Play to preview it".to_owned()
                    } else {
                        "Project saved".to_owned()
                    });
                }
                true
            }
            Err(message) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(message);
                }
                false
            }
        }
    }

    pub(crate) fn capture_codex_changes(&mut self) -> Vec<CodexFileChange> {
        let Some(before) = self.codex_checkpoint.take() else {
            self.codex_changes = None;
            return Vec::new();
        };
        let Ok(after) = snapshot_project_files(&self.project_root) else {
            self.codex_changes = None;
            return Vec::new();
        };
        let changes = diff_project_files(before, after);
        self.codex_changes = Some(changes.clone());
        changes
    }

    pub(crate) fn undo_codex_changes(&mut self) {
        let Some(changes) = self.codex_changes.take() else {
            if let Some(shell) = &mut self.shell {
                shell.set_notice("There are no Codex changes to undo".to_owned());
            }
            return;
        };
        let mut restored = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();
        for change in changes {
            let path = self.project_root.join(&change.relative_path);
            let current = fs::read(&path).ok();
            if current != change.after {
                skipped += 1;
                continue;
            }
            let result = match change.before {
                Some(bytes) => write_atomic(&path, &bytes),
                None => fs::remove_file(&path).map_err(|error| {
                    format!(
                        "could not remove {}: {error}",
                        change.relative_path.display()
                    )
                }),
            };
            match result {
                Ok(()) => restored += 1,
                Err(message) => errors.push(message),
            }
        }
        if let Some(shell) = &mut self.shell {
            shell.clear_codex_changes();
            if errors.is_empty() {
                let message = if skipped == 0 {
                    format!("Undid {restored} Codex change(s); rebuilding preview…")
                } else {
                    format!(
                        "Undid {restored} Codex change(s); kept {skipped} later edit(s); rebuilding preview…"
                    )
                };
                shell.set_notice(message);
            } else {
                shell.set_project_error(format!(
                    "Undo restored {restored} file(s), skipped {skipped}, and failed: {}",
                    errors.join("; ")
                ));
            }
        }
        self.refresh_authored_manifest_from_disk();
        self.start_project_reload();
    }

    pub(crate) fn refresh_authored_manifest_from_disk(&mut self) {
        let path = self.project_root.join("manifest.json");
        match fs::read_to_string(&path) {
            Ok(source) => {
                self.authored_manifest_source = source.clone();
                if let Some(shell) = &mut self.shell {
                    shell.set_source_manifest(&source, false);
                }
            }
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(format!(
                        "Could not read the updated manifest {}: {error}",
                        path.display()
                    ));
                }
            }
        }
        let scene_path = self.project_root.join("scene.json");
        if scene_path.is_file()
            && let Ok(source) = fs::read_to_string(&scene_path)
        {
            self.authored_scene_source = Some(source.clone());
            if let Some(shell) = &mut self.shell {
                shell.set_source_scene(Some(&source), false);
            }
        }
    }

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
                                "block-new".to_owned(),
                                "New Block".to_owned(),
                                "primitive".to_owned(),
                                serde_json::json!({
                                    "shape": "box",
                                    "size": [4, 1, 4],
                                    "material": "signal"
                                }),
                                [0.0, 1.0, 0.0],
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
                    self.commit_authoring_scene_transaction(
                        scene_source,
                        updated,
                        &id,
                        &format!("{} added — save, then Rebuild & Play", kind.label()),
                    );
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
                            "Scene object duplicated — save to keep it",
                        );
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
                    "Floor changed in {world_id} — save, then Rebuild & Play"
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
                        "Block added — drag it in the viewport, then press Play to test".to_owned(),
                    );
                } else {
                    shell.set_notice(format!(
                        "{} added — save, then Rebuild & Play",
                        kind.label()
                    ));
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
            shell.set_notice(match &operation {
                SceneEditOperation::SetTransform { .. }
                | SceneEditOperation::SetPrimitiveSize { .. } => {
                    "Scene object changed — save to keep it".to_owned()
                }
                SceneEditOperation::UpdateProperty { .. } => {
                    "Property changed — save to keep it".to_owned()
                }
                SceneEditOperation::Duplicate => {
                    "Scene object duplicated — save to keep it".to_owned()
                }
                SceneEditOperation::Delete => "Scene object deleted — save to keep it".to_owned(),
            });
        }
        Ok(())
    }
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

fn unique_asset_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("png");
    for suffix in 2.. {
        let candidate = directory.join(format!("{stem}-{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("the suffix range is finite only in theory")
}

#[cfg(test)]
mod scene_edit_tests {
    use super::*;

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
}
