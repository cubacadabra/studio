use super::*;
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
            }
        }
        match saved {
            Ok(()) => {
                if let Some(shell) = &mut self.shell {
                    let preview_stale = shell.preview_is_stale();
                    shell.mark_source_files_saved();
                    shell.set_source_manifest(&self.authored_manifest_source, false);
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
    }

    pub(crate) fn apply_scene_edit(&mut self, request: SceneEditRequest) -> Result<(), String> {
        if !self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable)
        {
            return Err("Open a raw source project to edit scene objects.".to_owned());
        }
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
            SceneEditRequest::UpdateTransform {
                target,
                position,
                size,
            } => (
                target,
                SceneEditOperation::UpdateTransform { position, size },
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
            SceneEditOperation::UpdateTransform { position, size } => {
                object.insert("position".to_owned(), serde_json::json!(position));
                if let Some(size) = size {
                    object.insert("size".to_owned(), serde_json::json!(size));
                }
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
                SceneEditOperation::UpdateTransform { .. } => {
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

    pub(crate) fn choose_and_open_project(&mut self) {
        let starting_directory = if !self.standalone_preview && self.project_root.is_dir() {
            self.project_root
                .parent()
                .unwrap_or(&self.project_root)
                .to_path_buf()
        } else {
            env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };
        let mut dialog = rfd::FileDialog::new()
            .set_title("Open Project")
            .set_directory(starting_directory)
            .set_can_create_directories(false);
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window);
        }
        let Some(project) = dialog.pick_folder() else {
            return;
        };
        self.start_project_load(project);
    }

    pub(crate) fn start_project_load(&mut self, project: PathBuf) {
        self.recent_projects = remember_recent_project(&project);
        if let Some(shell) = &mut self.shell {
            shell.set_recent_projects(self.recent_projects.clone());
        }
        self.start_project_load_with_mode(project, false, false);
    }

    pub(crate) fn start_project_reload(&mut self) {
        self.start_project_reload_with_origin(false);
    }

    pub(crate) fn start_codex_project_reload(&mut self) {
        self.start_project_reload_with_origin(true);
    }

    pub(crate) fn start_project_reload_with_origin(&mut self, codex_rebuild: bool) {
        if self.standalone_preview {
            if let Some(shell) = &mut self.shell {
                shell.set_project_error(
                    "The standalone morph preview cannot be rebuilt.".to_owned(),
                );
                if codex_rebuild {
                    shell.set_codex_preview_rebuild_failed(
                        "the standalone morph preview cannot be rebuilt",
                    );
                }
            }
            return;
        }
        self.start_project_load_with_mode(self.project_root.clone(), true, codex_rebuild);
    }

    pub(crate) fn start_project_load_with_mode(
        &mut self,
        project: PathBuf,
        preserve_editor: bool,
        codex_rebuild: bool,
    ) {
        if self.pending_project_load.is_some()
            || self.background_project_ready.is_some()
            || self.prepared_project_ready.is_some()
        {
            if codex_rebuild && let Some(shell) = &mut self.shell {
                shell.set_codex_preview_rebuild_failed("another project load is already running");
            }
            return;
        }
        if let Some(shell) = &mut self.shell {
            if preserve_editor {
                shell.begin_game_rebuild();
            } else {
                shell.begin_project_loading();
            }
            shell.set_project_loading_progress(0.01);
        }
        let (sender, receiver) = mpsc::channel();
        let worker_project = project.clone();
        let spawn = thread::Builder::new()
            .name("studio-project-loader".to_owned())
            .spawn(move || {
                let progress_sender = sender.clone();
                let result = load_project_in_background(&worker_project, move |progress| {
                    let _ = progress_sender.send(ProjectLoadEvent::Progress(progress));
                });
                let _ = sender.send(ProjectLoadEvent::Finished(result));
            });
        match spawn {
            Ok(_) => {
                self.pending_project_load = Some(PendingProjectLoad {
                    project,
                    preserve_editor,
                    codex_rebuild,
                    receiver,
                });
            }
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.cancel_project_loading();
                    if codex_rebuild {
                        shell.set_codex_preview_rebuild_failed(&format!(
                            "the project loader could not start: {error}"
                        ));
                    }
                }
                self.show_open_project_error(&format!(
                    "Could not start the project loader: {error}"
                ));
            }
        }
        self.request_redraw();
    }

    pub(crate) fn poll_project_load(&mut self) {
        let mut finished = None;
        loop {
            let event = self
                .pending_project_load
                .as_ref()
                .map(|load| load.receiver.try_recv());
            match event {
                Some(Ok(ProjectLoadEvent::Progress(progress))) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_project_loading_progress(progress);
                    }
                }
                Some(Ok(ProjectLoadEvent::Finished(result))) => {
                    finished = Some(result);
                    break;
                }
                Some(Err(mpsc::TryRecvError::Empty)) | None => break,
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    finished = Some(Err("The project loader stopped unexpectedly.".to_owned()));
                    break;
                }
            }
        }
        if let Some(result) = finished {
            let (project, preserve_editor, codex_rebuild) = self
                .pending_project_load
                .take()
                .map(|load| (load.project, load.preserve_editor, load.codex_rebuild))
                .unwrap_or_default();
            self.background_project_ready = Some((project, preserve_editor, codex_rebuild, result));
        }
    }

    pub(crate) fn prepare_ready_project_runtime(&mut self) {
        let Some((project, preserve_editor, codex_rebuild, result)) =
            self.background_project_ready.take()
        else {
            return;
        };
        let result = match result {
            Ok(background) => match ClientSession::load(
                &background.sources.manifest_source,
                &background.sources.script_source,
            ) {
                Ok(client) => match BackendClient::new(client.game_id()) {
                    Ok(network) => Ok(PreparedProjectLoad {
                        background,
                        client,
                        network,
                    }),
                    Err(error) => {
                        remove_temporary_package(&background.sources);
                        Err(error)
                    }
                },
                Err(error) => {
                    remove_temporary_package(&background.sources);
                    Err(error.to_string())
                }
            },
            Err(error) => Err(error),
        };
        if result.is_ok()
            && let Some(shell) = &mut self.shell
        {
            shell.set_project_loading_progress(0.93);
        }
        self.prepared_project_ready = Some((project, preserve_editor, codex_rebuild, result));
    }

    pub(crate) fn commit_ready_project_load(&mut self) {
        let Some((project, preserve_editor, codex_rebuild, result)) =
            self.prepared_project_ready.take()
        else {
            return;
        };
        let result =
            result.and_then(|prepared| self.commit_project_load(prepared, preserve_editor));
        match result {
            Ok(()) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_notice(if preserve_editor {
                        "Preview rebuilt and playing".to_owned()
                    } else {
                        format!("Opened {}", project.display())
                    });
                    if codex_rebuild {
                        shell.set_codex_preview_rebuilt();
                    }
                }
            }
            Err(message) => {
                if let Some(shell) = &mut self.shell {
                    shell.cancel_project_loading();
                }
                if preserve_editor {
                    if let Some(shell) = &mut self.shell {
                        shell.set_project_error(format!("Rebuild failed: {message}"));
                        if codex_rebuild {
                            shell.set_codex_preview_rebuild_failed(&message);
                        }
                    }
                } else {
                    self.show_open_project_error(&message);
                }
            }
        }
    }

    pub(crate) fn show_open_project_error(&mut self, message: &str) {
        if let Some(shell) = &mut self.shell {
            shell.set_notice(format!("Could not open project: {message}"));
        }
        let mut dialog = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Couldn’t Open Project")
            .set_description(message)
            .set_buttons(rfd::MessageButtons::Ok);
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window);
        }
        dialog.show();
    }

    pub(crate) fn commit_project_load(
        &mut self,
        prepared: PreparedProjectLoad,
        preserve_editor: bool,
    ) -> Result<(), String> {
        let PreparedProjectLoad {
            background:
                BackgroundProjectLoad {
                    sources:
                        GameSources {
                            project_root,
                            root,
                            authored_manifest_source,
                            manifest_source,
                            script_source,
                            standalone_preview,
                            temporary_package,
                        },
                    image_atlas,
                    local_morph_catalog,
                },
            mut client,
            network,
        } = prepared;

        // A newly-created engine starts its package generation at the same
        // value as the previous engine. The long-lived renderer therefore
        // cannot distinguish two consecutive ClientSession values by
        // generation alone. Alternate between the first and second package
        // generation using the engine's existing public loading API. This
        // keeps Studio compatible with released engine checkouts while still
        // forcing the renderer to consume the replacement scene.
        let client_uses_base_package_generation = !self.renderer_uses_base_package_generation;
        if !client_uses_base_package_generation {
            if !client.engine_mut().load_package_source(&manifest_source) {
                if let Some(package) = temporary_package {
                    let _ = fs::remove_dir_all(package);
                }
                return Err("the shared engine rejected the rebuilt scene".to_owned());
            }
            if !client.engine_mut().load_script_source(&script_source) {
                if let Some(package) = temporary_package {
                    let _ = fs::remove_dir_all(package);
                }
                return Err("the shared engine could not compile the rebuilt game logic".to_owned());
            }
        }

        if let (Some(renderer), Some(atlas)) = (&mut self.renderer, &image_atlas)
            && !renderer.set_package_image_atlas(
                atlas.width,
                atlas.height,
                &atlas.pixels,
                atlas.regions.clone(),
            )
        {
            if let Some(package) = temporary_package {
                let _ = fs::remove_dir_all(package);
            }
            return Err("the new game's image atlas could not be uploaded".to_owned());
        }
        let old_temporary_package = self.temporary_package.take();
        self.project_root = project_root;
        self.game_root = root;
        self.authored_manifest_source = authored_manifest_source;
        self.manifest_source = manifest_source;
        self.standalone_preview = standalone_preview;
        self.temporary_package = temporary_package;
        if !standalone_preview {
            self.recent_projects = remember_recent_project(&self.project_root);
        }
        self.image_atlas = image_atlas;
        self.network = network;
        self.client = client;
        self.renderer_uses_base_package_generation = client_uses_base_package_generation;
        self.local_morph_catalog = local_morph_catalog;
        self.runtime_ui_revision = u64::MAX;
        self.morph_loadout = default_morph_loadout();
        self.morph_request_serial = 0;
        self.pending_morph = None;
        self.registered_morphs.clear();
        self.pressed_keys.clear();
        if let Some(window) = &self.window {
            window.set_title(&format!(
                "Cubacadabra Studio — {}",
                game_name(&self.project_root)
            ));
        }
        if preserve_editor {
            if let Some(shell) = &mut self.shell {
                shell.set_project_editable(
                    !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
                );
                shell.set_source_manifest(&self.authored_manifest_source, false);
                shell.set_source_assets(load_source_assets(&self.project_root));
                shell.set_source_files(load_source_files(&self.project_root));
                shell.set_source_directories(load_source_directories(&self.project_root));
                shell.finish_project_loading();
                shell.set_notice("Preview rebuilt and playing".to_owned());
            }
            if let Some(local_catalog) = self.local_morph_catalog.take() {
                self.install_local_morphs(local_catalog)?;
            }
            if let Some(package) = old_temporary_package {
                let _ = fs::remove_dir_all(package);
            }
            self.update_viewport();
            return Ok(());
        }

        let mut shell = {
            let window = self
                .window
                .as_ref()
                .ok_or_else(|| "Studio window is not ready.".to_owned())?;
            let renderer = self
                .renderer
                .as_ref()
                .ok_or_else(|| "Studio renderer is not ready.".to_owned())?;
            StudioShell::new(window, renderer, &self.manifest_source)
        };
        shell.set_project_asset_available(true);
        shell.set_project_editable(
            !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
        );
        shell.set_source_manifest(&self.authored_manifest_source, false);
        shell.set_source_assets(load_source_assets(&self.project_root));
        shell.set_source_files(load_source_files(&self.project_root));
        shell.set_source_directories(load_source_directories(&self.project_root));
        shell.set_codex_project_root(self.project_root.clone());
        if let Err(message) = self.codex.set_project_root(&self.project_root) {
            shell.set_codex_chat_error(message);
        }
        if let Some(parent) = self.project_root.parent() {
            shell.set_new_project_parent(parent.to_path_buf());
        }
        shell.set_recent_projects(self.recent_projects.clone());
        self.shell = Some(shell);
        if let Some(local_catalog) = self.local_morph_catalog.take() {
            self.install_local_morphs(local_catalog)?;
        }
        if let Some(package) = old_temporary_package {
            let _ = fs::remove_dir_all(package);
        }
        self.update_viewport();
        Ok(())
    }
}

fn active_manifest_world_id(manifest: &Value) -> String {
    manifest
        .pointer("/launch/destinationWorld")
        .and_then(Value::as_str)
        .or_else(|| manifest.get("startWorld").and_then(Value::as_str))
        .unwrap_or("lobby")
        .to_owned()
}

fn manifest_world_mut<'a>(
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

fn ensure_scene_collection<'a>(
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

fn default_scene_object(kind: SceneObjectKind, index: usize) -> Value {
    let number = index + 1;
    match kind {
        SceneObjectKind::Block => serde_json::json!({
            "id": format!("block-{number}"),
            "position": [0, 1, 0],
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
    }
}

#[cfg(test)]
mod scene_edit_tests {
    use super::*;

    #[test]
    fn every_insertable_scene_kind_has_a_valid_position_and_collection() {
        let mut world = serde_json::json!({});
        for kind in SceneObjectKind::ALL {
            let collection = kind.collection();
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
    }
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
