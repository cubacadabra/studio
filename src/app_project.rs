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

    pub(crate) fn save_project_source(&mut self) {
        let editable = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable);
        if !editable {
            return;
        }
        let manifest_path = self.project_root.join("manifest.json");
        match write_atomic(&manifest_path, self.authored_manifest_source.as_bytes()) {
            Ok(()) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_source_manifest(&self.authored_manifest_source, false);
                    shell.set_notice("Project saved".to_owned());
                }
            }
            Err(message) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(message);
                }
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
        let (target, operation) = match request {
            SceneEditRequest::UpdateBlock {
                target,
                position,
                size,
            } => (target, SceneEditOperation::Update { position, size }),
            SceneEditRequest::DuplicateBlock { target } => (target, SceneEditOperation::Duplicate),
            SceneEditRequest::DeleteBlock { target } => (target, SceneEditOperation::Delete),
            SceneEditRequest::UpdateSignText { .. } => {
                return Err("sign text edit was not handled".to_owned());
            }
        };
        let (world_id, index) = parse_block_target(&target)?;
        let world = if world_id == "lobby" {
            &mut manifest
        } else {
            manifest
                .get_mut("worlds")
                .and_then(Value::as_object_mut)
                .and_then(|worlds| worlds.get_mut(&world_id))
                .ok_or_else(|| format!("scene world `{world_id}` was not found"))?
        };
        let blocks = world
            .get_mut("blocks")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| format!("scene world `{world_id}` has no blocks"))?;
        let block = blocks
            .get_mut(index)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| format!("scene block `{target}` was not found"))?;
        match operation {
            SceneEditOperation::Update { position, size } => {
                block.insert("position".to_owned(), serde_json::json!(position));
                block.insert("size".to_owned(), serde_json::json!(size));
            }
            SceneEditOperation::Duplicate => {
                let mut copy = Value::Object(block.clone());
                let copy_object = copy
                    .as_object_mut()
                    .ok_or_else(|| "scene block could not be duplicated".to_owned())?;
                let base_id = copy_object
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("platform");
                copy_object.insert(
                    "id".to_owned(),
                    Value::String(format!("{base_id}-copy-{}", blocks.len() + 1)),
                );
                blocks.push(copy);
            }
            SceneEditOperation::Delete => {
                blocks.remove(index);
            }
        }
        let source = serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("could not serialize the scene manifest: {error}"))?
            + "\n";
        self.authored_manifest_source = source.clone();
        if let Some(shell) = &mut self.shell {
            shell.set_source_manifest(&source, true);
            shell.set_notice(match operation {
                SceneEditOperation::Update { .. } => {
                    "Platform changed — save to keep it".to_owned()
                }
                SceneEditOperation::Duplicate => "Platform duplicated — save to keep it".to_owned(),
                SceneEditOperation::Delete => "Platform deleted — save to keep it".to_owned(),
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
        self.image_atlas = image_atlas;
        self.network = network;
        self.client = client;
        self.renderer_uses_base_package_generation = client_uses_base_package_generation;
        self.local_morph_catalog = local_morph_catalog;
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
        shell.set_codex_project_root(self.project_root.clone());
        if let Err(message) = self.codex.set_project_root(&self.project_root) {
            shell.set_codex_chat_error(message);
        }
        if let Some(parent) = self.project_root.parent() {
            shell.set_new_project_parent(parent.to_path_buf());
        }
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
