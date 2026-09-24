use super::*;

impl StudioApp {
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
        self.start_project_load_with_catalog(project, None);
    }

    pub(crate) fn start_project_load_with_catalog(
        &mut self,
        project: PathBuf,
        morph_catalog: Option<PathBuf>,
    ) {
        self.recent_projects = remember_recent_project(&project);
        if let Some(shell) = &mut self.shell {
            shell.set_recent_projects(self.recent_projects.clone());
        }
        self.start_project_load_with_mode(project, false, false, morph_catalog);
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
        self.start_project_load_with_mode(self.project_root.clone(), true, codex_rebuild, None);
    }

    pub(crate) fn start_project_load_with_mode(
        &mut self,
        project: PathBuf,
        preserve_editor: bool,
        codex_rebuild: bool,
        morph_catalog: Option<PathBuf>,
    ) {
        self.start_project_load_with_play_mode(
            project,
            preserve_editor,
            codex_rebuild,
            true,
            morph_catalog,
        );
    }

    fn start_project_load_with_play_mode(
        &mut self,
        project: PathBuf,
        preserve_editor: bool,
        codex_rebuild: bool,
        play_after_rebuild: bool,
        morph_catalog: Option<PathBuf>,
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
        let worker_morph_catalog = morph_catalog;
        let spawn = thread::Builder::new()
            .name("studio-project-loader".to_owned())
            .spawn(move || {
                let progress_sender = sender.clone();
                let result = load_project_in_background(
                    &worker_project,
                    worker_morph_catalog.as_deref(),
                    move |progress| {
                        let _ = progress_sender.send(ProjectLoadEvent::Progress(progress));
                    },
                );
                let _ = sender.send(ProjectLoadEvent::Finished(result));
            });
        match spawn {
            Ok(_) => {
                self.pending_project_load = Some(PendingProjectLoad {
                    project,
                    preserve_editor,
                    codex_rebuild,
                    play_after_rebuild,
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
            let (project, preserve_editor, codex_rebuild, play_after_rebuild) = self
                .pending_project_load
                .take()
                .map(|load| {
                    (
                        load.project,
                        load.preserve_editor,
                        load.codex_rebuild,
                        load.play_after_rebuild,
                    )
                })
                .unwrap_or_default();
            self.background_project_ready = Some((
                project,
                preserve_editor,
                codex_rebuild,
                play_after_rebuild,
                result,
            ));
        }
    }

    pub(crate) fn prepare_ready_project_runtime(&mut self) {
        let Some((project, preserve_editor, codex_rebuild, play_after_rebuild, result)) =
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
        self.prepared_project_ready = Some((
            project,
            preserve_editor,
            codex_rebuild,
            play_after_rebuild,
            result,
        ));
    }

    pub(crate) fn commit_ready_project_load(&mut self) {
        let Some((project, preserve_editor, codex_rebuild, play_after_rebuild, result)) =
            self.prepared_project_ready.take()
        else {
            return;
        };
        let result = result.and_then(|prepared| {
            self.commit_project_load(prepared, preserve_editor, play_after_rebuild)
        });
        match result {
            Ok(()) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_notice(if preserve_editor {
                        if play_after_rebuild {
                            "Preview rebuilt and playing".to_owned()
                        } else {
                            "Preview applied — continue editing or press Play".to_owned()
                        }
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
        play_after_rebuild: bool,
    ) -> Result<(), String> {
        let preserved_review_camera = preserve_editor
            .then(|| self.shell.as_ref().map(StudioShell::review_camera))
            .flatten();
        let PreparedProjectLoad {
            background:
                BackgroundProjectLoad {
                    sources:
                        GameSources {
                            project_root,
                            root,
                            authored_manifest_source,
                            authored_scene_source,
                            manifest_source,
                            script_source,
                            review_camera,
                            standalone_preview,
                            temporary_package,
                        },
                    image_atlas,
                    world_models,
                    local_morph_catalog,
                },
            mut client,
            network,
        } = prepared;

        if !preserve_editor || self.project_root != project_root {
            self.invalidate_scene_history();
        }

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
        if let Some(renderer) = &mut self.renderer {
            let sources = world_models
                .iter()
                .map(|model| (model.id.as_str(), model.bytes.as_slice()))
                .collect::<Vec<_>>();
            renderer
                .replace_world_meshes(&sources)
                .map_err(|error| format!("world model replacement was rejected: {error}"))?;
        }
        let old_temporary_package = self.temporary_package.take();
        self.project_root = project_root;
        self.game_root = root;
        self.authored_manifest_source = authored_manifest_source;
        self.authored_scene_source = authored_scene_source;
        self.authoring_scene = self
            .authored_scene_source
            .as_deref()
            .map(cubacadabra_scene::parse_authoring_scene)
            .transpose()?;
        self.authoring_scene_indices = self
            .authoring_scene
            .as_ref()
            .map(authoring_scene_indices)
            .unwrap_or_default();
        self.manifest_source = manifest_source;
        self.standalone_preview = standalone_preview;
        self.temporary_package = temporary_package;
        if !standalone_preview {
            self.recent_projects = remember_recent_project(&self.project_root);
        }
        self.image_atlas = image_atlas;
        self.world_models = world_models;
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
                shell.restore_review_camera(preserved_review_camera.unwrap_or(review_camera));
                shell.set_project_editable(
                    !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
                );
                shell.set_source_files(load_source_files(&self.project_root));
                shell.set_source_manifest(&self.authored_manifest_source, false);
                shell.set_source_scene(self.authored_scene_source.as_deref(), false);
                shell.set_source_assets(load_source_assets(&self.project_root));
                shell.set_source_directories(load_source_directories(&self.project_root));
                shell.finish_project_loading(play_after_rebuild);
                shell.set_notice(if play_after_rebuild {
                    "Preview rebuilt and playing".to_owned()
                } else {
                    "Preview applied — continue editing or press Play".to_owned()
                });
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
            let mut shell = StudioShell::new(window, renderer, &self.manifest_source);
            shell.set_about_preview_texture(renderer.device(), renderer.about_preview_texture());
            shell
        };
        shell.set_review_camera(review_camera);
        shell.set_project_asset_available(true);
        shell.set_project_editable(
            !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
        );
        if shell.project_is_editable() {
            shell.set_playing(false);
        }
        shell.set_source_files(load_source_files(&self.project_root));
        shell.set_source_manifest(&self.authored_manifest_source, false);
        shell.set_source_scene(self.authored_scene_source.as_deref(), false);
        shell.set_source_assets(load_source_assets(&self.project_root));
        shell.set_source_directories(load_source_directories(&self.project_root));
        shell.set_codex_project_root(self.project_root.clone());
        if let Err(message) = self.codex.set_project_root(&self.project_root) {
            shell.set_codex_chat_error(message);
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
