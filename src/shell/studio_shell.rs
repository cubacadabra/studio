use super::*;
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
        let source_syntax = source_syntax_for_path(Path::new("src/main.luau"));
        Self {
            context,
            state,
            renderer,
            workspace: Workspace::default(),
            review_camera: ReviewCameraPreset::Gameplay,
            review_camera_reset: false,
            runtime_viewport: Rect::NOTHING,
            scene_outline,
            authoring_scene_source: None,
            runtime_ui_nodes: Vec::new(),
            expanded_scene,
            selected_scene,
            scene_focus_requested: false,
            scene_editor_target: String::new(),
            scene_editor_position: [0.0; 3],
            scene_editor_size: [1.0; 3],
            scene_editor_scale: [1.0; 3],
            scene_editor_position_text: std::array::from_fn(|_| "0".to_owned()),
            scene_editor_size_text: std::array::from_fn(|_| "1".to_owned()),
            scene_editor_scale_text: std::array::from_fn(|_| "1".to_owned()),
            scene_editor_text: String::new(),
            scene_editor_properties: BTreeMap::new(),
            scene_object_projections: Vec::new(),
            scene_viewport_edit_requested: None,
            selected_world_asset,
            selected_asset: "forest-grass",
            test_tool: "Sessions",
            asset_filter: "All",
            // A project opened from the command line starts in the live
            // workspace. The no-argument bootstrap changes this to the start
            // screen in `set_start_screen` before the first frame.
            playing: true,
            project_editable: false,
            project_dirty: false,
            preview_stale: false,
            project_error: None,
            scene_edit_requested: None,
            undo_requested: false,
            redo_requested: false,
            save_requested: false,
            rebuild_and_play_requested: false,
            rebuild_preview_requested: false,
            restart_requested: false,
            notice: "Ready".to_owned(),
            search_query: String::new(),
            scene_search_query: String::new(),
            scene_search_matches: BTreeSet::new(),
            scene_tree_rows: Vec::new(),
            scene_tree_rows_dirty: true,
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
            codex_live_excerpt: String::new(),
            codex_live_pending_excerpt: String::new(),
            codex_live_excerpt_queue: VecDeque::new(),
            codex_live_last_published_at: None,
            codex_live_last_received_at: None,
            codex_live_needs_separator: false,
            codex_live_in_code_block: false,
            codex_cancel_requested: false,
            codex_cancel_sent: false,
            codex_chat_error: None,
            codex_change_files: None,
            codex_source_change_count: 0,
            codex_change_review_open: false,
            codex_undo_requested: false,
            source_files: BTreeMap::new(),
            source_assets: BTreeMap::new(),
            source_directories: BTreeSet::new(),
            source_collapsed_directories: BTreeSet::new(),
            source_import_requested: None,
            dropped_files: Vec::new(),
            selected_source_file: None,
            selected_source_asset: None,
            source_asset_texture: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            audio_preview: None,
            source_editor_text: String::new(),
            source_editor: CodeEditor::default(),
            source_syntax,
            start_screen: false,
            start_screen_logged: false,
            recent_projects: Vec::new(),
            recent_project_requested: None,
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
            pending_project_action: None,
            exit_requested: false,
            imported_asset_paths: Vec::new(),
            roughness: 0.72,
            pending_textures_delta: egui::TexturesDelta::default(),
        }
    }
}
