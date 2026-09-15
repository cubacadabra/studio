use super::*;
impl StudioApp {
    pub(crate) fn load(
        game_root: Option<PathBuf>,
        morph_catalog_path: Option<PathBuf>,
    ) -> Result<Self, Box<dyn Error>> {
        let sources = load_game_sources(game_root)?;
        let authored_manifest_source = sources.authored_manifest_source;
        let manifest_source = sources.manifest_source;
        let script_source = sources.script_source;
        let game_root = sources.root;
        let project_root = sources.project_root;
        let standalone_preview = sources.standalone_preview;
        let temporary_package = sources.temporary_package;
        let morph_catalog_path =
            morph_catalog_path.or_else(|| discover_project_morph_catalog(&project_root));
        let recent_projects = if sources.standalone_preview {
            load_recent_projects()
        } else {
            remember_recent_project(&project_root)
        };
        let local_morph_catalog = morph_catalog_path
            .as_deref()
            .map(load_local_morph_catalog)
            .transpose()?;
        let mut client = ClientSession::load(&manifest_source, &script_source)?;
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
            project_root,
            image_atlas: load_image_atlas(&game_root, &manifest_source)?,
            authored_manifest_source,
            manifest_source,
            game_root,
            standalone_preview,
            temporary_package,
            codex,
            network,
            client,
            recent_projects,
            window: None,
            renderer: None,
            shell: None,
            runtime_ui_revision: u64::MAX,
            pending_project_load: None,
            background_project_ready: None,
            prepared_project_ready: None,
            renderer_uses_base_package_generation: true,
            codex_checkpoint: None,
            codex_changes: None,
            local_morph_catalog,
            pressed_keys: HashSet::new(),
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
            movement_pointer_active: false,
            movement_pointer_origin: None,
            ui_pointer_active: false,
            look_delta: (0.0, 0.0),
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
        let display_handle = window.display_handle()?.as_raw();
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

        let mut shell = StudioShell::new(&window, &renderer, &self.manifest_source);
        shell.set_project_asset_available(!self.standalone_preview);
        shell.set_project_editable(
            !self.standalone_preview && self.project_root.join("src/main.luau").is_file(),
        );
        shell.set_source_manifest(&self.authored_manifest_source, false);
        shell.set_source_assets(load_source_assets(&self.project_root));
        shell.set_source_files(load_source_files(&self.project_root));
        shell.set_source_directories(load_source_directories(&self.project_root));
        shell.set_codex_project_root(self.project_root.clone());
        if let Ok(parent) = env::current_dir() {
            shell.set_new_project_parent(parent);
        }
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
        self.update_viewport();
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn update_viewport(&mut self) {
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let fallback = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size.width as f32 / scale, size.height as f32 / scale),
        );
        let viewport = self
            .shell
            .as_ref()
            .map(StudioShell::runtime_viewport)
            .filter(|rect| rect.is_positive())
            .unwrap_or(fallback);
        self.client.set_ui_viewport_values(
            viewport.width(),
            viewport.height(),
            scale,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        if let Some(renderer) = &mut self.renderer {
            renderer.set_studio_viewport(Some([
                viewport.min.x * scale,
                viewport.min.y * scale,
                viewport.width() * scale,
                viewport.height() * scale,
            ]));
        }
    }

    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(size.width as f32, size.height as f32);
        }
        self.update_viewport();
    }

    pub(crate) fn render(&mut self) {
        self.drain_backend_events();
        self.drain_codex_events();
        self.commit_ready_project_load();
        self.prepare_ready_project_runtime();
        self.poll_project_load();
        #[cfg(target_os = "macos")]
        while let Some(command) = macos::take_menu_action() {
            if let Some(shell) = &mut self.shell {
                shell.execute_command(command);
            }
        }
        if self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_open_project_request)
        {
            self.choose_and_open_project();
        }
        if let Some(project) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_recent_project_request)
        {
            self.start_project_load(project);
        }
        if let Some(target) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_source_import_request)
        {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Add images to assets/images")
                .add_filter("PNG images", &["png"])
                .set_directory(self.project_root.join(&target));
            if let Some(window) = &self.window {
                dialog = dialog.set_parent(window);
            }
            if let Some(files) = dialog.pick_files() {
                self.import_source_images(files, target);
            }
        }
        let dropped_files = self
            .shell
            .as_mut()
            .map(StudioShell::take_dropped_files)
            .unwrap_or_default();
        if !dropped_files.is_empty() {
            self.import_source_images(dropped_files, PathBuf::from("assets/images"));
        }
        #[cfg(target_os = "macos")]
        if let Some((title, parent, error)) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_native_new_project_dialog)
            && let Some((title, parent)) =
                macos::show_new_project_dialog(&title, &parent, error.as_deref())
        {
            if let Some(shell) = &mut self.shell {
                shell.set_new_project_draft(title.clone(), parent.clone());
            }
            self.create_new_project(&title, &parent);
        }
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;

        let project_name = game_name(&self.project_root);
        if let Some(shell) = &mut self.shell {
            shell.set_active_morph_loadout(&self.morph_loadout);
        }
        if let Some(request) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_morph_request)
        {
            if matches!(request, wardrobe::Request::RetryCatalog) {
                self.network.request_morph_catalog();
            } else if let Err(message) = self.request_morph_change(request) {
                if let Some(shell) = &mut self.shell {
                    shell.set_notice(message);
                }
            }
        }
        let import_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_import_request);
        if import_requested {
            self.import_morph_glb();
        }
        let auth_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_auth_request);
        if auth_requested {
            if let Some(shell) = &mut self.shell {
                shell.set_auth_pending(true);
                shell.set_notice("Opening browser for sign-in…".to_owned());
            }
            self.network.begin_browser_auth();
        }
        let chatgpt_auth_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_chatgpt_auth_request);
        if chatgpt_auth_requested {
            if let Some(shell) = &mut self.shell {
                shell.set_chatgpt_pending();
            }
            if let Err(message) = self.codex.begin_chatgpt_login()
                && let Some(shell) = &mut self.shell
            {
                shell.set_chatgpt_error(message);
            }
        }
        let codex_chat_open_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_codex_chat_open_request);
        if codex_chat_open_requested
            && let Err(message) = self.codex.open_chat()
            && let Some(shell) = &mut self.shell
        {
            shell.set_codex_chat_error(message);
        }
        if let Some(request) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_codex_chat_send_request)
        {
            self.codex_checkpoint = snapshot_project_files(&self.project_root).ok();
            self.codex_changes = None;
            if let Err(message) = self.codex.send_chat_message(
                request.message,
                request.model.to_owned(),
                request.reasoning_effort.to_owned(),
            ) {
                self.codex_checkpoint = None;
                if let Some(shell) = &mut self.shell {
                    shell.set_codex_chat_error(message);
                }
            }
        }
        let cancel_codex_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_codex_cancel_request);
        if cancel_codex_requested {
            if let Some(shell) = &mut self.shell {
                shell.set_codex_chat_cancelling();
            }
            if let Err(message) = self.codex.cancel_chat_message()
                && let Some(shell) = &mut self.shell
            {
                shell.set_codex_chat_error(message);
            }
        }
        let draft_export_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_draft_export_request);
        if draft_export_requested {
            self.export_morph_draft();
        }
        // Morph requests can turn the loading veil on or commit the first
        // native v2 appearance. Prepare the overlay after those transitions
        // so the old bundled character never reaches a visible frame.
        let prepared_shell: Option<PreparedShell> = match (&mut self.shell, &self.window) {
            (Some(shell), Some(window)) => Some(shell.prepare(window, &project_name)),
            _ => None,
        };
        let undo_codex_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_codex_undo_request);
        if undo_codex_requested {
            self.undo_codex_changes();
        }
        if let Some(edit) = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_scene_edit_request)
        {
            if let Err(message) = self.apply_scene_edit(edit) {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(message);
                }
            }
        }
        let save_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_save_request);
        if save_requested {
            if self.save_project_source()
                && let Some(action) = self
                    .shell
                    .as_mut()
                    .and_then(StudioShell::take_pending_project_action_after_save)
            {
                if let Some(shell) = &mut self.shell {
                    shell.apply_pending_project_action(action);
                }
            }
        }
        let rebuild_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_rebuild_and_play_request);
        if rebuild_requested {
            if self.save_project_source() {
                self.start_project_reload();
            }
        }
        let restart_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_restart_request);
        if restart_requested {
            let dirty = self
                .shell
                .as_ref()
                .is_some_and(StudioShell::project_is_dirty);
            if dirty {
                if let Some(shell) = &mut self.shell {
                    shell.set_playing(false);
                    shell.set_notice(
                        "Unsaved scene changes — use Rebuild & Play to save and restart."
                            .to_owned(),
                    );
                }
            } else {
                self.start_project_reload();
            }
        }
        let sidecar_export_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_sidecar_export_request);
        if sidecar_export_requested {
            self.export_morph_sidecar();
        }
        let sidecar_import_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_sidecar_import_request);
        if sidecar_import_requested {
            self.import_morph_sidecar();
        }
        #[cfg(not(target_os = "macos"))]
        let new_project_folder_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_new_project_folder_request);
        #[cfg(not(target_os = "macos"))]
        if new_project_folder_requested {
            if let Some(parent) = rfd::FileDialog::new()
                .set_title("Choose where to create the game")
                .pick_folder()
            {
                if let Some(shell) = &mut self.shell {
                    shell.set_new_project_parent(parent);
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let new_project_request = self
            .shell
            .as_mut()
            .and_then(StudioShell::take_new_project_request);
        #[cfg(not(target_os = "macos"))]
        if let Some((title, parent)) = new_project_request {
            self.create_new_project(&title, &parent);
        }
        let project_add_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_project_add_request);
        if project_add_requested {
            self.add_morph_to_game();
        }
        let pack_import_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_pack_import_request);
        if pack_import_requested {
            self.import_morph_pack();
        }
        let publish_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_publish_request);
        if publish_requested {
            self.publish_morph_pack();
        }
        let thumbnail_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_morph_thumbnail_request);
        if thumbnail_requested {
            self.generate_morph_thumbnail();
        }
        self.update_viewport();
        let project_loading = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::is_project_loading);
        let playing = !project_loading && self.shell.as_ref().is_none_or(StudioShell::is_playing);
        let morph_preview = !project_loading
            && self
                .shell
                .as_ref()
                .is_some_and(StudioShell::is_morphs_workspace);
        let controls_active = playing || morph_preview;

        let mut forward = if controls_active {
            axis(
                &self.pressed_keys,
                &[KeyCode::KeyW, KeyCode::ArrowUp],
                &[KeyCode::KeyS, KeyCode::ArrowDown],
            )
        } else {
            0.0
        };
        let mut strafe = if controls_active {
            axis(
                &self.pressed_keys,
                &[KeyCode::KeyD, KeyCode::ArrowRight],
                &[KeyCode::KeyA, KeyCode::ArrowLeft],
            )
        } else {
            0.0
        };
        if controls_active {
            let (joystick_forward, joystick_strafe) = joystick_movement(self.joystick_input);
            forward += joystick_forward;
            strafe += joystick_strafe;
        }
        let length = (forward * forward + strafe * strafe).sqrt();
        let (forward, strafe) = if length > 1.0 {
            (forward / length, strafe / length)
        } else {
            (forward, strafe)
        };
        let sprint = self.mobile_sprint
            || self.pressed_keys.contains(&KeyCode::ShiftLeft)
            || self.pressed_keys.contains(&KeyCode::ShiftRight);
        self.client.set_input_values(
            forward,
            strafe,
            controls_active && sprint,
            controls_active && self.jump_queued,
            controls_active && self.climb,
            self.look_delta.0,
            self.look_delta.1,
            self.zoom_delta,
        );
        self.jump_queued = false;
        self.look_delta = (0.0, 0.0);
        self.zoom_delta = 0.0;
        self.dispatch_client_actions();
        if playing {
            self.client.step(delta);
        }
        self.drain_ui_events();
        self.refresh_runtime_ui_outline();
        self.dispatch_client_actions();
        if !self.standalone_preview
            && let Some(movement) = self.client.local_movement(length > 0.01, playing && sprint)
        {
            self.network.send_move(
                movement.position[0],
                movement.position[1],
                movement.position[2],
                movement.yaw,
                movement.moving,
                movement.sprinting,
                movement.respawn_event_id,
            );
        }

        if let Some(renderer) = &mut self.renderer {
            renderer.set_avatar_preview_mode(
                self.shell
                    .as_ref()
                    .is_some_and(StudioShell::is_morphs_workspace),
            );
            renderer.sync(self.client.engine());
            match (&mut self.shell, prepared_shell) {
                (Some(shell), Some(prepared)) => {
                    renderer.draw_with_overlay(|device, queue, encoder, destination| {
                        shell.paint(device, queue, encoder, destination, prepared);
                    });
                }
                _ => renderer.draw(),
            }
        }
    }

    pub(crate) fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn refresh_runtime_ui_outline(&mut self) {
        let revision = self.client.engine().studio_ui_document_revision();
        if revision == self.runtime_ui_revision {
            return;
        }
        let nodes = self.client.engine().studio_ui_nodes();
        self.runtime_ui_revision = revision;
        if let Some(shell) = &mut self.shell {
            shell.set_runtime_ui_nodes(&nodes);
        }
    }
}
impl Drop for StudioApp {
    fn drop(&mut self) {
        if let Some(package) = self.temporary_package.take() {
            let _ = fs::remove_dir_all(package);
        }
    }
}
