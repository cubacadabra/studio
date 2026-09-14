use super::*;
use egui_wgpu::wgpu;

impl StudioShell {
    pub(crate) fn new(
        window: &Window,
        game_renderer: &GameRenderer,
        manifest_source: &str,
    ) -> Self {
        let context = egui::Context::default();
        configure_context(&context);
        let state = EguiState::new(
            context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(4_096),
        );
        let renderer = EguiRenderer::new(
            game_renderer.device(),
            game_renderer.studio_overlay_format(),
            RendererOptions::default(),
        );
        #[cfg(target_os = "macos")]
        install_system_icon_textures(&context);
        Self::from_egui_with_manifest(context, state, renderer, manifest_source)
    }

    #[cfg(test)]
    pub(crate) fn from_egui(
        context: egui::Context,
        state: EguiState,
        renderer: EguiRenderer,
    ) -> Self {
        Self::from_egui_with_manifest(context, state, renderer, "{}")
    }

    pub(crate) fn from_egui_with_manifest(
        context: egui::Context,
        state: EguiState,
        renderer: EguiRenderer,
        manifest_source: &str,
    ) -> Self {
        let logo_texture = load_logo_texture(&context);
        let scene_outline =
            SceneOutline::parse(manifest_source).unwrap_or_else(|_| SceneOutline::empty());
        let selected_scene = scene_outline.initial_selection.clone();
        let expanded_scene = scene_outline.initial_expanded.clone();
        let selected_world_asset = scene_outline
            .assets
            .first()
            .map(|asset| asset.name.clone())
            .unwrap_or_default();
        Self {
            context,
            state,
            renderer,
            workspace: Workspace::default(),
            runtime_viewport: Rect::NOTHING,
            scene_outline,
            expanded_scene,
            selected_scene,
            scene_editor_target: String::new(),
            scene_editor_position: [0.0; 3],
            scene_editor_size: [1.0; 3],
            scene_editor_text: String::new(),
            selected_world_asset,
            selected_asset: "forest-grass",
            test_tool: "Sessions",
            asset_filter: "All",
            // Studio historically launched directly into its live runtime.
            // Keep that behavior now that the shell has a Play/Stop toggle so
            // keyboard and engine-owned pointer controls work immediately.
            playing: true,
            project_editable: false,
            project_dirty: false,
            project_error: None,
            scene_edit_requested: None,
            save_requested: false,
            rebuild_and_play_requested: false,
            restart_requested: false,
            notice: "Ready".to_owned(),
            search_query: String::new(),
            morph_query: String::new(),
            morph_catalog: parse_catalog(include_str!(
                "../../../rust/assets/characters/morph_catalog.json"
            ))
            .expect("bundled morph catalog must be valid"),
            morph_artifacts: BTreeMap::new(),
            morph_request: None,
            morph_starters: true,
            morph_loading: false,
            morph_catalog_ready: false,
            morph_catalog_error: None,
            morph_thumbnails: BTreeMap::new(),
            active_loadout: crate::default_morph_loadout(),
            active_morphs: BTreeSet::new(),
            selected_morph: MorphAssetId::parse("cuba:base/person.v1")
                .expect("built-in morph ID must be valid"),
            morph_import_requested: false,
            morph_sidecar_import_requested: false,
            morph_preview_path: None,
            morph_preview: None,
            morph_lod_previews: [None, None, None],
            morph_preview_lod: None,
            morph_source_summary: None,
            morph_draft_asset: None,
            morph_attachment_joint: "head".to_owned(),
            morph_attachment_translation: [0.0; 3],
            morph_attachment_rotation: [0.0, 0.0, 0.0, 1.0],
            morph_attachment_scale: [1.0; 3],
            morph_lod_nodes: [String::new(), String::new(), String::new()],
            morph_draft_status: None,
            morph_draft_export_requested: false,
            morph_sidecar_export_requested: false,
            morph_project_add_requested: false,
            morph_pack_import_requested: false,
            morph_publish_requested: false,
            morph_thumbnail_requested: false,
            morph_import_error: None,
            project_asset_available: true,
            auth_requested: false,
            auth_pending: false,
            auth_user: None,
            chatgpt_auth_requested: false,
            chatgpt_pending: false,
            chatgpt_available: true,
            chatgpt_account: None,
            chatgpt_error: None,
            codex_project_root: PathBuf::new(),
            codex_chat_open: false,
            codex_chat_open_requested: false,
            codex_chat_send_requested: None,
            codex_chat_messages: Vec::new(),
            codex_chat_draft: String::new(),
            codex_chat_model: CODEX_CHAT_CURRENT_MODEL,
            codex_chat_reasoning_effort: CODEX_CHAT_DEFAULT_EFFORT,
            codex_chat_ready: false,
            codex_activity: CodexActivity::Idle,
            codex_cancel_requested: false,
            codex_cancel_sent: false,
            codex_chat_error: None,
            codex_change_files: None,
            codex_source_change_count: 0,
            codex_change_review_open: false,
            codex_undo_requested: false,
            open_project_requested: false,
            project_loading: None,
            new_project_dialog_open: false,
            new_project_title: String::new(),
            new_project_parent: PathBuf::from("."),
            #[cfg(not(target_os = "macos"))]
            new_project_folder_requested: false,
            #[cfg(not(target_os = "macos"))]
            new_project_create_requested: false,
            new_project_error: None,
            #[cfg(not(target_os = "macos"))]
            new_project_title_focus_requested: false,
            logo_texture,
            roughness: 0.72,
            pending_textures_delta: egui::TexturesDelta::default(),
        }
    }

    pub(crate) fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    pub(crate) fn runtime_viewport(&self) -> Rect {
        self.runtime_viewport
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playing
    }

    pub(crate) fn is_morphs_workspace(&self) -> bool {
        self.workspace == Workspace::Morphs
    }

    pub(crate) fn set_notice(&mut self, notice: String) {
        self.notice = notice;
    }

    pub(crate) fn set_project_editable(&mut self, editable: bool) {
        self.project_editable = editable;
    }

    pub(crate) fn project_is_editable(&self) -> bool {
        self.project_editable
    }

    pub(crate) fn project_is_dirty(&self) -> bool {
        self.project_dirty
    }

    pub(crate) fn set_source_manifest(&mut self, source: &str, dirty: bool) {
        let Ok(outline) = SceneOutline::parse(source) else {
            return;
        };
        let selected = self
            .scene_outline
            .root
            .find(&self.selected_scene)
            .is_some_and(|_| outline.root.find(&self.selected_scene).is_some())
            .then(|| self.selected_scene.clone())
            .unwrap_or_else(|| outline.initial_selection.clone());
        self.scene_outline = outline;
        self.selected_scene = selected;
        self.project_dirty = dirty;
        self.scene_editor_target.clear();
        self.scene_editor_text.clear();
    }

    pub(crate) fn set_project_error(&mut self, message: String) {
        self.project_error = Some(message.clone());
        self.notice = message;
    }

    pub(crate) fn finish_project_loading(&mut self) {
        self.project_loading = None;
        self.project_error = None;
        self.playing = true;
    }

    pub(crate) fn begin_game_rebuild(&mut self) {
        if self.project_loading.is_some() {
            return;
        }
        self.project_loading = Some(ProjectLoadingState {
            progress: 0.0,
            previous_outline: self.scene_outline.clone(),
            previous_expanded: self.expanded_scene.clone(),
            previous_selection: self.selected_scene.clone(),
            previous_world_asset: self.selected_world_asset.clone(),
            previous_workspace: self.workspace,
        });
        self.project_error = None;
        self.notice = "Rebuilding preview…".to_owned();
    }

    pub(crate) fn take_scene_edit_request(&mut self) -> Option<SceneEditRequest> {
        self.scene_edit_requested.take()
    }

    pub(crate) fn take_save_request(&mut self) -> bool {
        std::mem::take(&mut self.save_requested)
    }

    pub(crate) fn take_rebuild_and_play_request(&mut self) -> bool {
        std::mem::take(&mut self.rebuild_and_play_requested)
    }

    pub(crate) fn take_restart_request(&mut self) -> bool {
        std::mem::take(&mut self.restart_requested)
    }

    pub(crate) fn take_auth_request(&mut self) -> bool {
        std::mem::take(&mut self.auth_requested)
    }

    pub(crate) fn set_auth_pending(&mut self, pending: bool) {
        self.auth_pending = pending;
    }

    pub(crate) fn set_auth_completed(&mut self, user: crate::network::AuthUser) {
        self.auth_pending = false;
        self.auth_user = Some(user.clone());
        self.notice = format!("Signed in as {}", user.name);
    }

    pub(crate) fn set_auth_error(&mut self, message: String) {
        self.auth_pending = false;
        self.notice = message;
    }

    pub(crate) fn take_chatgpt_auth_request(&mut self) -> bool {
        std::mem::take(&mut self.chatgpt_auth_requested)
    }

    pub(crate) fn set_codex_project_root(&mut self, project_root: PathBuf) {
        self.codex_project_root = project_root;
    }

    pub(crate) fn take_codex_chat_open_request(&mut self) -> bool {
        std::mem::take(&mut self.codex_chat_open_requested)
    }

    pub(crate) fn take_codex_chat_send_request(&mut self) -> Option<CodexChatSendRequest> {
        self.codex_chat_send_requested.take()
    }

    pub(crate) fn set_codex_chat_ready(&mut self) {
        self.codex_chat_ready = true;
        self.codex_chat_error = None;
    }

    pub(crate) fn set_codex_chat_delta(&mut self, _delta: String) {
        self.codex_activity = self.codex_activity.after_agent_progress();
    }

    pub(crate) fn set_codex_work_status(&mut self, status: CodexWorkStatus) {
        if !self.codex_activity.is_cancellable() {
            return;
        }
        self.codex_activity = match status {
            CodexWorkStatus::Thinking => CodexActivity::Thinking,
            CodexWorkStatus::Editing => CodexActivity::Editing,
            CodexWorkStatus::Checking => CodexActivity::Checking,
            CodexWorkStatus::Working => CodexActivity::Working,
        };
    }

    pub(crate) fn set_codex_chat_message(&mut self, _text: String) {
        // An agent-message item can complete while the turn continues with
        // more tool work. Only turn/completed advances Studio to rebuilding.
        self.codex_activity = self.codex_activity.after_agent_progress();
    }

    pub(crate) fn set_codex_chat_completed(&mut self) {
        self.codex_activity = CodexActivity::Rebuilding;
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
    }

    pub(crate) fn set_codex_preview_rebuilt(&mut self) {
        self.finish_codex_activity("Done — preview rebuilt and playing.");
    }

    pub(crate) fn set_codex_preview_rebuild_failed(&mut self, message: &str) {
        self.finish_codex_activity("The change was made, but the preview could not be rebuilt.");
        self.codex_chat_error = Some(format!("Rebuild failed: {message}"));
    }

    pub(crate) fn set_codex_chat_error(&mut self, message: String) {
        self.finish_codex_activity("The request could not be completed.");
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
        self.codex_chat_error = Some(message);
    }

    pub(crate) fn set_codex_chat_cancelling(&mut self) {
        self.codex_cancel_requested = true;
        self.codex_activity = CodexActivity::Cancelling;
        self.notice = "Stopping Codex…".to_owned();
    }

    pub(crate) fn set_codex_chat_cancelled(&mut self) {
        self.finish_codex_activity("Request cancelled. The preview was not rebuilt.");
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
    }

    pub(crate) fn finish_codex_activity(&mut self, message: &str) {
        let was_active = self.codex_activity.is_active();
        self.codex_activity = CodexActivity::Idle;
        if was_active {
            self.codex_chat_messages.push(CodexChatMessage {
                role: CodexChatRole::Assistant,
                text: message.to_owned(),
            });
        }
    }

    pub(crate) fn set_codex_changes(&mut self, files: Vec<String>) {
        let source_change_count = files.iter().filter(|file| file.ends_with(".luau")).count();
        let visible_files = files
            .into_iter()
            .filter(|file| !file.ends_with(".luau"))
            .collect::<Vec<_>>();
        self.codex_source_change_count = source_change_count;
        self.codex_change_files = (!visible_files.is_empty()).then_some(visible_files);
        self.codex_change_review_open = false;
    }

    pub(crate) fn clear_codex_changes(&mut self) {
        self.codex_change_files = None;
        self.codex_source_change_count = 0;
        self.codex_change_review_open = false;
    }

    pub(crate) fn take_codex_undo_request(&mut self) -> bool {
        std::mem::take(&mut self.codex_undo_requested)
    }

    pub(crate) fn take_codex_cancel_request(&mut self) -> bool {
        if !self.codex_activity.is_active()
            || !self.codex_cancel_requested
            || self.codex_cancel_sent
        {
            return false;
        }
        self.codex_cancel_sent = true;
        true
    }

    pub(crate) fn set_chatgpt_pending(&mut self) {
        self.chatgpt_pending = true;
        self.chatgpt_available = true;
        self.chatgpt_error = None;
        self.notice = "Opening browser for ChatGPT sign-in…".to_owned();
    }

    pub(crate) fn set_chatgpt_account(&mut self, account: Option<ChatGptAccount>) {
        if self.chatgpt_pending {
            return;
        }
        self.chatgpt_available = true;
        self.chatgpt_account = account;
        self.chatgpt_error = None;
    }

    pub(crate) fn set_chatgpt_browser_opened(&mut self) {
        self.chatgpt_pending = true;
        self.notice =
            "Finish signing in with ChatGPT in your browser. Studio will continue automatically."
                .to_owned();
    }

    pub(crate) fn set_chatgpt_connected(&mut self, account: ChatGptAccount) {
        self.chatgpt_pending = false;
        self.chatgpt_available = true;
        self.chatgpt_error = None;
        self.notice = account
            .email
            .as_deref()
            .map(|email| format!("ChatGPT connected as {email}"))
            .unwrap_or_else(|| "ChatGPT connected".to_owned());
        self.chatgpt_account = Some(account);
    }

    pub(crate) fn set_chatgpt_error(&mut self, message: String) {
        self.chatgpt_pending = false;
        self.chatgpt_available = true;
        self.chatgpt_error = Some(message.clone());
        self.notice = message;
    }

    pub(crate) fn set_chatgpt_unavailable(&mut self, message: String) {
        let was_pending = self.chatgpt_pending;
        self.chatgpt_pending = false;
        self.chatgpt_available = false;
        self.chatgpt_account = None;
        self.chatgpt_error = Some(message.clone());
        if was_pending {
            self.notice = message;
        }
    }

    pub(crate) fn set_project_asset_available(&mut self, available: bool) {
        self.project_asset_available = available;
    }

    pub(crate) fn take_open_project_request(&mut self) -> bool {
        std::mem::take(&mut self.open_project_requested)
    }

    pub(crate) fn begin_project_loading(&mut self) {
        if self.project_loading.is_some() {
            return;
        }
        let empty = SceneOutline::empty();
        let empty_selection = empty.initial_selection.clone();
        let empty_expanded = empty.initial_expanded.clone();
        self.project_loading = Some(ProjectLoadingState {
            progress: 0.0,
            previous_outline: std::mem::replace(&mut self.scene_outline, empty),
            previous_expanded: std::mem::replace(&mut self.expanded_scene, empty_expanded),
            previous_selection: std::mem::replace(&mut self.selected_scene, empty_selection),
            previous_world_asset: std::mem::take(&mut self.selected_world_asset),
            previous_workspace: std::mem::replace(&mut self.workspace, Workspace::World),
        });
        self.notice = "Loading project…".to_owned();
    }

    pub(crate) fn set_project_loading_progress(&mut self, progress: f32) {
        if let Some(loading) = &mut self.project_loading {
            loading.progress = loading.progress.max(progress.clamp(0.0, 1.0));
        }
    }

    pub(crate) fn cancel_project_loading(&mut self) {
        let Some(loading) = self.project_loading.take() else {
            return;
        };
        self.scene_outline = loading.previous_outline;
        self.expanded_scene = loading.previous_expanded;
        self.selected_scene = loading.previous_selection;
        self.selected_world_asset = loading.previous_world_asset;
        self.workspace = loading.previous_workspace;
    }

    pub(crate) fn is_project_loading(&self) -> bool {
        self.project_loading.is_some()
    }

    pub(crate) fn set_new_project_parent(&mut self, parent: PathBuf) {
        self.new_project_parent = parent;
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn take_native_new_project_dialog(
        &mut self,
    ) -> Option<(String, PathBuf, Option<String>)> {
        if !std::mem::take(&mut self.new_project_dialog_open) {
            return None;
        }
        Some((
            self.new_project_title.clone(),
            self.new_project_parent.clone(),
            self.new_project_error.take(),
        ))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_new_project_draft(&mut self, title: String, parent: PathBuf) {
        self.new_project_title = title;
        self.new_project_parent = parent;
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn take_new_project_folder_request(&mut self) -> bool {
        std::mem::take(&mut self.new_project_folder_requested)
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn take_new_project_request(&mut self) -> Option<(String, PathBuf)> {
        if !std::mem::take(&mut self.new_project_create_requested) {
            return None;
        }
        Some((
            self.new_project_title.trim().to_owned(),
            self.new_project_parent.clone(),
        ))
    }

    pub(crate) fn set_new_project_error(&mut self, message: String) {
        self.new_project_dialog_open = true;
        self.new_project_error = Some(message);
        #[cfg(not(target_os = "macos"))]
        {
            self.new_project_title_focus_requested = true;
        }
    }

    pub(crate) fn set_new_project_created(&mut self, project: &Path) {
        self.new_project_dialog_open = false;
        self.new_project_error = None;
        self.notice = format!("Created {}", project.display());
    }

    pub(crate) fn set_remote_morph_catalog(&mut self, source: &str) -> Result<usize, String> {
        let published = crate::wardrobe::PublishedCatalog::parse(source)?;
        let count = published.catalog.assets.len();
        self.morph_catalog = published.catalog;
        self.morph_artifacts = published.artifacts;
        self.morph_catalog_ready = true;
        self.morph_catalog_error = None;
        Ok(count)
    }

    pub(crate) fn set_local_morph_catalog(&mut self, catalog: MorphCatalog) {
        self.morph_catalog = catalog;
        self.morph_artifacts.clear();
        self.morph_catalog_ready = true;
        self.morph_catalog_error = None;
    }

    pub(crate) fn morph_artifact(&self, id: &MorphAssetId) -> Option<&crate::wardrobe::Artifact> {
        self.morph_artifacts.get(id)
    }

    pub(crate) fn set_catalog_error(&mut self, message: String) {
        self.morph_catalog_error = Some(message);
    }

    pub(crate) fn set_morph_thumbnail(&mut self, url: String, bytes: &[u8]) {
        if let Ok(image) = image::load_from_memory(bytes) {
            if image.width() > 1024 || image.height() > 1024 {
                return;
            }
            let rgba = image.to_rgba8();
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            );
            let texture = self
                .context
                .load_texture(&url, image, egui::TextureOptions::LINEAR);
            self.morph_thumbnails.insert(url, texture);
        }
    }

    pub(crate) fn set_morph_loading(&mut self, loading: bool) {
        self.morph_loading = loading;
    }

    pub(crate) fn select_morph(&mut self, id: MorphAssetId) {
        self.selected_morph = id;
    }

    pub(crate) fn upsert_morph_asset(
        &mut self,
        definition: cubacadabra_morphs::MorphAssetDefinition,
    ) {
        self.morph_catalog
            .assets
            .retain(|asset| asset.id != definition.id);
        self.morph_catalog.assets.push(definition);
    }

    pub(crate) fn morph_catalog(&self) -> &MorphCatalog {
        &self.morph_catalog
    }

    pub(crate) fn take_morph_request(&mut self) -> Option<crate::wardrobe::Request> {
        self.morph_request.take()
    }

    pub(crate) fn set_active_morph_loadout(&mut self, loadout: &cubacadabra_morphs::MorphLoadout) {
        self.active_morphs = crate::wardrobe::selected_ids(loadout)
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        self.active_loadout = loadout.clone();
    }

    pub(crate) fn take_morph_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_import_requested)
    }

    pub(crate) fn take_morph_sidecar_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_sidecar_import_requested)
    }

    pub(crate) fn take_morph_project_add_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_project_add_requested)
    }

    pub(crate) fn take_morph_pack_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_pack_import_requested)
    }

    pub(crate) fn take_morph_sidecar_export_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_sidecar_export_requested)
    }

    pub(crate) fn take_morph_draft_export_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_draft_export_requested)
    }

    pub(crate) fn take_morph_publish_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_publish_requested)
    }

    pub(crate) fn take_morph_thumbnail_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_thumbnail_requested)
    }

    pub(crate) fn morph_attachment(&self) -> MorphAttachment {
        MorphAttachment {
            mode: MorphAttachmentMode::Rigid,
            joint: self.morph_attachment_joint.trim().to_owned(),
            translation: self.morph_attachment_translation,
            rotation: self.morph_attachment_rotation,
            scale: self.morph_attachment_scale,
        }
    }

    pub(crate) fn morph_sidecar_payload(&self) -> Result<(String, String), String> {
        let path = self
            .morph_preview_path
            .as_deref()
            .ok_or_else(|| "Import a GLB before exporting its sidecar.".to_owned())?;
        let preview = self
            .morph_preview
            .as_ref()
            .ok_or_else(|| "The imported GLB has no preview mesh to save.".to_owned())?;
        let asset = self
            .morph_draft_asset
            .as_ref()
            .or_else(|| self.morph_catalog.asset(&self.selected_morph))
            .ok_or_else(|| "Select a catalog asset before exporting its sidecar.".to_owned())?;
        let geometry_file = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "The imported GLB needs a safe filename for its sidecar.".to_owned())?
            .to_owned();
        let lod_nodes = [
            self.morph_lod_nodes[0].trim(),
            self.morph_lod_nodes[1].trim(),
            self.morph_lod_nodes[2].trim(),
        ];
        let fallback_triangle_count = u32::try_from(preview.indices.len() / 3)
            .map_err(|_| "The preview mesh triangle count is too large.".to_owned())?;
        let summary = self
            .morph_source_summary
            .as_ref()
            .ok_or_else(|| "The imported GLB has no source summary to save.".to_owned())?;
        let triangle_counts = lod_nodes.map(|node| {
            summary
                .node_triangle_counts
                .get(node)
                .copied()
                .unwrap_or(fallback_triangle_count)
                .max(1)
        });
        let json = build_source_manifest_json(
            asset,
            geometry_file,
            self.morph_attachment(),
            lod_nodes,
            triangle_counts,
        )
        .map_err(|diagnostics| {
            diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        let suggested_name = format!(
            "{}.morph.json",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, json))
    }

    pub(crate) fn morph_pack_inputs(&self) -> Result<(String, String, String), String> {
        let (sidecar_name, manifest_json) = self.morph_sidecar_payload()?;
        let glb_path = self
            .morph_preview_path
            .as_ref()
            .ok_or_else(|| "Import a GLB before publishing its pack.".to_owned())?
            .clone();
        let suggested_name = format!(
            "{}.morphpack",
            std::path::Path::new(&sidecar_name)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((glb_path, suggested_name, manifest_json))
    }

    pub(crate) fn morph_project_payload(
        &self,
    ) -> Result<(String, String, MorphGlbPreviewMesh), String> {
        let path = self
            .morph_preview_path
            .clone()
            .ok_or_else(|| "Import a GLB before adding it to this game.".to_owned())?;
        let (_, manifest_json) = self.morph_sidecar_payload()?;
        let preview = self
            .morph_preview
            .clone()
            .ok_or_else(|| "The imported GLB has no preview mesh to add.".to_owned())?;
        Ok((path, manifest_json, preview))
    }

    pub(crate) fn morph_draft_payload(&self) -> Result<(String, String), String> {
        let path = self
            .morph_preview_path
            .as_deref()
            .ok_or_else(|| "Import a GLB before saving its draft.".to_owned())?;
        let asset = self
            .morph_draft_asset
            .as_ref()
            .or_else(|| self.morph_catalog.asset(&self.selected_morph))
            .ok_or_else(|| "Select a catalog asset before saving its draft.".to_owned())?;
        let geometry_file = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "The imported GLB needs a safe filename for its draft.".to_owned())?
            .to_owned();
        let summary = self
            .morph_source_summary
            .as_ref()
            .ok_or_else(|| "The imported GLB has no source summary to save.".to_owned())?;
        let fallback_triangle_count = self
            .morph_preview
            .as_ref()
            .map(|preview| preview.indices.len() / 3)
            .and_then(|count| u32::try_from(count).ok())
            .ok_or_else(|| "The imported GLB has no preview triangle count.".to_owned())?;
        let lod_nodes = [
            self.morph_lod_nodes[0].trim(),
            self.morph_lod_nodes[1].trim(),
            self.morph_lod_nodes[2].trim(),
        ];
        let triangle_counts = lod_nodes.map(|node| {
            summary
                .node_triangle_counts
                .get(node)
                .copied()
                .unwrap_or_else(|| {
                    if node.is_empty() {
                        0
                    } else {
                        fallback_triangle_count
                    }
                })
        });
        let json = build_morph_draft_json(
            asset,
            geometry_file,
            self.morph_attachment(),
            lod_nodes,
            triangle_counts,
        )?;
        let suggested_name = format!(
            "{}.morph.draft.json",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, json))
    }

    pub(crate) fn set_morph_sidecar_export_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    true,
                    "Sidecar saved. The GLB and .morph.json can now travel together.".to_owned(),
                ));
                self.notice = "Morph sidecar saved".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_draft_export_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    false,
                    "Draft saved. Complete the LOD mapping before exporting or publishing."
                        .to_owned(),
                ));
                self.notice = "Morph draft saved".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_publish_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status =
                    Some((true, format!("Published {asset_id} ({byte_len} bytes).")));
                self.notice = "Morph pack published".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_project_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status = Some((
                    true,
                    format!("Added {asset_id} to this game ({byte_len} bytes)."),
                ));
                self.notice = "Character asset added to game".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_runtime_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status = Some((
                    true,
                    format!("Loaded {asset_id} for the live player ({byte_len} bytes)."),
                ));
                self.notice = "Morph pack loaded".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn morph_thumbnail_payload(&self) -> Result<(String, MorphGlbPreviewMesh), String> {
        let path = self
            .morph_preview_path
            .as_ref()
            .ok_or_else(|| "Import a GLB before generating a thumbnail.".to_owned())?;
        let preview = self
            .morph_preview
            .as_ref()
            .ok_or_else(|| "The imported GLB has no preview mesh.".to_owned())?
            .clone();
        let suggested_name = format!(
            "{}.png",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, preview))
    }

    pub(crate) fn set_morph_thumbnail_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    true,
                    "Thumbnail generated from the current shaded preview.".to_owned(),
                ));
                self.notice = "Morph thumbnail generated".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_preview(
        &mut self,
        path: String,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        let triangle_count = u32::try_from(preview.indices.len() / 3)
            .unwrap_or(u32::MAX)
            .max(1);
        let draft_asset = Some(default_rigid_accessory_asset(&path, triangle_count));
        self.morph_preview_path = Some(path);
        self.morph_preview = Some(preview);
        self.morph_lod_previews = [None, None, None];
        self.morph_preview_lod = None;
        self.morph_attachment_joint = "head".to_owned();
        self.morph_attachment_translation = [0.0; 3];
        self.morph_attachment_rotation = [0.0, 0.0, 0.0, 1.0];
        self.morph_attachment_scale = [1.0; 3];
        self.morph_lod_nodes = ["near", "mid", "far"].map(|level| {
            summary
                .lod_candidates
                .get(level)
                .filter(|candidates| candidates.len() == 1)
                .and_then(|candidates| candidates.first())
                .cloned()
                .unwrap_or_default()
        });
        self.morph_source_summary = Some(summary);
        self.morph_draft_asset = draft_asset;
        self.morph_draft_status = None;
        self.morph_import_error = None;
        self.notice = "GLB preview imported".to_owned();
    }

    pub(crate) fn set_morph_lod_preview(&mut self, level: usize, preview: MorphGlbPreviewMesh) {
        if level >= self.morph_lod_previews.len() {
            return;
        }
        self.morph_lod_previews[level] = Some(preview.clone());
        if self.morph_preview_lod == Some(level) {
            self.morph_preview = Some(preview);
        }
    }

    pub(crate) fn select_morph_preview_lod(&mut self, level: Option<usize>) {
        let Some(level) = level else {
            self.morph_preview_lod = None;
            return;
        };
        let Some(preview) = self.morph_lod_previews.get(level).and_then(Option::as_ref) else {
            return;
        };
        self.morph_preview_lod = Some(level);
        self.morph_preview = Some(preview.clone());
    }

    pub(crate) fn set_morph_sidecar_preview(
        &mut self,
        glb_path: String,
        manifest: MorphSourceManifest,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        let attachment = manifest.attachment.clone();
        let lod_nodes = ["near", "mid", "far"].map(|level| {
            manifest
                .geometry
                .lod_nodes
                .get(level)
                .cloned()
                .unwrap_or_default()
        });
        self.set_morph_preview(glb_path, preview, summary);
        self.morph_draft_asset = Some(manifest.asset);
        self.morph_attachment_joint = attachment.joint;
        self.morph_attachment_translation = attachment.translation;
        self.morph_attachment_rotation = attachment.rotation;
        self.morph_attachment_scale = attachment.scale;
        self.morph_lod_nodes = lod_nodes;
        self.morph_draft_status = Some((
            true,
            "Sidecar reimported and GLB contract validated.".to_owned(),
        ));
        self.notice = "Morph sidecar reimported".to_owned();
    }

    pub(crate) fn set_morph_draft_preview(
        &mut self,
        glb_path: String,
        draft: MorphDraftDocument,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        self.set_morph_preview(glb_path, preview, summary);
        self.morph_draft_asset = Some(draft.asset);
        self.morph_attachment_joint = draft.attachment.joint;
        self.morph_attachment_translation = draft.attachment.translation;
        self.morph_attachment_rotation = draft.attachment.rotation;
        self.morph_attachment_scale = draft.attachment.scale;
        self.morph_lod_nodes = draft.lod_nodes;
        self.validate_morph_draft();
        self.notice = "Morph draft reimported".to_owned();
    }

    pub(crate) fn set_morph_import_error(&mut self, message: String) {
        self.morph_import_error = Some(message.clone());
        self.notice = message;
    }

    pub(crate) fn validate_morph_draft(&mut self) {
        let Some(summary) = &self.morph_source_summary else {
            self.morph_draft_status = Some((
                false,
                "Import a GLB before validating its draft.".to_owned(),
            ));
            return;
        };
        let mut issues = Vec::new();
        let joint = self.morph_attachment_joint.trim();
        if joint.is_empty() || joint.contains('/') || joint.contains('\\') {
            issues.push("Attachment joint must be a non-empty joint name.".to_owned());
        }
        if !self
            .morph_attachment_translation
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 10.0)
        {
            issues.push("Attachment offset must stay within +/-10 units.".to_owned());
        }
        if !self
            .morph_attachment_scale
            .iter()
            .all(|value| value.is_finite() && (0.01..=100.0).contains(value))
        {
            issues.push("Attachment scale must stay within 0.01–100.".to_owned());
        }
        let rotation_length = self
            .morph_attachment_rotation
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if !rotation_length.is_finite() || !(0.99..=1.01).contains(&rotation_length) {
            issues.push("Attachment rotation must be a normalized quaternion.".to_owned());
        }
        if self
            .morph_draft_asset
            .as_ref()
            .is_some_and(|asset| asset.kind == MorphAssetKind::Headwear)
            && let Some(mesh) = self
                .morph_lod_previews
                .first()
                .and_then(Option::as_ref)
                .or(self.morph_preview.as_ref())
            && let Some((minimum, maximum)) = morph_mesh_bounds(mesh)
        {
            let runtime_width = ((maximum[0] - minimum[0]) * self.morph_attachment_scale[0].abs())
                .max((maximum[2] - minimum[2]) * self.morph_attachment_scale[2].abs());
            if !(0.25..=2.20).contains(&runtime_width) {
                issues.push(format!(
                    "Headwear runtime width is {runtime_width:.2} units; use Fit to person head or adjust attachment scale."
                ));
            }
        }
        for (index, level) in ["Near", "Mid", "Far"].into_iter().enumerate() {
            let node = self.morph_lod_nodes[index].trim();
            if node.is_empty() {
                issues.push(format!("{level} LOD needs a node mapping."));
            } else if !summary.node_names.iter().any(|candidate| candidate == node) {
                issues.push(format!(
                    "{level} LOD node {node:?} is not present in the GLB."
                ));
            }
        }
        for first in 0..self.morph_lod_nodes.len() {
            for second in (first + 1)..self.morph_lod_nodes.len() {
                let first_node = self.morph_lod_nodes[first].trim();
                if !first_node.is_empty() && first_node == self.morph_lod_nodes[second].trim() {
                    issues.push("Near, Mid, and Far must use distinct nodes.".to_owned());
                }
            }
        }
        if issues.is_empty() {
            self.morph_draft_status = Some((
                true,
                "Draft mapping is ready for sidecar export.".to_owned(),
            ));
            self.notice = "Draft mapping validated".to_owned();
        } else {
            self.morph_draft_status = Some((false, issues.join(" ")));
            self.notice = "Draft mapping needs attention".to_owned();
        }
    }

    pub(crate) fn fit_current_morph_to_person(&mut self) {
        let result = self
            .morph_lod_previews
            .first()
            .and_then(Option::as_ref)
            .or(self.morph_preview.as_ref())
            .ok_or_else(|| "Import a headwear mesh before fitting it.".to_owned())
            .and_then(|mesh| {
                fit_rigid_headwear_to_person(mesh, self.morph_attachment_joint.trim())
            });
        match result {
            Ok(attachment) => {
                self.morph_attachment_translation = attachment.translation;
                self.morph_attachment_rotation = attachment.rotation;
                self.morph_attachment_scale = attachment.scale;
                self.morph_draft_status = Some((
                    true,
                    format!(
                        "Fit to person head at {:.3}× scale. Validate and republish the pack.",
                        attachment.scale[0]
                    ),
                ));
                self.notice = "Headwear attachment fitted".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn execute_command(&mut self, command: StudioCommand) {
        if self.project_loading.is_some() {
            return;
        }
        match command {
            StudioCommand::NewProject => {
                self.new_project_dialog_open = true;
                self.new_project_title.clear();
                self.new_project_error = None;
                #[cfg(not(target_os = "macos"))]
                {
                    self.new_project_title_focus_requested = true;
                }
            }
            StudioCommand::OpenProject => {
                self.open_project_requested = true;
                self.notice = "Choose a project folder…".to_owned();
            }
            StudioCommand::Save => {
                if self.project_editable {
                    self.save_requested = true;
                    self.notice = "Saving project…".to_owned();
                } else {
                    self.notice = "This preview is read-only".to_owned();
                }
            }
            StudioCommand::RevealProject => {
                self.notice = "Reveal Project is not connected yet".to_owned();
            }
            StudioCommand::Copy => {
                self.state.egui_input_mut().events.push(egui::Event::Copy);
            }
            StudioCommand::Preferences => {
                self.notice = "Preferences are coming later".to_owned();
            }
            StudioCommand::MaximizeViewport => {
                self.notice = "Viewport maximize is coming later".to_owned();
            }
            StudioCommand::ResetLayout => {
                self.notice = "Layout reset".to_owned();
            }
            StudioCommand::ShowWorld => self.select_workspace(Workspace::World),
            StudioCommand::ShowAssets => self.select_workspace(Workspace::Assets),
            StudioCommand::ShowMaterials => self.select_workspace(Workspace::Materials),
            StudioCommand::ShowMorphs => self.select_workspace(Workspace::Morphs),
            StudioCommand::ShowTest => self.select_workspace(Workspace::Test),
        }
    }

    pub(crate) fn select_workspace(&mut self, workspace: Workspace) {
        self.workspace = workspace;
        self.notice = format!("{} workspace", workspace.label());
    }

    pub(crate) fn prepare(&mut self, window: &Window, project_name: &str) -> PreparedShell {
        let input = self.state.take_egui_input(window);
        let context = self.context.clone();
        let output = context.run_ui(input, |ui| self.show(ui, project_name));
        self.state
            .handle_platform_output(window, output.platform_output);
        let pixels_per_point = context.pixels_per_point();
        let paint_jobs = context.tessellate(output.shapes, pixels_per_point);
        let size = window.inner_size();
        self.pending_textures_delta.append(output.textures_delta);
        PreparedShell {
            paint_jobs,
            screen: ScreenDescriptor {
                size_in_pixels: [size.width, size.height],
                pixels_per_point,
            },
        }
    }

    pub(crate) fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::TextureView,
        prepared: PreparedShell,
    ) {
        let textures_delta = std::mem::take(&mut self.pending_textures_delta);
        for (id, image_delta) in &textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }
        let command_buffers = self.renderer.update_buffers(
            device,
            queue,
            encoder,
            &prepared.paint_jobs,
            &prepared.screen,
        );
        debug_assert!(command_buffers.is_empty());
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Cubacadabra Studio shell"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.renderer.render(
            &mut pass.forget_lifetime(),
            &prepared.paint_jobs,
            &prepared.screen,
        );
        for id in &textures_delta.free {
            self.renderer.free_texture(id);
        }
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, project_name: &str) {
        self.runtime_viewport = Rect::NOTHING;
        self.show_top_bar(ui, project_name);
        self.show_status_bar(ui);
        if self.codex_chat_open {
            self.show_codex_chat(ui);
        }
        match self.workspace {
            Workspace::World => self.show_world(ui),
            Workspace::Assets => self.show_assets(ui),
            Workspace::Materials => self.show_materials(ui),
            Workspace::Morphs => self.show_morphs(ui),
            Workspace::Test => self.show_test(ui),
        }
        #[cfg(not(target_os = "macos"))]
        self.show_new_project_dialog(ui.ctx());
        self.show_project_loading(ui.ctx());
        self.show_project_error(ui.ctx());
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    pub(crate) fn show_project_loading(&self, context: &egui::Context) {
        let Some(loading) = &self.project_loading else {
            return;
        };
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        egui::Modal::new(egui::Id::new("project_loading"))
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.border_strong))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(20)),
            )
            .show(context, |ui| {
                ui.set_width(280.0);
                ui.label(
                    RichText::new("Loading...")
                        .font(semibold_font(TYPE.primary))
                        .color(colors.text),
                );
                ui.add_space(10.0);
                ui.add(
                    egui::ProgressBar::new(loading.progress)
                        .desired_width(ui.available_width())
                        .show_percentage(),
                );
            });
    }

    pub(crate) fn show_project_error(&mut self, context: &egui::Context) {
        let Some(mut error) = self.project_error.clone() else {
            return;
        };
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        egui::Window::new("Preview build failed")
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.axis_x))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(14)),
            )
            .show(context, |ui| {
                ui.label(
                    RichText::new("The last working preview is still running.")
                        .size(TYPE.secondary)
                        .color(colors.text),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::multiline(&mut error)
                        .desired_rows(7)
                        .interactive(false)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                if ui.button("Dismiss").clicked() {
                    self.project_error = None;
                }
            });
    }

    pub(crate) fn show_codex_chat(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::right("codex_chat_panel")
            .resizable(true)
            .default_size(360.0)
            .size_range(280.0..=480.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Codex chat")
                                .font(semibold_font(TYPE.primary))
                                .color(colors.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .button(RichText::new("×").size(TYPE.primary))
                                .on_hover_text("Close Codex chat")
                                .clicked()
                            {
                                self.codex_chat_open = false;
                            }
                        });
                    });
                    let project_root = self.codex_project_root.display().to_string();
                    ui.label(
                        RichText::new(format!("Working in {project_root}"))
                            .size(TYPE.meta)
                            .color(colors.muted),
                    )
                    .on_hover_text(project_root);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Model").size(TYPE.meta).color(colors.muted));
                        let selected_model = self.codex_chat_model;
                        egui::ComboBox::from_id_salt("codex_chat_model")
                            .selected_text(codex_chat_model_label(selected_model, selected_model))
                            .width(156.0)
                            .show_ui(ui, |ui| {
                                for &(model, description) in &CODEX_CHAT_MODELS {
                                    ui.selectable_value(
                                        &mut self.codex_chat_model,
                                        model,
                                        codex_chat_model_label(model, selected_model),
                                    )
                                    .on_hover_text(description);
                                }
                            });
                        ui.label(RichText::new("Level").size(TYPE.meta).color(colors.muted));
                        egui::ComboBox::from_id_salt("codex_chat_reasoning_effort")
                            .selected_text(self.codex_chat_reasoning_effort)
                            .width(72.0)
                            .show_ui(ui, |ui| {
                                for &(effort, description) in &CODEX_CHAT_EFFORTS {
                                    ui.selectable_value(
                                        &mut self.codex_chat_reasoning_effort,
                                        effort,
                                        effort,
                                    )
                                    .on_hover_text(description);
                                }
                            });
                    });
                    ui.label(
                        RichText::new(codex_chat_model_description(self.codex_chat_model))
                            .size(TYPE.meta)
                            .color(colors.muted),
                    );
                    ui.separator();

                    // Keep the composer in the panel's visible region. With
                    // `auto_shrink(false)`, an unconstrained scroll area
                    // consumes all remaining height and lays the composer
                    // out below the panel clip rect.
                    let reserved_chat_controls_height = 150.0;
                    let messages_height =
                        (ui.available_height() - reserved_chat_controls_height).max(64.0);
                    egui::ScrollArea::vertical()
                        .id_salt("codex_chat_messages")
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .max_height(messages_height)
                        .show(ui, |ui| {
                            if self.codex_chat_messages.is_empty() {
                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new(
                                        "Describe a code change. Codex applies it, then Studio rebuilds the preview.",
                                    )
                                    .size(TYPE.secondary)
                                    .color(colors.secondary_text),
                                );
                            }
                            for message in &self.codex_chat_messages {
                                let (label, color) = match message.role {
                                    CodexChatRole::User => ("You", colors.accent),
                                    CodexChatRole::Assistant => ("Codex", colors.secondary_text),
                                };
                                ui.label(
                                    RichText::new(label)
                                        .font(semibold_font(TYPE.meta))
                                        .color(color),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&message.text)
                                            .size(TYPE.secondary)
                                            .color(colors.text),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(12.0);
                            }
                            if self.codex_activity.is_active() {
                                ui.label(
                                    RichText::new("Codex")
                                        .font(semibold_font(TYPE.meta))
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Spinner::new()
                                            .size(16.0)
                                            .color(colors.accent),
                                    );
                                    ui.label(
                                        RichText::new(self.codex_activity.label())
                                            .font(semibold_font(TYPE.secondary))
                                            .color(colors.text),
                                    );
                                    if self.codex_activity.is_cancellable()
                                        && ui.small_button("Cancel").clicked()
                                    {
                                        self.codex_cancel_requested = true;
                                        self.codex_activity = CodexActivity::Cancelling;
                                    }
                                });
                                ui.label(
                                    RichText::new(self.codex_activity.detail())
                                        .size(TYPE.meta)
                                        .color(colors.muted),
                                );
                                ui.add_space(12.0);
                                ui.ctx().request_repaint_after(Duration::from_millis(16));
                            }
                            if let Some(error) = &self.codex_chat_error {
                                ui.label(
                                    RichText::new(error).size(TYPE.meta).color(colors.axis_x),
                                );
                                ui.add_space(12.0);
                            }
                        });

                    if !self.codex_chat_ready {
                        ui.label(
                            RichText::new("Starting Codex…")
                                .size(TYPE.meta)
                            .color(colors.muted),
                        );
                    }
                    if self.codex_source_change_count > 0 || self.codex_change_files.is_some() {
                        let files = self.codex_change_files.clone().unwrap_or_default();
                        let changed_count = if self.codex_source_change_count > 0 {
                            self.codex_source_change_count
                        } else {
                            files.len()
                        };
                        let changed_label = if self.codex_source_change_count > 0 {
                            format!(
                                "Codex changed {} source file{}",
                                changed_count,
                                if changed_count == 1 { "" } else { "s" }
                            )
                        } else {
                            format!(
                                "Changed {} file{}",
                                changed_count,
                                if changed_count == 1 { "" } else { "s" }
                            )
                        };
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(changed_label)
                                .font(semibold_font(TYPE.meta))
                                .color(colors.text),
                            );
                            if !files.is_empty()
                                && ui
                                    .button(if self.codex_change_review_open {
                                        "Hide files"
                                    } else {
                                        "Show files"
                                    })
                                    .clicked()
                            {
                                self.codex_change_review_open = !self.codex_change_review_open;
                            }
                            let undo_enabled =
                                self.project_loading.is_none() && !self.codex_activity.is_active();
                            if ui
                                .add_enabled(undo_enabled, egui::Button::new("Undo this change"))
                                .on_disabled_hover_text("Undo is available after the rebuild finishes")
                                .clicked()
                            {
                                self.codex_undo_requested = true;
                            }
                        });
                        if self.codex_change_review_open {
                            Frame::NONE
                                .fill(colors.surface)
                                .inner_margin(Margin::symmetric(8, 5))
                                .show(ui, |ui| {
                                    for file in &files {
                                        ui.label(
                                            RichText::new(file)
                                                .size(TYPE.meta)
                                                .color(colors.secondary_text),
                                        );
                                    }
                                });
                        }
                    }
                    ui.separator();
                    let can_send = self.codex_chat_ready
                        && !self.codex_activity.is_active()
                        && self.chatgpt_account.is_some()
                        && !self.project_dirty
                        && self.project_loading.is_none();
                    ui.add_enabled_ui(can_send, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.codex_chat_draft)
                                .desired_rows(3)
                                .hint_text("Ask Codex to make a change…")
                                .desired_width(f32::INFINITY),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if self.project_dirty {
                                    "Save scene changes before asking Codex to edit code"
                                } else {
                                    "Codex edits code; Studio rebuilds when it finishes"
                                })
                                    .size(TYPE.meta)
                                    .color(colors.muted),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add_sized([68.0, 28.0], egui::Button::new("Send"))
                                    .clicked()
                                {
                                    let message = self.codex_chat_draft.trim().to_owned();
                                    if !message.is_empty() {
                                        self.codex_chat_messages.push(CodexChatMessage {
                                            role: CodexChatRole::User,
                                            text: message.clone(),
                                        });
                                        self.codex_chat_draft.clear();
                                        self.codex_activity = CodexActivity::Thinking;
                                        self.codex_cancel_requested = false;
                                        self.codex_cancel_sent = false;
                                        self.codex_chat_error = None;
                                        self.codex_chat_send_requested =
                                            Some(CodexChatSendRequest {
                                                message,
                                                model: self.codex_chat_model,
                                                reasoning_effort: self.codex_chat_reasoning_effort,
                                            });
                                    }
                                }
                            });
                        });
                    });
                });
            });
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn show_new_project_dialog(&mut self, context: &egui::Context) {
        if !self.new_project_dialog_open {
            return;
        }
        let mut close_requested = false;
        let dialog_width = (context.content_rect().width() - 40.0).clamp(280.0, 440.0);
        let response = egui::Modal::new(egui::Id::new("new_project_dialog"))
            .backdrop_color(Color32::from_black_alpha(128))
            .frame(
                Frame::NONE
                    .fill(if context.style_of(context.theme()).visuals.dark_mode {
                        DARK_PALETTE.panel_raised
                    } else {
                        LIGHT_PALETTE.surface_deep
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if context.style_of(context.theme()).visuals.dark_mode {
                            DARK_PALETTE.border_strong
                        } else {
                            LIGHT_PALETTE.border
                        },
                    ))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(20)),
            )
            .show(context, |ui| {
                let colors = palette(ui);
                ui.set_width(dialog_width);
                ui.label(
                    RichText::new("New Project")
                        .font(semibold_font(18.0))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Create a starter game with its manifest, Luau entry point, and local SDK.",
                    )
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
                );
                ui.add_space(18.0);
                ui.label(
                    RichText::new("Game title")
                        .font(medium_font(TYPE.secondary))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                let title_response = ui.add(
                    egui::TextEdit::singleline(&mut self.new_project_title)
                        .hint_text("The Wild West")
                        .desired_width(ui.available_width())
                        .min_size(egui::vec2(0.0, 28.0))
                        .vertical_align(Align::Center),
                );
                if std::mem::take(&mut self.new_project_title_focus_requested) {
                    title_response.request_focus();
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Location")
                        .font(medium_font(TYPE.secondary))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                Frame::NONE
                    .fill(colors.field)
                    .stroke(Stroke::new(1.0, colors.border))
                    .corner_radius(4.0)
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_sized([78.0, 26.0], egui::Button::new("Choose…"))
                                .clicked()
                            {
                                self.new_project_folder_requested = true;
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.new_project_parent.display().to_string())
                                        .size(TYPE.secondary)
                                        .color(colors.secondary_text),
                                )
                                .truncate(),
                            )
                            .on_hover_text(self.new_project_parent.display().to_string());
                        });
                    });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "A new folder is created from the title. Existing projects are never overwritten.",
                    )
                    .size(TYPE.meta)
                    .color(colors.muted),
                );
                if let Some(error) = &self.new_project_error {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(error)
                            .size(TYPE.secondary)
                            .color(colors.axis_x),
                    );
                }
                ui.add_space(20.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_sized([82.0, 28.0], egui::Button::new("Cancel"))
                        .clicked()
                    {
                        close_requested = true;
                    }
                    let enabled = !self.new_project_title.trim().is_empty();
                    let button_text = if ui.visuals().dark_mode {
                        colors.surface_deep
                    } else {
                        Color32::WHITE
                    };
                    let create = ui.add_enabled_ui(enabled, |ui| {
                        ui.add_sized(
                            [110.0, 28.0],
                            egui::Button::new(
                                RichText::new("Create project")
                                    .font(medium_font(TYPE.secondary))
                                    .color(button_text),
                            )
                            .fill(colors.accent)
                            .stroke(Stroke::NONE)
                            .corner_radius(4.0),
                        )
                    });
                    let enter_pressed = title_response.has_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if create.inner.clicked() || (enabled && enter_pressed) {
                        self.new_project_create_requested = true;
                        close_requested = true;
                    }
                });
            });
        let escape_pressed = response.is_top_modal
            && !response.any_popup_open
            && context
                .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if close_requested || escape_pressed {
            self.new_project_dialog_open = false;
        }
    }

    pub(crate) fn show_top_bar(&mut self, root: &mut egui::Ui, project_name: &str) {
        let colors = palette(root);
        egui::Panel::top("studio_top_bar")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(editor_frame(colors.surface).inner_margin(Margin::symmetric(8, 0)))
            .show(root, |ui| {
                egui::MenuBar::new().style(menu_bar_style).ui(ui, |ui| {
                    ui.add(
                        egui::Image::from_texture(&self.logo_texture)
                            .fit_to_exact_size(egui::vec2(20.0, 20.0))
                            .sense(Sense::hover()),
                    );

                    #[cfg(not(target_os = "macos"))]
                    {
                        ui.menu_button(RichText::new("File").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            if menu_entry(ui, Icon::Plus, "New Project…", "Ctrl+N", true).clicked()
                            {
                                self.execute_command(StudioCommand::NewProject);
                                ui.close();
                            }
                            if menu_entry(ui, Icon::Open, "Open Project…", "Ctrl+O", true).clicked()
                            {
                                self.execute_command(StudioCommand::OpenProject);
                                ui.close();
                            }
                            if menu_entry(ui, Icon::Save, "Save", "Ctrl+S", true).clicked() {
                                self.execute_command(StudioCommand::Save);
                                ui.close();
                            }
                            ui.separator();
                            if menu_entry(ui, Icon::Folder, "Reveal Project", "", true).clicked() {
                                self.execute_command(StudioCommand::RevealProject);
                                ui.close();
                            }
                        });
                        ui.menu_button(RichText::new("Edit").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            menu_entry(ui, Icon::Undo, "Undo", "Ctrl+Z", false);
                            menu_entry(ui, Icon::Redo, "Redo", "Ctrl+Shift+Z", false);
                            ui.separator();
                            if menu_entry(ui, Icon::Settings, "Preferences…", "Ctrl+,", true)
                                .clicked()
                            {
                                self.execute_command(StudioCommand::Preferences);
                                ui.close();
                            }
                        });
                        ui.menu_button(RichText::new("Window").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            if menu_entry(ui, Icon::Grid, "Maximize Viewport", "Space", true)
                                .clicked()
                            {
                                self.execute_command(StudioCommand::MaximizeViewport);
                                ui.close();
                            }
                            if menu_entry(ui, Icon::Sliders, "Reset Layout", "", true).clicked() {
                                self.execute_command(StudioCommand::ResetLayout);
                                ui.close();
                            }
                        });
                    }

                    ui.add_space(8.0);
                    for workspace in Workspace::ALL {
                        if workspace_tab(ui, workspace.label(), self.workspace == workspace)
                            .clicked()
                        {
                            self.execute_command(workspace.command());
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if self.auth_pending {
                            ui.label(
                                RichText::new("Signing in…")
                                    .size(TYPE.meta)
                                    .color(colors.muted),
                            );
                        } else if let Some(user) = &self.auth_user {
                            ui.label(
                                RichText::new(&user.name)
                                    .size(TYPE.meta)
                                    .color(colors.secondary_text),
                            );
                        } else if toolbar_button(ui, Icon::Character, "Sign in", false).clicked() {
                            self.auth_requested = true;
                        }
                        vertical_separator(ui, 14.0);
                        self.show_chatgpt_control(ui, colors);
                        let play_label = if self.playing { "Stop" } else { "Play" };
                        let play_width = toolbar_button_width(ui, play_label);
                        if ui.available_width() >= play_width + 48.0 {
                            let live = ui.allocate_response(
                                egui::vec2(40.0, CONTROL_HEIGHT),
                                Sense::hover(),
                            );
                            paint_status_label(ui, live.rect, colors.live, "Live");
                        }
                        let play_icon = if self.playing { Icon::Stop } else { Icon::Play };
                        if toolbar_button(ui, play_icon, play_label, self.playing).clicked() {
                            if self.playing {
                                self.playing = false;
                                self.notice = "Play session stopped".to_owned();
                            } else {
                                self.playing = true;
                                self.restart_requested = true;
                                self.notice = "Restarting preview…".to_owned();
                            }
                        }
                        if self.project_editable
                            && toolbar_button(ui, Icon::Play, "Rebuild & Play", false).clicked()
                        {
                            self.rebuild_and_play_requested = true;
                            self.notice = "Saving and rebuilding preview…".to_owned();
                        }
                        if self.project_editable
                            && toolbar_button(ui, Icon::Play, "Restart", false).clicked()
                        {
                            self.restart_requested = true;
                            self.notice = "Restarting preview…".to_owned();
                        }
                        let project_width = (ui.available_width() - 17.0).min(180.0);
                        if project_width >= 72.0 {
                            vertical_separator(ui, 14.0);
                            ui.add_sized(
                                [project_width, CONTROL_HEIGHT],
                                egui::Label::new(
                                    RichText::new(project_name)
                                        .size(TYPE.secondary)
                                        .color(colors.secondary_text),
                                )
                                .truncate(),
                            )
                            .on_hover_text(project_name);
                        }
                    });
                });
            });
    }

    pub(crate) fn show_chatgpt_control(&mut self, ui: &mut egui::Ui, colors: Palette) {
        if self.chatgpt_pending {
            toolbar_status(ui, Icon::Sparkles, "Connecting ChatGPT…", colors.muted)
                .on_hover_text("Finish signing in with ChatGPT in your browser");
            return;
        }

        if let Some(account) = &self.chatgpt_account {
            let label = chatgpt_account_label(account);
            let tooltip = account
                .email
                .as_deref()
                .map(|email| format!("ChatGPT connected as {email}"))
                .unwrap_or_else(|| "ChatGPT connected".to_owned());
            if toolbar_button(ui, Icon::Sparkles, &label, self.codex_chat_open)
                .on_hover_text(format!("{tooltip}. Open Codex chat"))
                .clicked()
            {
                self.codex_chat_open = true;
                self.codex_chat_open_requested = true;
                self.codex_chat_error = None;
            }
            return;
        }

        if self.chatgpt_available {
            if toolbar_button(ui, Icon::Sparkles, "Connect ChatGPT", false)
                .on_hover_text("Use your ChatGPT subscription with Codex in Studio")
                .clicked()
            {
                self.chatgpt_auth_requested = true;
                self.set_chatgpt_pending();
            }
            return;
        }

        toolbar_status(ui, Icon::Sparkles, "ChatGPT unavailable", colors.faint).on_hover_text(
            self.chatgpt_error
                .as_deref()
                .unwrap_or("Codex App Server is unavailable"),
        );
    }

    pub(crate) fn show_status_bar(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::bottom("studio_status_bar")
            .exact_size(STATUS_BAR_HEIGHT)
            .frame(editor_frame(colors.panel_raised).inner_margin(Margin::symmetric(8, 0)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(STATUS_BAR_HEIGHT);
                    ui.spacing_mut().interact_size.y = 16.0;
                    let status_color = if self.project_error.is_some() {
                        colors.axis_x
                    } else if self.project_dirty {
                        colors.accent
                    } else {
                        colors.muted
                    };
                    inline_icon(
                        ui,
                        if self.project_error.is_some() {
                            Icon::Stop
                        } else {
                            Icon::Check
                        },
                        status_color,
                    );
                    let notice_width = (ui.available_width() - 124.0).max(40.0);
                    ui.add_sized(
                        [notice_width, 16.0],
                        egui::Label::new(
                            RichText::new(&self.notice)
                                .size(TYPE.meta)
                                .color(status_color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.notice);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("Layout preview")
                                .size(TYPE.meta)
                                .color(colors.faint),
                        );
                        vertical_separator(ui, 12.0);
                        ui.label(RichText::new("Metal").size(TYPE.meta).color(colors.faint));
                    });
                });
            });
    }

    pub(crate) fn show_world(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::bottom("world_assets")
            .resizable(true)
            .default_size(112.0)
            .size_range(100.0..=280.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| self.asset_shelf(ui));

        egui::Panel::left("world_scene")
            .resizable(true)
            .default_size(208.0)
            .size_range(180.0..=340.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| self.scene_tree(ui));

        egui::Panel::right("world_inspector")
            .resizable(true)
            .default_size(256.0)
            .size_range(224.0..=340.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| self.inspector(ui));

        self.viewport_panel(root, "Perspective", "Viewport");
    }

    pub(crate) fn show_assets(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::left("asset_categories")
            .resizable(true)
            .default_size(220.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                panel_header(ui, Icon::Folder, "Library", |ui| {
                    icon_button(ui, Icon::Plus, "Add source", false);
                });
                content_frame().show(ui, |ui| {
                    for (filter, icon) in [
                        ("All", Icon::Grid),
                        ("Images", Icon::Image),
                        ("Materials", Icon::Material),
                        ("Characters", Icon::Character),
                    ] {
                        if navigation_row(ui, icon, filter, self.asset_filter == filter, false)
                            .clicked()
                        {
                            self.asset_filter = filter;
                            self.notice = format!("Showing {filter}");
                        }
                    }
                });
            });
        egui::Panel::right("asset_details")
            .resizable(true)
            .default_size(256.0)
            .size_range(224.0..=340.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                panel_header(ui, Icon::Sliders, "Asset details", |ui| {
                    icon_button(ui, Icon::More, "Asset options", false);
                });
                content_frame().show(ui, |ui| {
                    selected_object_header(ui, self.selected_asset, "Asset preview");
                    ui.add_space(4.0);
                    property_section(ui, "File", |ui| {
                        property_row(ui, "Type", asset_kind(self.selected_asset).1);
                        property_row(ui, "Status", "Preview");
                    });
                    ui.add_space(4.0);
                    drop_target(ui, "Drop a replacement file");
                });
            });
        egui::CentralPanel::default()
            .frame(editor_frame(colors.surface))
            .show(root, |ui| {
                panel_header(ui, Icon::Assets, "Assets", |ui| {
                    icon_button(ui, Icon::Grid, "Grid view", true);
                    icon_button(ui, Icon::More, "Asset options", false);
                });
                content_frame().show(ui, |ui| {
                    search_field(ui, &mut self.search_query, ui.available_width());
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        for asset in ["forest-grass", "forest-wood", "campfire", "tree", "castle"] {
                            let (icon, kind) = asset_kind(asset);
                            if asset_tile(ui, asset, icon, kind, self.selected_asset == asset)
                                .clicked()
                            {
                                self.selected_asset = asset;
                            }
                        }
                    });
                });
            });
    }

    pub(crate) fn show_materials(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::left("material_list")
            .resizable(true)
            .default_size(220.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                panel_header(ui, Icon::Material, "Materials", |ui| {
                    icon_button(ui, Icon::Plus, "New material", false);
                });
                content_frame().show(ui, |ui| {
                    for material in ["forest-grass", "forest-wood", "campfire"] {
                        if navigation_row(
                            ui,
                            Icon::Material,
                            material,
                            self.selected_asset == material,
                            false,
                        )
                        .clicked()
                        {
                            self.selected_asset = material;
                            self.notice = format!("Selected {material}");
                        }
                    }
                });
            });
        egui::Panel::right("material_inspector")
            .resizable(true)
            .default_size(256.0)
            .size_range(224.0..=340.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                panel_header(ui, Icon::Sliders, "Material", |ui| {
                    icon_button(ui, Icon::More, "Material options", false);
                });
                content_frame().show(ui, |ui| {
                    selected_object_header(ui, self.selected_asset, "Surface material");
                    ui.add_space(4.0);
                    property_section(ui, "Texture", |ui| {
                        property_row(ui, "Image", self.selected_asset);
                    });
                    property_section(ui, "Surface", |ui| {
                        ui.label(
                            RichText::new("Roughness")
                                .size(TYPE.secondary)
                                .color(colors.secondary_text),
                        );
                        if ui
                            .add(egui::Slider::new(&mut self.roughness, 0.0..=1.0))
                            .changed()
                        {
                            self.notice = "Material controls are preview only".to_owned();
                        }
                        property_row(ui, "Tile U", "8.0");
                        property_row(ui, "Tile V", "8.0");
                    });
                });
            });
        self.viewport_panel(root, "Daylight", "Material preview");
    }

    pub(crate) fn show_morphs(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        self.show_morph_library(root);

        if root.available_width() >= 650.0 {
            egui::Panel::right("morph_inspector")
            .resizable(true)
            .default_size(280.0)
            .size_range(236.0..=360.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                panel_header(ui, Icon::Sliders, "Morph inspector", |ui| {
                    icon_button(ui, Icon::More, "Morph options", false);
                });
                content_frame().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                        if ui.button("Import character asset…").clicked() {
                            self.morph_import_requested = true;
                            self.morph_import_error = None;
                        }
                        if ui.button("Open .morph.json").clicked() {
                            self.morph_sidecar_import_requested = true;
                            self.morph_import_error = None;
                        }
                        if ui.button("Load .morphpack").clicked() {
                            self.morph_pack_import_requested = true;
                            self.morph_import_error = None;
                        }
                    });
                    if let Some(path) = &self.morph_preview_path {
                        property_section(ui, "Imported source", |ui| {
                            let filename = std::path::Path::new(path)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or(path);
                            property_row(ui, "File", filename);
                            if let Some(preview) = &self.morph_preview {
                                property_row(ui, "Vertices", &preview.vertices.len().to_string());
                                property_row(ui, "Indices", &preview.indices.len().to_string());
                                property_row(
                                    ui,
                                    "Triangles",
                                    &(preview.indices.len() / 3).to_string(),
                                );
                                if let Some((minimum, maximum)) = morph_mesh_bounds(preview) {
                                    let size: [f32; 3] = std::array::from_fn(|axis| {
                                        maximum[axis] - minimum[axis]
                                    });
                                    property_row(
                                        ui,
                                        "Source size",
                                        &format!(
                                            "{:.2} × {:.2} × {:.2}",
                                            size[0], size[1], size[2]
                                        ),
                                    );
                                    property_row(
                                        ui,
                                        "Runtime size",
                                        &format!(
                                            "{:.2} × {:.2} × {:.2}",
                                            size[0] * self.morph_attachment_scale[0],
                                            size[1] * self.morph_attachment_scale[1],
                                            size[2] * self.morph_attachment_scale[2]
                                        ),
                                    );
                                }
                            }
                            if let Some(asset) = &self.morph_draft_asset {
                                property_row(ui, "Draft", &asset.display_name);
                                property_row(ui, "Asset ID", asset.id.as_str());
                            }
                        });
                        if let Some(summary) = self.morph_source_summary.clone() {
                            property_section(ui, "Source contract", |ui| {
                                property_row(ui, "Nodes", &summary.node_names.len().to_string());
                                property_row(ui, "Meshes", &summary.mesh_names.len().to_string());
                                property_row(
                                    ui,
                                    "Materials",
                                    &summary.material_names.len().to_string(),
                                );
                                property_row(
                                    ui,
                                    "Source triangles",
                                    &summary.triangle_count.to_string(),
                                );
                                for level in ["near", "mid", "far"] {
                                    let status = source_lod_status(&summary, level);
                                    property_row(ui, &format!("{} LOD", title_case(level)), &status);
                                }
                                if ["near", "mid", "far"]
                                    .into_iter()
                                    .any(|level| summary.lod_candidates[level].is_empty())
                                {
                                    ui.label(
                                        RichText::new(
                                            "Preview only: map distinct Near / Mid / Far nodes before publishing.",
                                        )
                                        .size(TYPE.meta)
                                        .color(colors.axis_x),
                                    );
                                }
                            });
                            property_section(ui, "Draft mapping", |ui| {
                                ui.label(
                                    RichText::new("Attachment joint")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.morph_attachment_joint)
                                        .hint_text("head")
                                        .desired_width(ui.available_width()),
                                );
                                ui.label(
                                    RichText::new("Attachment offset X / Y / Z")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for (axis, value) in
                                        self.morph_attachment_translation.iter_mut().enumerate()
                                    {
                                        ui.add(
                                            egui::DragValue::new(value)
                                                .speed(0.01)
                                                .range(-10.0..=10.0)
                                                .prefix(["X ", "Y ", "Z "][axis]),
                                        );
                                    }
                                });
                                ui.label(
                                    RichText::new("Attachment scale X / Y / Z")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for (axis, value) in
                                        self.morph_attachment_scale.iter_mut().enumerate()
                                    {
                                        ui.add(
                                            egui::DragValue::new(value)
                                                .speed(0.01)
                                                .range(0.01..=100.0)
                                                .prefix(["X ", "Y ", "Z "][axis]),
                                        );
                                    }
                                });
                                if ui.button("Fit to person head").clicked() {
                                    self.fit_current_morph_to_person();
                                }
                                for (index, level) in ["Near", "Mid", "Far"].into_iter().enumerate()
                                {
                                    ui.label(
                                        RichText::new(format!("{level} LOD node"))
                                            .size(TYPE.meta)
                                            .color(colors.secondary_text),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.morph_lod_nodes[index])
                                            .hint_text("GLB node name")
                                            .desired_width(ui.available_width()),
                                    );
                                }
                                if ui.button("Validate mapping").clicked() {
                                    self.validate_morph_draft();
                                }
                                if ui.button("Save draft").clicked() {
                                    self.morph_draft_export_requested = true;
                                }
                                if ui.button("Export .morph.json").clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_sidecar_export_requested = true;
                                    }
                                }
                                let add_to_game = ui
                                    .add_enabled(
                                        self.project_asset_available,
                                        egui::Button::new("Add to this game"),
                                    )
                                    .on_disabled_hover_text(
                                        "Open a game project before adding an asset.",
                                    );
                                if add_to_game.clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_project_add_requested = true;
                                    }
                                }
                                if ui.button("Export .morphpack").clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_publish_requested = true;
                                    }
                                }
                                if ui.button("Generate thumbnail PNG").clicked() {
                                    self.morph_thumbnail_requested = true;
                                }
                                if let Some((valid, status)) = &self.morph_draft_status {
                                    ui.label(
                                        RichText::new(status)
                                            .size(TYPE.meta)
                                            .color(if *valid { colors.live } else { colors.axis_x }),
                                    );
                                }
                            });
                        }
                    }
                    if let Some(error) = &self.morph_import_error {
                        ui.label(RichText::new(error).size(TYPE.meta).color(colors.axis_x));
                        ui.add_space(4.0);
                    }
                    if let Some(preset) = self.morph_catalog.presets.iter().find(|preset| preset.id == self.selected_morph) {
                        selected_object_header(ui, &preset.display_name, "Starter");
                        property_section(ui, "Appearance", |ui| {
                            for id in std::iter::once(&preset.base).chain(preset.parts.iter()) {
                                if let Some(asset) = self.morph_catalog.asset(id) {
                                    property_row(ui, morph_kind_label(asset.kind), &asset.display_name);
                                }
                            }
                            if let Some(cubacadabra_morphs::MorphParameterValue::Text(skin)) = preset.parameters.get("skin") {
                                property_row(ui, "Skin", skin);
                            }
                        });
                    } else if let Some(asset) = self.morph_catalog.asset(&self.selected_morph) {
                        selected_object_header(
                            ui,
                            &asset.display_name,
                            morph_kind_label(asset.kind),
                        );
                        ui.add_space(4.0);
                        property_section(ui, "Identity", |ui| {
                            let id = asset.id.to_string();
                            property_row(ui, "ID", &id);
                            property_row(ui, "Kind", morph_kind_label(asset.kind));
                            if let Some(rig) = &asset.rig_profile {
                                let rig = rig.to_string();
                                property_row(ui, "Rig", &rig);
                            }
                        });
                        property_section(ui, "Compatibility", |ui| {
                            let fits = asset
                                .fit_profiles
                                .iter()
                                .map(MorphAssetId::as_str)
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(ui, "Fits", &fits);
                            let bases = asset
                                .supported_bases
                                .iter()
                                .map(MorphAssetId::as_str)
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(
                                ui,
                                "Bases",
                                if bases.is_empty() { "Any" } else { &bases },
                            );
                        });
                        property_section(ui, "Runtime", |ui| {
                            let capabilities = asset
                                .required_capabilities
                                .iter()
                                .map(|capability| capability.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(ui, "Needs", &capabilities);
                            property_row(ui, "LOD", "Near / Mid / Far");
                        });
                    } else {
                        ui.label(
                            RichText::new("Select a morph to inspect its contract.")
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                    }
                });
            });
        }
        self.morph_preview_panel(root);
    }

    pub(crate) fn morph_preview_panel(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Camera, colors.muted);
                    ui.label(
                        RichText::new("Morph preview")
                            .font(semibold_font(TYPE.primary))
                            .color(colors.text),
                    );
                    vertical_separator(ui, 12.0);
                    ui.label(
                        RichText::new(if self.morph_preview.is_some() {
                            "Imported GLB"
                        } else {
                            self.morph_catalog
                                .presets
                                .iter()
                                .find(|preset| {
                                    crate::wardrobe::matches_preset(&self.active_loadout, preset)
                                })
                                .map_or("Custom appearance", |preset| preset.display_name.as_str())
                        })
                        .size(TYPE.secondary)
                        .color(colors.secondary_text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, Icon::More, "Preview options", false);
                        icon_button(ui, Icon::Camera, "Camera view", false);
                    });
                });
                let preview_rect = Rect::from_min_max(
                    egui::pos2(
                        available.min.x + 1.0,
                        available.min.y + EDITOR_HEADER_HEIGHT + 2.0,
                    ),
                    egui::pos2(available.max.x - 1.0, available.max.y - 1.0),
                );
                self.runtime_viewport = preview_rect;
                ui.allocate_rect(preview_rect, Sense::hover());
                if !self.morph_catalog_ready || self.morph_loading {
                    // The engine starts with its bundled appearance while the
                    // Studio catalog and initial loadout are arriving. Keep
                    // that implementation fallback out of the preview so it
                    // cannot flash before the requested appearance is ready.
                    ui.painter().rect_filled(preview_rect, 0.0, colors.surface);
                    ui.painter().text(
                        preview_rect.center(),
                        Align2::CENTER_CENTER,
                        "Loading appearance…",
                        FontId::proportional(TYPE.secondary),
                        colors.muted,
                    );
                }
                ui.painter().rect_stroke(
                    available,
                    0.0,
                    Stroke::new(1.0, colors.border),
                    StrokeKind::Inside,
                );
            });
    }

    pub(crate) fn show_test(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::bottom("test_tools")
            .resizable(true)
            .default_size(112.0)
            .size_range(80.0..=260.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Test, colors.muted);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for tool in ["Sessions", "State", "Network", "Logs", "Performance"] {
                        if compact_tab(ui, tool, self.test_tool == tool).clicked() {
                            self.test_tool = tool;
                            self.notice = format!("{tool} inspector preview");
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, Icon::More, "Test options", false);
                    });
                });
                content_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        inline_icon(ui, tool_icon(self.test_tool), colors.faint);
                        ui.label(
                            RichText::new(format!("{} tools will appear here.", self.test_tool))
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                let gap = 1.0;
                let half_width = (available.width() - gap) * 0.5;
                let half_height = (available.height() - gap) * 0.5;
                let rects = [
                    Rect::from_min_size(available.min, Vec2::new(half_width, half_height)),
                    Rect::from_min_size(
                        egui::pos2(available.min.x + half_width + gap, available.min.y),
                        Vec2::new(half_width, half_height),
                    ),
                    Rect::from_min_size(
                        egui::pos2(available.min.x, available.min.y + half_height + gap),
                        Vec2::new(half_width, half_height),
                    ),
                    Rect::from_min_size(
                        egui::pos2(
                            available.min.x + half_width + gap,
                            available.min.y + half_height + gap,
                        ),
                        Vec2::new(half_width, half_height),
                    ),
                ];
                for (index, rect) in rects.into_iter().enumerate() {
                    let header = Rect::from_min_size(
                        rect.min,
                        Vec2::new(rect.width(), EDITOR_HEADER_HEIGHT),
                    );
                    if index > 0 {
                        ui.painter().rect_filled(rect, 0.0, colors.surface);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Session preview",
                            FontId::proportional(TYPE.secondary),
                            colors.muted,
                        );
                    }
                    ui.painter().rect_filled(header, 0.0, colors.panel_header);
                    let icon_rect = Rect::from_center_size(
                        header.left_center() + egui::vec2(14.0, 0.0),
                        Vec2::splat(UI.icon),
                    );
                    paint_icon(ui.painter(), icon_rect, Icon::Camera, colors.faint);
                    ui.painter().text(
                        header.left_center() + egui::vec2(27.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("Player {}", index + 1),
                        medium_font(TYPE.secondary),
                        colors.text,
                    );
                    let status_center = header.right_center() - egui::vec2(13.0, 0.0);
                    ui.painter().circle_filled(
                        status_center,
                        3.0,
                        if index == 0 {
                            colors.accent
                        } else {
                            colors.faint
                        },
                    );
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(1.0, colors.border),
                        StrokeKind::Inside,
                    );
                    if index == 0 {
                        self.runtime_viewport = Rect::from_min_max(
                            egui::pos2(rect.min.x + 1.0, header.max.y),
                            egui::pos2(rect.max.x - 1.0, rect.max.y - 1.0),
                        );
                    }
                }
            });
    }

    pub(crate) fn viewport_panel(&mut self, root: &mut egui::Ui, mode: &str, title: &str) {
        let colors = palette(root);
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                let header = editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Camera, colors.muted);
                    ui.label(
                        RichText::new(title)
                            .font(semibold_font(TYPE.primary))
                            .color(colors.text),
                    );
                    vertical_separator(ui, 12.0);
                    ui.label(
                        RichText::new(mode)
                            .size(TYPE.secondary)
                            .color(colors.secondary_text),
                    );
                    paint_down_chevron(ui);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, Icon::More, "Viewport options", false);
                        icon_button(ui, Icon::Camera, "Camera view", false);
                        icon_button(ui, Icon::Sliders, "Viewport shading", false);
                        icon_button(ui, Icon::Grid, "Toggle grid", true);
                    });
                });
                self.runtime_viewport = Rect::from_min_max(
                    egui::pos2(available.min.x + 1.0, header.max.y),
                    egui::pos2(available.max.x - 1.0, available.max.y - 1.0),
                );
                ui.painter().rect_stroke(
                    available,
                    0.0,
                    Stroke::new(1.0, colors.border),
                    StrokeKind::Inside,
                );
                if let Some(selected) = self
                    .scene_outline
                    .root
                    .find(&self.selected_scene)
                    .filter(|node| node.kind == "Block")
                {
                    let badge = Rect::from_min_size(
                        self.runtime_viewport.min + egui::vec2(12.0, 12.0),
                        egui::vec2(220.0, 42.0),
                    );
                    ui.painter()
                        .rect_filled(badge, UI.radius, colors.panel_raised);
                    ui.painter().rect_stroke(
                        badge,
                        UI.radius,
                        Stroke::new(1.0, colors.asset_selection_stroke),
                        StrokeKind::Inside,
                    );
                    ui.painter().text(
                        badge.min + egui::vec2(10.0, 13.0),
                        Align2::LEFT_CENTER,
                        format!("Selected · {}", selected.label),
                        semibold_font(TYPE.meta),
                        colors.text,
                    );
                    ui.painter().text(
                        badge.min + egui::vec2(10.0, 29.0),
                        Align2::LEFT_CENTER,
                        "Edit position or size in Inspector",
                        FontId::proportional(TYPE.meta - 1.0),
                        colors.secondary_text,
                    );
                }
                ui.allocate_rect(self.runtime_viewport, Sense::hover());
            });
    }

    pub(crate) fn scene_tree(&mut self, ui: &mut egui::Ui) {
        if self.project_loading.is_some() {
            return;
        }
        panel_header(ui, Icon::World, "Scene", |ui| {
            icon_button(ui, Icon::Filter, "Filter scene", false);
        });
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::NONE
                    .inner_margin(Margin::symmetric(0, 4))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        show_scene_node(
                            ui,
                            &self.scene_outline.root,
                            0,
                            &mut self.expanded_scene,
                            &mut self.selected_scene,
                        );
                    });
            });
    }

    pub(crate) fn inspector(&mut self, ui: &mut egui::Ui) {
        panel_header(ui, Icon::Sliders, "Inspector", |ui| {
            icon_button(ui, Icon::Lock, "Lock inspector", false);
            icon_button(ui, Icon::More, "Inspector options", false);
        });
        let selected = self
            .scene_outline
            .root
            .find(&self.selected_scene)
            .cloned()
            .unwrap_or_else(|| self.scene_outline.root.clone());
        content_frame().show(ui, |ui| {
            selected_object_header(ui, &selected.label, selected.kind);
            ui.add_space(2.0);
            if selected.kind == "Block" {
                self.block_inspector(ui, &selected);
            } else if selected.kind == "Sign" {
                self.sign_inspector(ui, &selected);
            } else if selected.properties.is_empty() {
                ui.label(
                    RichText::new("No properties")
                        .size(TYPE.secondary)
                        .color(palette(ui).muted),
                );
            } else {
                property_section(ui, "Manifest", |ui| {
                    for (label, value) in &selected.properties {
                        property_row(ui, label, value);
                    }
                });
            }
        });
    }

    pub(crate) fn block_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        if self.scene_editor_target != selected.id {
            self.scene_editor_target = selected.id.clone();
            self.scene_editor_position = vector_property(selected, "Position").unwrap_or([0.0; 3]);
            self.scene_editor_size = vector_property(selected, "Size").unwrap_or([1.0; 3]);
        }

        property_section(ui, "Transform", |ui| {
            let position_changed =
                vector_editor(ui, "Position", &mut self.scene_editor_position, 0.1);
            let size_changed = vector_editor(ui, "Size", &mut self.scene_editor_size, 0.1);
            if position_changed || size_changed {
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = Some(SceneEditRequest::UpdateBlock {
                    target: selected.id.clone(),
                    position: self.scene_editor_position,
                    size: self.scene_editor_size,
                });
                self.notice = "Platform changed — save to keep it".to_owned();
            }
        });
        property_section(ui, "Actions", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.project_editable, egui::Button::new("Duplicate"))
                    .on_disabled_hover_text("Open a raw source project to edit the scene")
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DuplicateBlock {
                        target: selected.id.clone(),
                    });
                    self.notice = "Duplicating platform…".to_owned();
                }
                if ui
                    .add_enabled(self.project_editable, egui::Button::new("Delete"))
                    .on_disabled_hover_text("Open a raw source project to edit the scene")
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DeleteBlock {
                        target: selected.id.clone(),
                    });
                    self.notice = "Deleting platform…".to_owned();
                }
            });
        });
        property_section(ui, "Manifest", |ui| {
            for (label, value) in &selected.properties {
                if label != "Position" && label != "Size" {
                    property_row(ui, label, value);
                }
            }
        });
        if !self.project_editable {
            ui.label(
                RichText::new("Open a raw source project to edit scene objects.")
                    .size(TYPE.meta)
                    .color(colors.muted),
            );
        }
    }

    pub(crate) fn sign_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        if self.scene_editor_target != selected.id {
            self.scene_editor_target = selected.id.clone();
            self.scene_editor_text = selected
                .properties
                .iter()
                .find(|(label, _)| label == "Text")
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
        }

        property_section(ui, "Content", |ui| {
            ui.label(
                RichText::new("Text")
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
            );
            if ui
                .add(
                    egui::TextEdit::multiline(&mut self.scene_editor_text)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text("Sign text"),
                )
                .changed()
            {
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = Some(SceneEditRequest::UpdateSignText {
                    target: selected.id.clone(),
                    text: self.scene_editor_text.clone(),
                });
                self.notice = "Sign text changed — save to keep it".to_owned();
            }
        });
        ui.label(
            RichText::new("Save, then Rebuild & Play to see the updated sign in the game.")
                .size(TYPE.meta)
                .color(colors.muted),
        );
    }

    pub(crate) fn asset_shelf(&mut self, ui: &mut egui::Ui) {
        let colors = palette(ui);
        editor_header(ui, |ui| {
            inline_icon(ui, Icon::Assets, colors.muted);
            ui.label(
                RichText::new("Assets")
                    .font(semibold_font(TYPE.primary))
                    .color(colors.text),
            );
            vertical_separator(ui, 12.0);
            ui.spacing_mut().item_spacing.x = 0.0;
            for filter in ["All", "Images", "Materials", "Characters"] {
                if compact_tab(ui, filter, self.asset_filter == filter).clicked() {
                    self.asset_filter = filter;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                icon_button(ui, Icon::Grid, "Grid view", true);
                search_field(ui, &mut self.search_query, 190.0);
            });
        });
        Frame::NONE
            .inner_margin(Margin::symmetric(8, 8))
            .show(ui, |ui| {
                let query = self.search_query.trim().to_lowercase();
                let assets = self
                    .scene_outline
                    .assets
                    .iter()
                    .filter(|asset| match self.asset_filter {
                        "Images" => asset.kind == "IMAGE",
                        "Materials" => asset.kind == "MATERIAL",
                        "Characters" => asset.kind == "CHARACTER",
                        _ => true,
                    })
                    .filter(|asset| query.is_empty() || asset.name.to_lowercase().contains(&query))
                    .cloned()
                    .collect::<Vec<_>>();
                ui.horizontal_wrapped(|ui| {
                    if assets.is_empty() {
                        ui.label(
                            RichText::new(if query.is_empty() {
                                "No assets in this game."
                            } else {
                                "No matching assets."
                            })
                            .size(TYPE.secondary)
                            .color(colors.muted),
                        );
                    }
                    for asset in assets {
                        if asset_tile(
                            ui,
                            &asset.name,
                            asset.icon,
                            asset.kind,
                            self.selected_world_asset == asset.name,
                        )
                        .clicked()
                        {
                            self.selected_world_asset = asset.name;
                        }
                    }
                });
            });
    }
}
