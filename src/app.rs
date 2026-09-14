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
            window: None,
            renderer: None,
            shell: None,
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
        shell.set_codex_project_root(self.project_root.clone());
        if let Ok(parent) = env::current_dir() {
            shell.set_new_project_parent(parent);
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
            self.save_project_source();
        }
        let rebuild_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_rebuild_and_play_request);
        if rebuild_requested {
            self.save_project_source();
            self.start_project_reload();
        }
        let restart_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_restart_request);
        if restart_requested {
            self.start_project_reload();
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

    pub(crate) fn import_morph_glb(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB model", &["glb"])
            .set_title("Import morph GLB")
            .pick_file()
        else {
            return;
        };
        let display_path = path.display().to_string();
        let result = fs::read(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))
            .and_then(|bytes| {
                let preview = decode_source_glb_preview(&bytes)
                    .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                let summary = inspect_source_glb_structure(&bytes)
                    .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                let lod_previews: [Option<MorphGlbPreviewMesh>; 3] = std::array::from_fn(|index| {
                    let level = ["near", "mid", "far"][index];
                    summary
                        .lod_candidates
                        .get(level)
                        .filter(|candidates| candidates.len() == 1)
                        .and_then(|candidates| candidates.first())
                        .and_then(|node| decode_source_glb_preview_node(&bytes, node).ok())
                });
                Ok((preview, summary, lod_previews))
            });
        if let Some(shell) = &mut self.shell {
            match result {
                Ok((preview, summary, lod_previews)) => {
                    shell.set_morph_preview(display_path, preview, summary);
                    for (level, preview) in lod_previews.into_iter().enumerate() {
                        if let Some(preview) = preview {
                            shell.set_morph_lod_preview(level, preview);
                        }
                    }
                }
                Err(message) => shell.set_morph_import_error(message),
            }
        }
        self.request_redraw();
    }

    pub(crate) fn export_morph_sidecar(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_sidecar_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, json)| {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph sidecar", &["json"])
                .set_file_name(&suggested_name)
                .set_title("Export morph sidecar")
                .save_file()
            else {
                return Err("Sidecar export cancelled.".to_owned());
            };
            fs::write(&path, json)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_sidecar_export_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn export_morph_draft(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_draft_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, json)| {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph draft", &["json"])
                .set_file_name(&suggested_name)
                .set_title("Save morph draft")
                .save_file()
            else {
                return Err("Morph draft save cancelled.".to_owned());
            };
            fs::write(&path, json)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_draft_export_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn import_morph_sidecar(&mut self) {
        let Some(sidecar_path) = rfd::FileDialog::new()
            .add_filter("Morph sidecar", &["json"])
            .set_title("Open morph sidecar")
            .pick_file()
        else {
            return;
        };
        let result = fs::read_to_string(&sidecar_path)
            .map_err(|error| format!("Could not read {}: {error}", sidecar_path.display()))
            .and_then(|manifest_source| {
                let draft = if is_morph_draft_json(&manifest_source) {
                    Some(parse_morph_draft_json(&manifest_source)?)
                } else {
                    None
                };
                let geometry_file = match &draft {
                    Some(draft) => draft.geometry_file.clone(),
                    None => source_manifest_geometry_file(&manifest_source)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?,
                };
                let glb_path = sidecar_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(geometry_file);
                let glb = fs::read(&glb_path).map_err(|error| {
                    format!(
                        "Could not read referenced GLB {}: {error}",
                        glb_path.display()
                    )
                })?;
                let (manifest, draft, preview, summary, lod_previews) = match draft {
                    Some(draft) => {
                        let preview = decode_source_glb_preview(&glb)
                            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                        let summary = inspect_source_glb_structure(&glb)
                            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                        let lod_previews: [Option<MorphGlbPreviewMesh>; 3] =
                            std::array::from_fn(|index| {
                                if draft.lod_nodes[index].is_empty() {
                                    None
                                } else {
                                    decode_source_glb_preview_node(&glb, &draft.lod_nodes[index])
                                        .ok()
                                }
                            });
                        (None, Some(draft), preview, summary, lod_previews)
                    }
                    None => {
                        let (manifest, preview, summary) =
                            inspect_source_sidecar(&manifest_source, &glb).map_err(
                                |diagnostics| Self::format_morph_diagnostics(&diagnostics),
                            )?;
                        let lod_previews: [Option<MorphGlbPreviewMesh>; 3] =
                            std::array::from_fn(|index| {
                                let level = ["near", "mid", "far"][index];
                                manifest.geometry.lod_nodes.get(level).and_then(|node| {
                                    decode_source_glb_preview_node(&glb, node).ok()
                                })
                            });
                        (Some(manifest), None, preview, summary, lod_previews)
                    }
                };
                Ok((
                    glb_path.display().to_string(),
                    manifest,
                    draft,
                    preview,
                    summary,
                    lod_previews,
                ))
            });
        if let Some(shell) = &mut self.shell {
            match result {
                Ok((glb_path, manifest, draft, preview, summary, lod_previews)) => {
                    if let Some(manifest) = manifest {
                        shell.set_morph_sidecar_preview(glb_path, manifest, preview, summary);
                    } else if let Some(draft) = draft {
                        shell.set_morph_draft_preview(glb_path, draft, preview, summary);
                    }
                    for (level, preview) in lod_previews.into_iter().enumerate() {
                        if let Some(preview) = preview {
                            shell.set_morph_lod_preview(level, preview);
                        }
                    }
                    shell.select_morph_preview_lod(Some(0));
                }
                Err(message) => shell.set_morph_import_error(message),
            }
        }
        self.request_redraw();
    }

    pub(crate) fn publish_morph_pack(&mut self) {
        let inputs = self
            .shell
            .as_ref()
            .map(StudioShell::morph_pack_inputs)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = inputs.and_then(|(glb_path, suggested_name, manifest_json)| {
            let glb = fs::read(&glb_path)
                .map_err(|error| format!("Could not read {}: {error}", glb_path))?;
            let (pack, summary) = compile_source_morph_pack(&manifest_json, &glb)
                .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph pack", &["morphpack"])
                .set_file_name(&suggested_name)
                .set_title("Publish morph pack")
                .save_file()
            else {
                return Err("Morph pack publish cancelled.".to_owned());
            };
            fs::write(&path, &pack)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
            Ok((summary.asset_id, summary.byte_len, pack))
        });
        let result = result.and_then(|(_, _, pack)| self.activate_morph_pack(&pack));
        if let Some(shell) = &mut self.shell {
            shell.set_morph_publish_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn add_morph_to_game(&mut self) {
        let result = if self.standalone_preview {
            Err("Open a game project before adding a character asset to it.".to_owned())
        } else {
            self.shell
                .as_ref()
                .map(StudioShell::morph_project_payload)
                .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()))
                .and_then(|(source_path, manifest_json, preview)| {
                    let glb = fs::read(&source_path)
                        .map_err(|error| format!("Could not read {}: {error}", source_path))?;
                    let manifest_json =
                        rewrite_manifest_geometry_file(&manifest_json, "source.glb")?;
                    let asset = source_manifest_asset(&manifest_json)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                    let asset_id = asset.id.to_string();
                    let slug = project_asset_slug(&asset_id)?;
                    let asset_directory = self.project_root.join("assets/characters").join(&slug);
                    fs::create_dir_all(&asset_directory).map_err(|error| {
                        format!(
                            "Could not create character asset directory {}: {error}",
                            asset_directory.display()
                        )
                    })?;
                    let (pack, _) = compile_source_morph_pack(&manifest_json, &glb)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                    let thumbnail = encode_morph_thumbnail_png(&preview)?;
                    write_atomic(&asset_directory.join("source.glb"), &glb)?;
                    write_atomic(
                        &asset_directory.join("source.morph.json"),
                        manifest_json.as_bytes(),
                    )?;
                    write_atomic(&asset_directory.join("runtime.morphpack"), &pack)?;
                    write_atomic(&asset_directory.join("thumbnail.png"), &thumbnail)?;

                    let catalog_path = self.project_root.join("assets/characters/catalog.json");
                    update_project_morph_catalog(
                        &catalog_path,
                        &asset_id,
                        &format!("{slug}/source.morph.json"),
                    )?;
                    let activated = self.activate_morph_pack(&pack)?;
                    Ok(activated)
                })
        };
        if let Some(shell) = &mut self.shell {
            shell.set_morph_project_result(result);
        }
        self.request_redraw();
    }

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

    pub(crate) fn import_morph_pack(&mut self) {
        let result = rfd::FileDialog::new()
            .add_filter("Morph pack", &["morphpack"])
            .set_title("Load morph pack")
            .pick_file()
            .ok_or_else(|| "Morph pack load cancelled.".to_owned())
            .and_then(|path| {
                let pack = fs::read(&path)
                    .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
                self.activate_morph_pack(&pack)
            });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_runtime_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn activate_morph_pack(&mut self, pack: &[u8]) -> Result<(String, usize), String> {
        debug!("decoding morph pack: bytes={}", pack.len());
        let decoded = decode_morph_pack(pack)
            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
        let asset_id = decoded.asset.id.to_string();
        debug!(
            "morph pack decoded: asset_id={} kind={:?} attachment_mode={:?}",
            asset_id, decoded.asset.kind, decoded.attachment.mode
        );
        self.renderer
            .as_mut()
            .ok_or_else(|| "The renderer is not ready for morph registration.".to_owned())?
            .register_morph_pack(pack)
            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
        debug!("morph pack registered with renderer: asset_id={}", asset_id);
        if let Some(shell) = &mut self.shell {
            shell.upsert_morph_asset(decoded.asset.clone());
        }
        self.registered_morphs.insert(asset_id.clone());
        self.request_morph_change(wardrobe::Request::Equip(decoded.asset.id))?;
        Ok((asset_id, pack.len()))
    }

    pub(crate) fn apply_morph_loadout(
        &mut self,
        mut loadout: cubacadabra_morphs::MorphLoadout,
    ) -> Result<(), String> {
        debug!(
            "resolving morph loadout: base={} parts={:?}",
            loadout.base, loadout.parts
        );
        let catalog = self
            .shell
            .as_ref()
            .ok_or_else(|| "Morph shell is not ready.".to_owned())?
            .morph_catalog()
            .clone();
        loadout.revision = self.client.engine().appearance_revision().saturating_add(1);
        let capabilities = morph_application::capabilities();
        cubacadabra_morphs::resolve_loadout(&catalog, &loadout, &capabilities).map_err(
            |diagnostics| {
                let message = Self::format_morph_diagnostics(&diagnostics);
                warn!("morph loadout resolution failed: {}", message);
                message
            },
        )?;
        let appearance = serde_json::to_string(&loadout)
            .map_err(|error| format!("Could not encode morph loadout: {error}"))?;
        debug!(
            "applying native morph loadout: base={} parts={:?}",
            loadout.base, loadout.parts
        );
        if self
            .client
            .engine_mut()
            .set_local_morph_loadout_json(&appearance)
            == 0
        {
            error!("engine rejected native morph loadout");
            return Err("The player appearance rejected that morph loadout.".to_owned());
        }
        self.morph_loadout = loadout;
        debug!(
            "morph loadout applied to engine: base={} parts={:?}",
            self.morph_loadout.base, self.morph_loadout.parts
        );
        Ok(())
    }

    pub(crate) fn generate_morph_thumbnail(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_thumbnail_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, preview)| {
            let png = encode_morph_thumbnail_png(&preview)?;
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name(&suggested_name)
                .set_title("Generate morph thumbnail")
                .save_file()
            else {
                return Err("Thumbnail generation cancelled.".to_owned());
            };
            fs::write(&path, png)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_thumbnail_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn format_morph_diagnostics(
        diagnostics: &[cubacadabra_morphs::MorphDiagnostic],
    ) -> String {
        let summary = diagnostics
            .iter()
            .take(3)
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("; ");
        if diagnostics.len() > 3 {
            format!("{summary}; and {} more", diagnostics.len() - 3)
        } else {
            summary
        }
    }

    pub(crate) fn pointer_event(&mut self, phase: u8, x: f32, y: f32) -> bool {
        self.client.ui_pointer_event(1, phase, x, y)
    }

    pub(crate) fn drain_ui_events(&mut self) {
        while let Some(source) = self.client.poll_ui_event_json() {
            let Ok(event) = serde_json::from_slice::<Value>(&source) else {
                continue;
            };
            let action = event
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let phase = event
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match action {
                "player.move" => {
                    let x = event.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    let y = event.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    self.joystick_input = (x, y);
                }
                "player.jump" if phase == "activate" => self.jump_queued = true,
                "player.run" if phase == "activate" => self.mobile_sprint = !self.mobile_sprint,
                "player.climb" if phase == "activate" => self.climb = !self.climb,
                _ => {}
            }
        }
    }

    pub(crate) fn handle_key(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        match event.state {
            ElementState::Pressed => {
                if code == KeyCode::Escape {
                    event_loop.exit();
                    return;
                }
                if code == KeyCode::Space && !event.repeat {
                    self.jump_queued = true;
                }
                self.pressed_keys.insert(code);
            }
            ElementState::Released => {
                self.pressed_keys.remove(&code);
            }
        }
    }

    pub(crate) fn handle_cursor_move(&mut self, x: f64, y: f64) {
        let scale = self.window.as_ref().map_or(1.0, Window::scale_factor) as f32;
        let logical = (x as f32 / scale, y as f32 / scale);
        if let Some(previous) = self.pointer_position {
            if (self.pointer_active || self.camera_pointer_active) && !self.ui_pointer_active {
                self.look_delta.0 += logical.0 - previous.0;
                self.look_delta.1 += logical.1 - previous.1;
            }
        }
        if self.movement_pointer_active
            && let Some(origin) = self.movement_pointer_origin
        {
            const JOYSTICK_RADIUS: f32 = 72.0;
            self.joystick_input = (
                ((logical.0 - origin.0) / JOYSTICK_RADIUS).clamp(-1.0, 1.0),
                ((logical.1 - origin.1) / JOYSTICK_RADIUS).clamp(-1.0, 1.0),
            );
        }
        self.pointer_position = Some(logical);
        if self.ui_pointer_active {
            if let Some((local_x, local_y)) = self.runtime_pointer(logical.0, logical.1, false) {
                self.pointer_event(1, local_x, local_y);
            }
        }
    }

    pub(crate) fn runtime_pointer(
        &self,
        x: f32,
        y: f32,
        require_inside: bool,
    ) -> Option<(f32, f32)> {
        let viewport = self.shell.as_ref()?.runtime_viewport();
        if !viewport.is_positive() || (require_inside && !viewport.contains(egui::pos2(x, y))) {
            return None;
        }
        Some((x - viewport.min.x, y - viewport.min.y))
    }

    pub(crate) fn handle_mouse_button(&mut self, state: ElementState, button: MouseButton) {
        let Some((x, y)) = self.pointer_position else {
            return;
        };
        let morph_preview = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::is_morphs_workspace);
        match state {
            ElementState::Pressed => {
                let Some((local_x, local_y)) = self.runtime_pointer(x, y, true) else {
                    return;
                };
                let camera_side = morph_preview
                    && self
                        .shell
                        .as_ref()
                        .is_some_and(|shell| local_x >= shell.runtime_viewport().width() * 0.5);
                match button {
                    MouseButton::Left if morph_preview && camera_side => {
                        self.camera_pointer_active = true;
                        self.pointer_active = false;
                        self.movement_pointer_active = false;
                        self.movement_pointer_origin = None;
                        self.joystick_input = (0.0, 0.0);
                        self.ui_pointer_active = false;
                    }
                    MouseButton::Left if morph_preview => {
                        self.movement_pointer_active = true;
                        self.movement_pointer_origin = Some((x, y));
                        self.joystick_input = (0.0, 0.0);
                        self.pointer_active = false;
                        self.ui_pointer_active = false;
                    }
                    MouseButton::Left => {
                        self.ui_pointer_active = self.pointer_event(0, local_x, local_y);
                        self.pointer_active = !self.ui_pointer_active;
                    }
                    MouseButton::Right => {
                        self.camera_pointer_active = true;
                        self.pointer_active = false;
                    }
                    _ => {}
                }
            }
            ElementState::Released => match button {
                MouseButton::Left if self.movement_pointer_active => {
                    self.movement_pointer_active = false;
                    self.movement_pointer_origin = None;
                    self.joystick_input = (0.0, 0.0);
                }
                MouseButton::Left if morph_preview && self.camera_pointer_active => {
                    self.camera_pointer_active = false;
                }
                MouseButton::Left => {
                    if self.ui_pointer_active {
                        if let Some((local_x, local_y)) = self.runtime_pointer(x, y, false) {
                            self.pointer_event(2, local_x, local_y);
                        }
                    }
                    self.ui_pointer_active = false;
                    self.pointer_active = false;
                }
                MouseButton::Right => self.camera_pointer_active = false,
                _ => {}
            },
        }
    }

    pub(crate) fn drain_backend_events(&mut self) {
        while let Some(event) = self.network.try_recv() {
            match event {
                BackendEvent::Connected => self.client.transport_connected(),
                BackendEvent::Disconnected => self.client.transport_disconnected(),
                BackendEvent::Message(source) => {
                    let _ = self.client.receive_text(&source);
                }
                BackendEvent::MorphCatalog(source) => self.install_published_morphs(&source),
                BackendEvent::MorphCatalogError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_catalog_error(message);
                        shell.set_notice("Morph catalog unavailable".into());
                    }
                }
                BackendEvent::MorphPacks { request_id, packs } => {
                    if let Err(message) = self.finish_morph_change(request_id, packs) {
                        if let Some(shell) = &mut self.shell {
                            shell.set_notice(message);
                        }
                    }
                }
                BackendEvent::MorphPacksError {
                    request_id,
                    message,
                } => {
                    if self
                        .pending_morph
                        .as_ref()
                        .is_some_and(|(serial, _)| *serial == request_id)
                    {
                        self.pending_morph = None;
                        if let Some(shell) = &mut self.shell {
                            shell.set_morph_loading(false);
                            shell.set_notice(format!("Appearance unchanged: {message}"));
                        }
                    }
                }
                BackendEvent::MorphThumbnail { url, bytes } => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_morph_thumbnail(url, &bytes);
                    }
                }
                BackendEvent::AuthStarted => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_notice(
                            "Finish signing in in your browser. Studio will continue automatically."
                                .to_owned(),
                        );
                    }
                }
                BackendEvent::AuthCompleted { user } => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_auth_completed(user);
                    }
                }
                BackendEvent::AuthError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_auth_error(message);
                    }
                }
            }
        }
    }

    pub(crate) fn drain_codex_events(&mut self) {
        while let Some(event) = self.codex.try_recv() {
            match event {
                CodexEvent::AccountStatus(account) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_account(account);
                    }
                }
                CodexEvent::BrowserOpened => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_browser_opened();
                    }
                }
                CodexEvent::LoginCompleted(account) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_connected(account);
                    }
                }
                CodexEvent::ChatReady => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_ready();
                    }
                }
                CodexEvent::WorkStatus(status) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_work_status(status);
                    }
                }
                CodexEvent::AssistantDelta(delta) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_delta(delta);
                    }
                }
                CodexEvent::AssistantMessage(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_message(message);
                    }
                }
                CodexEvent::ChatTurnCompleted => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_completed();
                        shell.set_notice("Change received — rebuilding preview…".to_owned());
                    }
                    let changes = self.capture_codex_changes();
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_changes(
                            changes
                                .iter()
                                .map(|change| change.relative_path.display().to_string())
                                .collect(),
                        );
                    }
                    if self
                        .shell
                        .as_ref()
                        .is_some_and(StudioShell::project_is_dirty)
                    {
                        self.save_project_source();
                    }
                    self.refresh_authored_manifest_from_disk();
                    self.start_codex_project_reload();
                }
                CodexEvent::ChatTurnCancelled => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_cancelled();
                        shell.set_notice(
                            "Codex stopped. The preview was not rebuilt; review or undo the changes."
                                .to_owned(),
                        );
                    }
                    let changes = self.capture_codex_changes();
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_changes(
                            changes
                                .iter()
                                .map(|change| change.relative_path.display().to_string())
                                .collect(),
                        );
                    }
                }
                CodexEvent::ChatError(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_codex_chat_error(message);
                    }
                }
                CodexEvent::Error(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_error(message);
                    }
                }
                CodexEvent::Unavailable(message) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_chatgpt_unavailable(message);
                    }
                }
            }
        }
    }

    pub(crate) fn dispatch_client_actions(&mut self) {
        for action in self.client.poll_actions() {
            if self.standalone_preview {
                continue;
            }
            match action {
                ClientAction::SetWorld(world_id) => self.network.set_world(world_id),
                ClientAction::SendText(source) => self.network.send(source),
            }
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
