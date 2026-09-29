use super::studio_viewport_ui::play_overlay_rects;
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
        self.play_viewports
            .get(self.controlled_player)
            .copied()
            .unwrap_or(self.runtime_viewport)
    }

    pub(crate) fn play_viewports(&self) -> &[Rect] {
        &self.play_viewports
    }

    pub(crate) fn play_player_count(&self) -> usize {
        if self.playing {
            self.play_player_count
        } else {
            1
        }
    }

    pub(crate) fn play_player_name(&self, index: usize) -> Option<&str> {
        self.play_player_names.get(index).map(String::as_str)
    }

    pub(crate) fn play_player_names(&self) -> &[String] {
        &self.play_player_names
    }

    pub(crate) fn controlled_player(&self) -> usize {
        self.controlled_player
    }

    pub(crate) fn select_play_player(&mut self, point: egui::Pos2) -> bool {
        let Some(index) = self
            .play_viewports
            .iter()
            .enumerate()
            .find(|(index, rect)| *index != self.controlled_player && rect.contains(point))
            .map(|(index, _)| index)
        else {
            return false;
        };
        self.play_player_slots.swap(self.controlled_player, index);
        self.controlled_player = index;
        // The slot map is the identity-bearing state. Rebuild the geometry
        // from it instead of swapping a second, potentially stale rectangle
        // array in parallel.
        self.play_viewports = play_overlay_rects(self.runtime_viewport, &self.play_player_slots);
        true
    }

    pub(crate) fn start_play(&mut self, players: usize) {
        self.play_player_count = players;
        self.play_player_names = if players > 1 {
            random_preview_names(players)
        } else {
            Vec::new()
        };
        self.play_player_slots = (0..players).collect();
        self.controlled_player = 0;
        self.play_viewports.clear();
        if self.project_dirty || self.preview_stale {
            self.request_rebuild_and_play();
        } else {
            self.set_playing(true);
            self.restart_requested = true;
            self.notice = "Restarting preview…".to_owned();
        }
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playing
    }

    pub(crate) fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if !playing {
            self.play_viewports.clear();
            self.controlled_player = 0;
            self.play_player_slots = (0..self.play_player_count).collect();
        }
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

    pub(crate) fn mark_scene_dirty(&mut self) {
        self.project_dirty = true;
        self.preview_stale = true;
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
        self.scene_search_matches_query.clear();
        self.scene_search_matches.clear();
        self.scene_search_result_count = 0;
        self.selected_scene = selected;
        self.selected_scenes = BTreeSet::from([self.selected_scene.clone()]);
        self.project_dirty = dirty;
        if dirty {
            self.preview_stale = true;
        }
        self.scene_editor_target.clear();
        self.scene_editor_name.clear();
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
        self.selected_scenes
            .retain(|id| outline.root.find(id).is_some());
        if self.selected_scenes.is_empty() {
            self.selected_scenes.insert(self.selected_scene.clone());
        } else if !self.selected_scenes.contains(&self.selected_scene) {
            self.selected_scene = self
                .selected_scenes
                .iter()
                .next()
                .cloned()
                .unwrap_or_else(|| outline.initial_selection.clone());
        }
        self.expanded_scene = previous_expanded
            .into_iter()
            .filter(|id| outline.root.find(id).is_some())
            .collect();
        self.expanded_scene
            .extend(outline.initial_expanded.iter().cloned());
        self.scene_outline = outline;
        self.scene_search_query.clear();
        self.scene_search_matches_query.clear();
        self.scene_search_matches.clear();
        self.scene_search_result_count = 0;
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
        self.scene_editor_name.clear();
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
            self.selected_scenes = BTreeSet::from([self.selected_scene.clone()]);
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
            previous_selections: self.selected_scenes.clone(),
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

    pub(crate) fn scene_object_geometries(&self) -> &[SceneObjectGeometry] {
        self.scene_outline.placeable_object_geometries()
    }

    pub(crate) fn take_scene_focus_request(&mut self) -> Option<SceneObjectGeometry> {
        if !std::mem::take(&mut self.scene_focus_requested) {
            return None;
        }
        self.scene_object_geometries()
            .iter()
            .find(|geometry| geometry.id == self.selected_scene)
            .cloned()
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
            .iter()
            .find(|geometry| geometry.id == self.selected_scene && geometry.editable)
            .cloned()
    }

    pub(crate) fn update_scene_object_geometry(
        &mut self,
        id: &str,
        position: [f32; 3],
        rotation: [f32; 3],
        local_rotation: [f32; 3],
        scale: [f32; 3],
        size: Option<[f32; 3]>,
    ) {
        if let Some(geometry) = self
            .scene_outline
            .placeable_objects
            .iter_mut()
            .find(|geometry| geometry.id == id)
        {
            geometry.position = position;
            geometry.rotation = rotation;
            geometry.local_rotation = local_rotation;
            geometry.scale = Some(scale);
            if let Some(size) = size {
                geometry.size = Some(size);
            }
        }
    }

    pub(crate) fn set_scene_object_projections(&mut self, projections: Vec<SceneObjectProjection>) {
        self.scene_object_projections = projections;
    }

    pub(crate) fn take_scene_viewport_edit_request(&mut self) -> Option<SceneViewportEditRequest> {
        self.scene_viewport_edit_requested.take()
    }

    pub(crate) fn set_scene_tool(&mut self, tool: SceneTool) {
        if self.scene_tool == tool {
            return;
        }
        self.scene_tool = tool;
        self.notice = format!("{} tool — {}", tool.label(), tool.hint(true));
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

    pub(crate) fn select_scene_node(&mut self, id: &str) -> bool {
        if self.scene_outline.root.find(id).is_none() {
            return false;
        }
        self.selected_scene = id.to_owned();
        self.selected_scenes.clear();
        self.selected_scenes.insert(id.to_owned());
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

    pub(crate) fn toggle_scene_node_selection(&mut self, id: &str) -> bool {
        if self.scene_outline.root.find(id).is_none() {
            return false;
        }
        if !self.selected_scenes.remove(id) {
            self.selected_scenes.insert(id.to_owned());
            self.selected_scene = id.to_owned();
        } else if self.selected_scene == id {
            self.selected_scene = self
                .selected_scenes
                .iter()
                .next_back()
                .cloned()
                .unwrap_or_else(|| self.scene_outline.initial_selection.clone());
        }
        if self.selected_scenes.is_empty() {
            self.selected_scenes.insert(self.selected_scene.clone());
        }
        let mut path = Vec::new();
        if self
            .scene_outline
            .root
            .collect_ancestor_ids(&self.selected_scene, &mut path)
        {
            self.expanded_scene.extend(path);
            self.scene_tree_rows_dirty = true;
        }
        true
    }

    pub(crate) fn select_scene_nodes(&mut self, ids: &[String]) -> bool {
        let selected = ids
            .iter()
            .filter(|id| self.scene_outline.root.find(id).is_some())
            .cloned()
            .collect::<BTreeSet<_>>();
        let Some(primary) = ids.iter().rev().find(|id| selected.contains(*id)).cloned() else {
            return false;
        };
        self.selected_scene = primary;
        self.selected_scenes = selected;
        let mut expanded = false;
        for id in &self.selected_scenes {
            let mut path = Vec::new();
            if self.scene_outline.root.collect_ancestor_ids(id, &mut path) {
                let before = self.expanded_scene.len();
                self.expanded_scene.extend(path);
                expanded |= before != self.expanded_scene.len();
            }
        }
        if expanded {
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

    pub(crate) fn take_publish_game_request(&mut self) -> bool {
        std::mem::take(&mut self.publish_game_requested)
    }

    pub(crate) fn set_publish_game_pending(&mut self, pending: bool) {
        self.publish_game_pending = pending;
    }

    pub(crate) fn can_publish_game(&self) -> bool {
        self.auth_user.is_some()
            && self.project_editable
            && self.project_loading.is_none()
            && !self.publish_game_pending
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

    pub(crate) fn clear_auth_user(&mut self) {
        self.auth_user = None;
    }

    pub(crate) fn take_chatgpt_auth_request(&mut self) -> bool {
        std::mem::take(&mut self.chatgpt_auth_requested)
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

    pub(crate) fn take_roblox_import_request(&mut self) -> bool {
        std::mem::take(&mut self.roblox_import_requested)
    }

    pub(crate) fn take_roblox_export_request(&mut self) -> bool {
        std::mem::take(&mut self.roblox_export_requested)
    }

    pub(crate) fn set_new_project_title(&mut self, title: String) {
        self.new_project_title = title;
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn take_new_project_cancelled(&mut self) -> bool {
        std::mem::take(&mut self.new_project_cancelled)
    }

    pub(crate) fn begin_project_loading(&mut self) {
        if self.project_loading.is_some() {
            return;
        }
        let empty = SceneOutline::empty();
        let empty_selection = empty.initial_selection.clone();
        let empty_selections = BTreeSet::from([empty_selection.clone()]);
        let empty_expanded = empty.initial_expanded.clone();
        self.project_loading = Some(ProjectLoadingState {
            progress: 0.0,
            rebuilding: false,
            previous_preview_stale: self.preview_stale,
            previous_outline: std::mem::replace(&mut self.scene_outline, empty),
            previous_expanded: std::mem::replace(&mut self.expanded_scene, empty_expanded),
            previous_selection: std::mem::replace(&mut self.selected_scene, empty_selection),
            previous_selections: std::mem::replace(&mut self.selected_scenes, empty_selections),
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
        self.selected_scenes = loading.previous_selections;
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

    #[cfg(not(target_os = "macos"))]
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

fn random_preview_names(players: usize) -> Vec<String> {
    const NAMES: &[&str] = &[
        "Alex", "Amira", "Avery", "Ben", "Camila", "Chloe", "Daniel", "Eli", "Emma", "Felix",
        "Grace", "Hana", "Isaac", "Jade", "Jamal", "Kai", "Leah", "Leo", "Lila", "Maya", "Mia",
        "Nina", "Noah", "Omar", "Priya", "Rafael", "Riley", "Sam", "Sofia", "Theo", "Uma", "Zoe",
    ];
    let mut names = NAMES.to_vec();
    let mut random = [0_u8; NAMES.len()];
    if let Err(error) = getrandom::fill(&mut random) {
        log::warn!("could not randomize preview names: {error}");
    }
    for index in (1..names.len()).rev() {
        names.swap(index, usize::from(random[index]) % (index + 1));
    }
    names.into_iter().take(players).map(str::to_owned).collect()
}

#[cfg(test)]
mod preview_name_tests {
    use super::random_preview_names;

    #[test]
    fn preview_names_are_unique_for_nine_players() {
        let names = random_preview_names(9);
        assert_eq!(names.len(), 9);
        assert_eq!(
            names.iter().collect::<std::collections::HashSet<_>>().len(),
            9
        );
    }
}

#[cfg(test)]
pub(crate) use super::studio_codex_state::{
    codex_live_chunk_end, codex_live_display_time, is_human_readable_codex_line,
};
