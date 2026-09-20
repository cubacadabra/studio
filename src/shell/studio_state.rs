use super::*;
impl StudioShell {
    pub(crate) fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let consumed = self.state.on_window_event(window, event).consumed;
        if let WindowEvent::DroppedFile(path) = event
            && self.project_editable
        {
            self.dropped_files.push(path.clone());
            self.notice = "Importing dropped image…".to_owned();
        }
        consumed
    }

    pub(crate) fn runtime_viewport(&self) -> Rect {
        self.runtime_viewport
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playing
    }

    pub(crate) fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if !playing && self.review_camera == ReviewCameraPreset::Gameplay {
            self.review_camera = ReviewCameraPreset::Showcase;
            self.review_camera_reset = true;
        }
    }

    pub(crate) fn is_morphs_workspace(&self) -> bool {
        self.workspace == Workspace::Morphs
    }

    pub(crate) fn editor_shortcuts_active(&self) -> bool {
        self.workspace == Workspace::World
            && self.project_editable
            && !self.playing
            && self.project_loading.is_none()
    }

    pub(crate) fn review_camera(&self) -> ReviewCameraPreset {
        self.review_camera
    }

    pub(crate) fn active_review_camera(&self) -> ReviewCameraPreset {
        if self.playing {
            ReviewCameraPreset::Gameplay
        } else {
            self.review_camera
        }
    }

    pub(crate) fn set_review_camera(&mut self, preset: ReviewCameraPreset) {
        self.review_camera = preset;
        self.review_camera_reset = true;
        self.notice = format!("{} review camera", preset.label());
    }

    pub(crate) fn restore_review_camera(&mut self, preset: ReviewCameraPreset) {
        self.review_camera = preset;
    }

    pub(crate) fn take_review_camera_reset(&mut self) -> bool {
        std::mem::take(&mut self.review_camera_reset)
    }

    pub(crate) fn set_start_screen(&mut self, start_screen: bool) {
        self.start_screen = start_screen;
        self.start_screen_logged = false;
        if start_screen {
            self.playing = false;
            self.notice = "No project open".to_owned();
        }
    }

    pub(crate) fn set_recent_projects(&mut self, projects: Vec<PathBuf>) {
        log::info!(
            "start screen: received {} recent project(s): {}",
            projects.len(),
            crate::recent_project_log_list(&projects)
        );
        self.recent_projects = projects;
        self.start_screen_logged = false;
    }

    pub(crate) fn take_recent_project_request(&mut self) -> Option<PathBuf> {
        self.recent_project_requested.take()
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

    pub(crate) fn preview_is_stale(&self) -> bool {
        self.preview_stale
    }

    pub(crate) fn set_source_manifest(&mut self, source: &str, dirty: bool) -> bool {
        let Ok(mut outline) = self.parse_scene_outline(source) else {
            return false;
        };
        outline.set_runtime_ui_nodes(&self.runtime_ui_nodes);
        if !self.runtime_ui_nodes.is_empty() {
            self.expanded_scene.insert("game".to_owned());
            self.expanded_scene.insert("game/interface".to_owned());
        }
        let selected = self
            .scene_outline
            .root
            .find(&self.selected_scene)
            .is_some_and(|_| outline.root.find(&self.selected_scene).is_some())
            .then(|| self.selected_scene.clone())
            .unwrap_or_else(|| outline.initial_selection.clone());
        self.scene_outline = outline;
        self.scene_tree_rows_dirty = true;
        self.scene_search_query.clear();
        self.scene_search_matches.clear();
        self.selected_scene = selected;
        self.project_dirty = dirty;
        if dirty {
            self.preview_stale = true;
        }
        self.scene_editor_target.clear();
        self.scene_editor_text.clear();
        if self.source_files.contains_key(Path::new("manifest.json")) {
            self.source_files
                .insert(PathBuf::from("manifest.json"), source.to_owned());
        }
        true
    }

    fn parse_scene_outline(&self, manifest_source: &str) -> Result<SceneOutline, String> {
        match self.authoring_scene_source.as_deref() {
            Some(source) => {
                let scene = parse_authoring_scene(source)?;
                SceneOutline::parse_with_authoring_scene(manifest_source, &scene)
            }
            None => SceneOutline::parse(manifest_source).map_err(|error| error.to_string()),
        }
    }

    pub(crate) fn set_source_scene(&mut self, source: Option<&str>, dirty: bool) -> bool {
        let previous_selection = self.selected_scene.clone();
        let previous_expanded = self.expanded_scene.clone();
        self.authoring_scene_source = source.map(str::to_owned);
        let manifest_source = self
            .source_files
            .get(std::path::Path::new("manifest.json"))
            .cloned();
        let Some(manifest_source) = manifest_source else {
            return false;
        };
        let Ok(mut outline) = self.parse_scene_outline(&manifest_source) else {
            return false;
        };
        outline.set_runtime_ui_nodes(&self.runtime_ui_nodes);
        self.selected_scene = outline
            .root
            .find(&previous_selection)
            .map(|node| node.id.clone())
            .unwrap_or_else(|| outline.initial_selection.clone());
        self.expanded_scene = previous_expanded
            .into_iter()
            .filter(|id| outline.root.find(id).is_some())
            .collect();
        self.expanded_scene
            .extend(outline.initial_expanded.iter().cloned());
        self.scene_outline = outline;
        self.scene_search_query.clear();
        self.scene_search_matches.clear();
        self.scene_tree_rows_dirty = true;
        self.project_dirty = dirty;
        if dirty {
            self.preview_stale = true;
        }
        if let Some(source) = source {
            self.source_files
                .insert(PathBuf::from("scene.json"), source.to_owned());
        } else {
            self.source_files.remove(std::path::Path::new("scene.json"));
        }
        self.scene_editor_target.clear();
        true
    }

    pub(crate) fn set_runtime_ui_nodes(&mut self, nodes: &[cubacadabra_client::StudioUiNode]) {
        self.runtime_ui_nodes = nodes.to_vec();
        self.scene_outline.set_runtime_ui_nodes(nodes);
        self.scene_tree_rows_dirty = true;
        if !nodes.is_empty() {
            self.expanded_scene.insert("game".to_owned());
            self.expanded_scene.insert("game/interface".to_owned());
        } else {
            self.expanded_scene.remove("game/interface");
        }
        if self.scene_outline.root.find(&self.selected_scene).is_none() {
            self.selected_scene = self.scene_outline.initial_selection.clone();
        }
    }

    pub(crate) fn set_project_error(&mut self, message: String) {
        self.project_error = Some(message.clone());
        self.notice = message;
    }

    pub(crate) fn finish_project_loading(&mut self, play_after_rebuild: bool) {
        self.project_loading = None;
        self.project_error = None;
        self.preview_stale = false;
        self.set_playing(play_after_rebuild);
    }

    pub(crate) fn begin_game_rebuild(&mut self) {
        if self.project_loading.is_some() {
            return;
        }
        self.project_loading = Some(ProjectLoadingState {
            progress: 0.0,
            rebuilding: true,
            previous_preview_stale: self.preview_stale,
            previous_outline: self.scene_outline.clone(),
            previous_expanded: self.expanded_scene.clone(),
            previous_selection: self.selected_scene.clone(),
            previous_world_asset: self.selected_world_asset.clone(),
            previous_workspace: self.workspace,
        });
        self.project_error = None;
        self.preview_stale = true;
        self.notice = "Rebuilding preview…".to_owned();
    }

    pub(crate) fn take_scene_edit_request(&mut self) -> Option<SceneEditRequest> {
        self.scene_edit_requested.take()
    }

    pub(crate) fn scene_object_geometries(&self) -> Vec<SceneObjectGeometry> {
        self.scene_outline.placeable_object_geometries()
    }

    pub(crate) fn take_scene_focus_request(&mut self) -> Option<SceneObjectGeometry> {
        if !std::mem::take(&mut self.scene_focus_requested) {
            return None;
        }
        self.scene_object_geometries()
            .into_iter()
            .find(|geometry| geometry.id == self.selected_scene)
    }

    pub(crate) fn request_scene_focus(&mut self) -> bool {
        if self
            .scene_object_geometries()
            .iter()
            .any(|geometry| geometry.id == self.selected_scene)
        {
            self.scene_focus_requested = true;
            true
        } else {
            false
        }
    }

    pub(crate) fn selected_scene_object_geometry(&self) -> Option<SceneObjectGeometry> {
        self.scene_object_geometries()
            .into_iter()
            .find(|geometry| geometry.id == self.selected_scene && geometry.editable)
    }

    pub(crate) fn set_scene_object_projections(&mut self, projections: Vec<SceneObjectProjection>) {
        self.scene_object_projections = projections;
    }

    pub(crate) fn take_scene_viewport_edit_request(&mut self) -> Option<SceneViewportEditRequest> {
        self.scene_viewport_edit_requested.take()
    }

    pub(crate) fn scene_editor_hit_test(&self, point: Pos2) -> bool {
        self.workspace == Workspace::World
            && self.project_editable
            && !self.playing
            && self
                .scene_object_projections
                .iter()
                .any(|projection| projection.editable && projection.contains(point))
    }

    pub(crate) fn authoring_local_position_for_world(
        &self,
        id: &str,
        world_position: [f32; 3],
    ) -> Result<Option<[f32; 3]>, String> {
        let Some(source) = self.authoring_scene_source.as_deref() else {
            return Ok(None);
        };
        let scene = parse_authoring_scene(source)?;
        if scene.node(id).is_none() {
            return Ok(None);
        }
        scene.local_position_for_world(id, world_position).map(Some)
    }

    pub(crate) fn authoring_local_transform_for_world(
        &self,
        id: &str,
        world_position: [f32; 3],
        world_scale: [f32; 3],
    ) -> Result<([f32; 3], [f32; 3]), String> {
        let Some(source) = self.authoring_scene_source.as_deref() else {
            return Ok((world_position, world_scale));
        };
        let scene = parse_authoring_scene(source)?;
        if scene.node(id).is_none() {
            return Ok((world_position, world_scale));
        }
        scene.local_transform_for_world(id, world_position, world_scale)
    }

    pub(crate) fn select_scene_node(&mut self, id: &str) -> bool {
        if self.scene_outline.root.find(id).is_none() {
            return false;
        }
        self.selected_scene = id.to_owned();
        let previous_expanded = self.expanded_scene.clone();
        self.expanded_scene.insert("game".to_owned());
        let mut path = Vec::new();
        if self.scene_outline.root.collect_ancestor_ids(id, &mut path) {
            self.expanded_scene.extend(path);
        }
        if self.expanded_scene != previous_expanded {
            self.scene_tree_rows_dirty = true;
        }
        true
    }

    pub(crate) fn deselect_scene_object(&mut self) -> bool {
        if !self
            .scene_object_geometries()
            .iter()
            .any(|geometry| geometry.id == self.selected_scene)
        {
            return false;
        }
        let overview = self.scene_outline.initial_selection.clone();
        self.select_scene_node(&overview);
        self.scene_focus_requested = false;
        self.notice = "Selection cleared".to_owned();
        true
    }

    pub(crate) fn take_save_request(&mut self) -> bool {
        std::mem::take(&mut self.save_requested)
    }

    pub(crate) fn take_undo_request(&mut self) -> bool {
        std::mem::take(&mut self.undo_requested)
    }

    pub(crate) fn take_redo_request(&mut self) -> bool {
        std::mem::take(&mut self.redo_requested)
    }

    pub(crate) fn request_close(&mut self) {
        if self.project_dirty {
            self.pending_project_action = Some(PendingProjectAction::Close);
            self.notice = "Unsaved changes — save or discard them before closing".to_owned();
        } else {
            self.exit_requested = true;
        }
    }

    pub(crate) fn take_exit_requested(&mut self) -> bool {
        std::mem::take(&mut self.exit_requested)
    }

    pub(crate) fn take_pending_project_action_after_save(
        &mut self,
    ) -> Option<PendingProjectAction> {
        if self.project_dirty {
            return None;
        }
        self.pending_project_action.take()
    }

    pub(crate) fn discard_pending_project_action(&mut self) -> Option<PendingProjectAction> {
        self.project_dirty = false;
        self.preview_stale = false;
        self.pending_project_action.take()
    }

    pub(crate) fn take_imported_assets_for_discard(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.imported_asset_paths)
    }

    pub(crate) fn record_imported_asset(&mut self, path: PathBuf) {
        self.imported_asset_paths.push(path);
    }

    pub(crate) fn apply_pending_project_action(&mut self, action: PendingProjectAction) {
        match action {
            PendingProjectAction::NewProject => self.execute_command(StudioCommand::NewProject),
            PendingProjectAction::OpenProject => self.execute_command(StudioCommand::OpenProject),
            PendingProjectAction::Close => self.exit_requested = true,
        }
    }

    pub(crate) fn take_rebuild_and_play_request(&mut self) -> bool {
        std::mem::take(&mut self.rebuild_and_play_requested)
    }

    pub(crate) fn request_rebuild_and_play(&mut self) {
        self.workspace = Workspace::World;
        self.rebuild_and_play_requested = true;
        self.notice = "Saving and rebuilding preview…".to_owned();
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

    pub(crate) fn submit_codex_chat(&mut self) {
        let message = self.codex_chat_draft.trim().to_owned();
        if message.is_empty() {
            return;
        }
        self.codex_chat_messages.push(CodexChatMessage {
            role: CodexChatRole::User,
            text: message.clone(),
        });
        self.codex_chat_draft.clear();
        self.codex_live_excerpt.clear();
        self.codex_live_pending_excerpt.clear();
        self.codex_live_excerpt_queue.clear();
        self.codex_live_last_published_at = None;
        self.codex_live_last_received_at = None;
        self.codex_live_needs_separator = false;
        self.codex_live_in_code_block = false;
        self.codex_activity = CodexActivity::Thinking;
        self.codex_cancel_requested = false;
        self.codex_cancel_sent = false;
        self.codex_chat_error = None;
        self.codex_chat_send_requested = Some(CodexChatSendRequest {
            message,
            model: self.codex_chat_model,
            reasoning_effort: self.codex_chat_reasoning_effort,
        });
    }

    pub(crate) fn set_codex_chat_ready(&mut self) {
        self.codex_chat_ready = true;
        self.codex_chat_error = None;
    }

    pub(crate) fn set_codex_chat_delta(&mut self, delta: String) {
        self.append_codex_live_excerpt(&delta);
        self.set_codex_activity(self.codex_activity.after_agent_progress());
    }

    pub(crate) fn set_codex_work_status(&mut self, status: CodexWorkStatus) {
        if !self.codex_activity.is_cancellable() {
            return;
        }
        let activity = match status {
            CodexWorkStatus::Thinking => CodexActivity::Thinking,
            CodexWorkStatus::Editing => CodexActivity::Editing,
            CodexWorkStatus::Checking => CodexActivity::Checking,
            CodexWorkStatus::Working => CodexActivity::Working,
        };
        self.set_codex_activity(activity);
    }

    pub(crate) fn set_codex_chat_message(&mut self, text: String) {
        // An agent-message item can complete while the turn continues with
        // more tool work. Only turn/completed advances Studio to rebuilding.
        self.append_codex_live_excerpt(&text);
        self.set_codex_activity(self.codex_activity.after_agent_progress());
    }

    pub(crate) fn set_codex_chat_completed(&mut self) {
        self.set_codex_activity(CodexActivity::Rebuilding);
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
        self.set_codex_activity(CodexActivity::Cancelling);
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
        self.codex_live_excerpt.clear();
        self.codex_live_pending_excerpt.clear();
        self.codex_live_excerpt_queue.clear();
        self.codex_live_last_published_at = None;
        self.codex_live_last_received_at = None;
        self.codex_live_needs_separator = false;
        self.codex_live_in_code_block = false;
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

    fn append_codex_live_excerpt(&mut self, text: &str) {
        let mut safe_lines = Vec::new();
        for raw_line in text.lines() {
            let has_leading_whitespace = raw_line.chars().next().is_some_and(char::is_whitespace);
            let has_trailing_whitespace = raw_line
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
            let line = raw_line.trim();
            if line.is_empty() {
                if !self.codex_live_in_code_block && raw_line.chars().any(char::is_whitespace) {
                    self.codex_live_needs_separator = true;
                }
                continue;
            }
            let fence_count = line.matches("```").count();
            if fence_count > 0 {
                if fence_count % 2 == 1 {
                    self.codex_live_in_code_block = !self.codex_live_in_code_block;
                }
                continue;
            }
            if self.codex_live_in_code_block {
                continue;
            }
            let line = line
                .replace("checking out the project", "reviewing the project")
                .replace("Checking out the project", "Reviewing the project")
                .replace("checking out", "reviewing")
                .replace("Checking out", "Reviewing");
            if !is_human_readable_codex_line(&line) {
                continue;
            }
            let needs_separator =
                has_leading_whitespace || self.codex_live_needs_separator || !safe_lines.is_empty();
            safe_lines.push((line, needs_separator));
            self.codex_live_needs_separator = has_trailing_whitespace;
        }
        let mut appended = false;
        for (line, needs_separator) in safe_lines {
            if needs_separator && !self.codex_live_pending_excerpt.is_empty() {
                self.codex_live_pending_excerpt.push(' ');
            }
            self.codex_live_pending_excerpt.push_str(&line);
            appended = true;
        }
        if appended {
            let now = Instant::now();
            self.codex_live_last_received_at = Some(now);
            self.queue_codex_live_excerpts(false);
            self.publish_codex_live_excerpt(now);
        }
    }

    fn queue_codex_live_excerpts(&mut self, flush_tail: bool) {
        while let Some(end) = codex_live_chunk_end(&self.codex_live_pending_excerpt, flush_tail) {
            let excerpt = self
                .codex_live_pending_excerpt
                .get(..end)
                .unwrap_or(&self.codex_live_pending_excerpt)
                .trim()
                .to_owned();
            self.codex_live_pending_excerpt = self
                .codex_live_pending_excerpt
                .get(end..)
                .unwrap_or_default()
                .trim_start()
                .to_owned();
            if excerpt.is_empty()
                || !is_human_readable_codex_line(&excerpt)
                || self.codex_live_excerpt == excerpt
                || self.codex_live_excerpt_queue.back() == Some(&excerpt)
            {
                continue;
            }
            self.codex_live_excerpt_queue.push_back(excerpt);
        }
    }

    fn publish_codex_live_excerpt(&mut self, now: Instant) {
        let display_time = codex_live_display_time(&self.codex_live_excerpt);
        if self
            .codex_live_last_published_at
            .is_some_and(|last| now.duration_since(last) < display_time)
        {
            return;
        }
        let Some(excerpt) = self.codex_live_excerpt_queue.pop_front() else {
            return;
        };
        self.codex_live_excerpt = excerpt;
        self.codex_live_last_published_at = Some(now);
    }

    pub(crate) fn advance_codex_live_activity(&mut self) {
        if self.codex_activity.is_active() {
            const INCOMPLETE_PHRASE_HOLD: Duration = Duration::from_millis(700);
            let now = Instant::now();
            let should_flush_tail = self.codex_live_excerpt_queue.is_empty()
                && self
                    .codex_live_last_received_at
                    .is_some_and(|last| now.duration_since(last) >= INCOMPLETE_PHRASE_HOLD);
            if should_flush_tail {
                self.queue_codex_live_excerpts(true);
            }
            self.publish_codex_live_excerpt(now);
        }
    }

    fn set_codex_activity(&mut self, activity: CodexActivity) {
        self.codex_activity = activity;
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

    pub(crate) fn take_source_import_request(&mut self) -> Option<PathBuf> {
        self.source_import_requested.take()
    }

    pub(crate) fn take_dropped_files(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.dropped_files)
    }

    pub(crate) fn set_source_directories(&mut self, directories: BTreeSet<PathBuf>) {
        self.source_directories = directories;
        self.source_collapsed_directories
            .retain(|directory| self.source_directories.contains(directory));
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
            rebuilding: false,
            previous_preview_stale: self.preview_stale,
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
        if !loading.rebuilding {
            self.preview_stale = loading.previous_preview_stale;
        }
    }

    pub(crate) fn is_project_loading(&self) -> bool {
        self.project_loading.is_some()
    }

    pub(crate) fn is_rebuilding_project(&self) -> bool {
        self.project_loading
            .as_ref()
            .is_some_and(|loading| loading.rebuilding)
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
}

pub(crate) fn codex_live_chunk_end(text: &str, flush_tail: bool) -> Option<usize> {
    const MIN_SENTENCE_CHARS: usize = 12;
    const MIN_TAIL_CHARS: usize = 24;
    const MAX_CHARS: usize = 120;
    let mut last_boundary = None;
    let mut char_count = 0;
    for (byte_index, character) in text.char_indices() {
        if character.is_whitespace() {
            last_boundary = Some(byte_index);
        }
        char_count += 1;
        if char_count >= MIN_SENTENCE_CHARS && matches!(character, '.' | '!' | '?') {
            return Some(byte_index + character.len_utf8());
        }
        if char_count == MAX_CHARS {
            return Some(last_boundary.unwrap_or_else(|| {
                text[byte_index..]
                    .char_indices()
                    .find_map(|(offset, character)| {
                        character.is_whitespace().then_some(byte_index + offset)
                    })
                    .unwrap_or(text.len())
            }));
        }
    }
    (flush_tail && char_count >= MIN_TAIL_CHARS).then_some(text.len())
}

pub(crate) fn codex_live_display_time(excerpt: &str) -> Duration {
    const MIN_DISPLAY_MILLIS: u64 = 1_500;
    const MAX_DISPLAY_MILLIS: u64 = 3_500;
    const MILLIS_PER_CHARACTER: u64 = 35;
    let millis = (excerpt.chars().count() as u64 * MILLIS_PER_CHARACTER)
        .clamp(MIN_DISPLAY_MILLIS, MAX_DISPLAY_MILLIS);
    Duration::from_millis(millis)
}

pub(crate) fn is_human_readable_codex_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    if lower.contains(".luau")
        || lower.contains("src/")
        || lower.contains("src\\")
        || line.contains('`')
    {
        return false;
    }
    let first_word = line.split_whitespace().next().unwrap_or_default();
    if matches!(
        first_word,
        "local"
            | "function"
            | "return"
            | "if"
            | "elseif"
            | "else"
            | "for"
            | "while"
            | "repeat"
            | "until"
            | "end"
            | "require"
            | "export"
            | "import"
    ) {
        return false;
    }
    !matches!(line.chars().next(), Some('-' | '{' | '}' | '(' | ')'))
        && !line.contains(" = ")
        && !line.contains("=>")
        && !(line.contains('(') && line.contains(')'))
}
