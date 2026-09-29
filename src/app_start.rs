use super::*;

impl StudioApp {
    pub(crate) fn load(
        startup_project: Option<PathBuf>,
        morph_catalog_path: Option<PathBuf>,
    ) -> Result<Self, Box<dyn Error>> {
        // Always bootstrap the chooser's standalone session first. A project
        // supplied with --path is opened after the window exists through the
        // same background loader and commit path as a folder selected in the
        // start screen.
        let sources = load_game_sources(None)?;
        let initial_review_camera = sources.review_camera;
        let authored_manifest_source = sources.authored_manifest_source;
        let authored_scene_source = sources.authored_scene_source;
        let authoring_scene = authored_scene_source
            .as_deref()
            .map(cubacadabra_scene::parse_authoring_scene)
            .transpose()
            .map_err(StudioError)?;
        let manifest_source = sources.manifest_source;
        let script_source = sources.script_source;
        let game_root = sources.root;
        let project_root = sources.project_root;
        let standalone_preview = sources.standalone_preview;
        let temporary_package = sources.temporary_package;
        let recent_projects = load_recent_projects();
        let (local_morph_catalog, startup_morph_catalog) = if startup_project.is_some() {
            (None, morph_catalog_path)
        } else {
            (
                morph_catalog_path
                    .as_deref()
                    .map(load_local_morph_catalog)
                    .transpose()?,
                None,
            )
        };
        let mut client = ClientSession::load(&manifest_source, &script_source)?;
        client
            .engine_mut()
            .set_studio_movement_joystick_visible(false);
        let about_preview = cubacadabra_about_preview::AboutPreview::new().map_err(StudioError)?;
        if standalone_preview {
            let position = client
                .engine()
                .snapshot()
                .get(..3)
                .and_then(|values| values.try_into().ok());
            if let Some(position) = position {
                client
                    .engine_mut()
                    .reconcile_player(position, std::f32::consts::PI);
            }
        }
        let network = BackendClient::new(client.game_id()).map_err(StudioError)?;
        let codex = CodexClient::new(&project_root).map_err(StudioError)?;
        info!(
            "studio loaded: game_id={} standalone_preview={} root={}",
            client.game_id(),
            standalone_preview,
            game_root.display()
        );
        if local_morph_catalog.is_none() {
            network.request_morph_catalog();
        }

        Ok(Self {
            initial_review_camera,
            #[cfg(debug_assertions)]
            preview_probe: preview_probe::PreviewProbe::from_env(),
            startup_project,
            startup_morph_catalog,
            project_root,
            image_atlas: load_image_atlas(&game_root, &manifest_source)?,
            world_models: load_world_models(&game_root, &manifest_source)?,
            authored_manifest_source,
            authored_scene_source,
            authoring_scene_indices: authoring_scene
                .as_ref()
                .map(authoring_scene_indices)
                .unwrap_or_default(),
            authoring_scene,
            manifest_source,
            script_source,
            game_root,
            standalone_preview,
            temporary_package,
            codex,
            network,
            client,
            primary_autopilot: PreviewAutopilot::new(0),
            preview_peers: Vec::new(),
            preview_namespace: None,
            preview_player_ids: BTreeMap::new(),
            about_preview,
            recent_projects,
            window: None,
            renderer: None,
            shell: None,
            runtime_ui_revision: u64::MAX,
            pending_project_load: None,
            background_project_ready: None,
            prepared_project_ready: None,
            pending_roblox_import: None,
            renderer_uses_base_package_generation: true,
            codex_checkpoint: None,
            codex_changes: None,
            scene_undo: Vec::new(),
            scene_redo: Vec::new(),
            scene_drag_snapshot: None,
            scene_drag_cancelled_target: None,
            local_morph_catalog,
            pressed_keys: HashSet::new(),
            modifiers: ModifiersState::default(),
            jump_queued: false,
            mobile_sprint: false,
            morph_loadout: default_morph_loadout(),
            morph_request_serial: 0,
            pending_morph: None,
            registered_morphs: HashSet::new(),
            climb: false,
            joystick_input: (0.0, 0.0),
            pointer_position: None,
            pointer_active: false,
            camera_pointer_active: false,
            pan_pointer_active: false,
            scene_pointer_active: false,
            movement_pointer_active: false,
            movement_pointer_origin: None,
            ui_pointer_active: false,
            look_delta: (0.0, 0.0),
            pan_delta: (0.0, 0.0),
            zoom_delta: 0.0,
            last_frame: Instant::now(),
        })
    }

    pub(crate) fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let window = event_loop.create_window(
            WindowAttributes::default()
                .with_title(format!(
                    "Cubacadabra Studio — {}",
                    game_name(&self.game_root)
                ))
                .with_inner_size(LogicalSize::new(
                    DEFAULT_WINDOW_WIDTH,
                    DEFAULT_WINDOW_HEIGHT,
                )),
        )?;
        let size = window.inner_size();
        let display_handle = event_loop.owned_display_handle();
        let window_handle = window.window_handle()?.as_raw();
        let mut renderer = Renderer::new(
            display_handle,
            window_handle,
            size.width as f32,
            size.height as f32,
        )
        .ok_or_else(|| StudioError("the shared wgpu renderer could not start".to_owned()))?;

        if let Some(atlas) = &self.image_atlas {
            if !renderer.set_package_image_atlas(
                atlas.width,
                atlas.height,
                &atlas.pixels,
                atlas.regions.clone(),
            ) {
                return Err(Box::new(StudioError(
                    "the game's image atlas could not be uploaded".to_owned(),
                )));
            }
        }
        for model in &self.world_models {
            renderer
                .register_world_mesh(&model.id, &model.bytes)
                .map_err(|error| {
                    StudioError(format!("world model {} was rejected: {error}", model.id))
                })?;
        }

        let mut shell = StudioShell::new(&window, &renderer, &self.manifest_source);
        shell.set_about_preview_texture(renderer.device(), renderer.about_preview_texture());
        shell.set_review_camera(self.initial_review_camera);
        shell.set_project_asset_available(!self.standalone_preview);
        shell.set_project_editable(
            !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
        );
        if shell.project_is_editable() {
            shell.set_playing(false);
        }
        shell.set_source_manifest(&self.authored_manifest_source, false);
        shell.set_source_scene(self.authored_scene_source.as_deref(), false);
        shell.set_source_assets(load_source_assets(&self.project_root));
        shell.set_source_files(load_source_files(&self.project_root));
        shell.set_source_directories(load_source_directories(&self.project_root));
        shell.set_codex_project_root(self.project_root.clone());
        if self.standalone_preview {
            shell.set_start_screen(true);
            shell.set_recent_projects(self.recent_projects.clone());
        }
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.shell = Some(shell);
        if let Some(local_catalog) = self.local_morph_catalog.take() {
            self.install_local_morphs(local_catalog)
                .map_err(|message| Box::new(StudioError(message)) as Box<dyn Error>)?;
        }
        if let Some(project) = self.startup_project.take() {
            let morph_catalog = self.startup_morph_catalog.take();
            self.start_project_load_with_catalog(project, morph_catalog);
        }
        self.update_viewport();
        self.request_redraw();
        Ok(())
    }
}
