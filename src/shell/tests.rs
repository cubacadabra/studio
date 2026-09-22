use super::*;
use cubacadabra_scene::{AuthoringNode, AuthoringScene, EditorMetadata, SourceMetadata, Transform};

#[test]
fn world_is_the_default_workspace() {
    assert_eq!(Workspace::default(), Workspace::World);
}

#[test]
fn scene_outline_uses_artist_facing_manifest_content() {
    let outline = SceneOutline::parse(
        r#"{
            "id": "gallery-game",
            "displayName": "Gallery Game",
            "version": "1.2.3",
            "lobby": false,
            "startWorld": "lobby",
            "launch": { "destinationWorld": "gallery" },
            "package": { "entry": "game.luau" },
            "assets": {
                "images": { "gallery-banner": { "path": "assets/gallery.png" } },
                "audio": { "room-tone": { "path": "assets/room.wav" } }
            },
            "world": { "spawn": [0, 1, 2], "clouds": [] },
            "avatars": {
                "player": { "character": { "body": "cuba:person.v1", "face": "happy" } }
            },
            "worlds": {
                "gallery": {
                    "world": { "groundSize": 64, "spawn": [2, 0, 8] },
                    "materials": { "paintedWood": { "image": "wood", "tileU": 4 } },
                    "blocks": [{ "position": [1, 2, 3], "color": "paintedWood" }],
                    "signs": [{ "text": "Welcome artists", "position": [0, 2, 0] }],
                    "interactions": [{ "id": "open-gallery", "label": "Open Gallery" }],
                    "server": { "ambientNpcs": { "entities": [{ "id": "curator", "username": "Curator" }] } }
                }
            }
        }"#,
    )
    .unwrap();

    assert_eq!(outline.root.label, "Gallery Game");
    assert_eq!(outline.initial_selection, "world/gallery");
    assert!(outline.initial_expanded.contains("world/gallery"));
    assert_eq!(
        outline.root.find("world/gallery/blocks/0").unwrap().label,
        "Block 1"
    );
    assert_eq!(
        outline.root.find("world/gallery/signs/0").unwrap().label,
        "Welcome artists"
    );
    assert_eq!(
        outline
            .root
            .find("world/gallery/characters/0")
            .unwrap()
            .label,
        "Curator"
    );
    assert!(outline.root.find("game/characters/player").is_some());
    assert!(
        outline
            .assets
            .iter()
            .any(|asset| asset.name == "gallery-banner" && asset.kind == "IMAGE")
    );
    assert!(
        outline
            .assets
            .iter()
            .any(|asset| asset.name == "paintedWood" && asset.kind == "MATERIAL")
    );
    assert!(!outline.assets.iter().any(|asset| asset.name == "castle"));
    assert!(!scene_text(&outline.root).contains("luau"));
}

#[test]
fn scene_outline_exposes_editable_placeable_objects_to_viewport_tools() {
    let outline = SceneOutline::parse(
        r#"{
            "id": "tools",
            "worlds": {
                "course": {
                    "blocks": [{"position": [1, 2, 3], "size": [4, 1, 4]}],
                    "signs": [{"text": "Hi", "position": [2, 3, 4]}],
                    "ladders": [{"id": "ladder", "position": [3, 4, 5], "size": [2, 6, 1]}],
                    "interactions": [{"id": "use", "position": [4, 5, 6]}],
                    "checkpoints": [{"id": "save", "position": [5, 6, 7]}],
                    "hazards": [{"id": "hurt", "position": [6, 7, 8], "size": [3, 1, 3]}],
                    "safeZones": [{"id": "safe", "position": [7, 8, 9]}]
                }
            }
        }"#,
    )
    .unwrap();

    let objects = outline.placeable_object_geometries();
    assert_eq!(objects.len(), SceneObjectKind::ALL.len());
    assert_eq!(
        objects
            .iter()
            .filter(|object| object.size.is_some())
            .count(),
        3
    );
    assert!(objects.iter().all(|object| is_scene_object(&object.id)));
}

#[test]
fn authoring_scene_uses_stable_component_nodes_for_vegas() {
    let manifest = include_str!("../../../examples/vegas-101/manifest.json");
    let scene = parse_authoring_scene(include_str!("../../../examples/vegas-101/scene.json"))
        .expect("Vegas authoring scene");
    let outline =
        SceneOutline::parse_with_authoring_scene(manifest, &scene).expect("authoring outline");
    assert_eq!(outline.root.find("sign-tables").unwrap().label, "TABLES");
    let imported_source = outline
        .root
        .find("imported-environment/imported-source")
        .expect("imported source should be grouped separately");
    assert_eq!(imported_source.label, "Imported Source");
    assert!(
        !outline
            .initial_expanded
            .iter()
            .any(|id| id.starts_with("source-hierarchy-"))
    );
    assert!(!outline.initial_expanded.contains("signs"));
    let linked_source_folder = scene
        .nodes
        .iter()
        .find(|node| {
            node.source
                .as_ref()
                .and_then(|source| source.path.as_deref())
                == Some("Workspace:Workspace[1]/Folder:Map[1]")
        })
        .expect("linked source folder should be present");
    assert!(linked_source_folder.editor.locked);
    assert!(!outline.initial_expanded.contains(&linked_source_folder.id));
    assert_eq!(
        outline.root.find("interaction-plinko").unwrap().kind,
        "Interaction"
    );
    assert!(
        outline
            .root
            .find("vegas-map")
            .unwrap()
            .properties
            .iter()
            .any(|(label, value)| label == "Locked" && value == "Yes")
    );
    let objects = outline.placeable_object_geometries();
    assert_eq!(objects.len(), 2124);
    assert_eq!(
        objects
            .iter()
            .filter(|object| object.id.starts_with("vegas-chair-"))
            .count(),
        245
    );
    assert_eq!(
        objects
            .iter()
            .filter(|object| object.id.starts_with("imported-part-"))
            .count(),
        1864
    );
    assert!(objects.iter().all(|object| {
        !matches!(
            object.id.as_str(),
            "world-vegas-floor" | "imported-environment" | "signs" | "interactions"
        )
    }));
    assert_eq!(
        objects
            .iter()
            .find(|object| object.id == "sign-tables")
            .unwrap()
            .position,
        [-12.4, 11.0, 14.2]
    );
    let chair_id = objects
        .iter()
        .find(|object| object.id.starts_with("vegas-chair-"))
        .expect("promoted chair should be editable")
        .id
        .clone();
    let chair = outline
        .root
        .find(&chair_id)
        .expect("promoted chair should exist in the outline");
    assert!(chair.label.starts_with("Vegas Chair "));
    assert!(
        chair
            .properties
            .iter()
            .any(|(label, value)| { label == "Size" && value == "4.127, 4.306, 4.181" })
    );
}

#[test]
fn duplicate_imported_model_names_get_presentation_ordinals() {
    let scene = AuthoringScene {
        format_version: 1,
        world_id: Some("world".to_owned()),
        nodes: vec![
            AuthoringNode {
                id: "world".to_owned(),
                parent_id: None,
                name: "World".to_owned(),
                transform: Transform::default(),
                components: BTreeMap::new(),
                editor: EditorMetadata::default(),
                source: None,
            },
            AuthoringNode {
                id: "plinko-a".to_owned(),
                parent_id: Some("world".to_owned()),
                name: "Plinko".to_owned(),
                transform: Transform::default(),
                components: BTreeMap::new(),
                editor: EditorMetadata::default(),
                source: Some(SourceMetadata {
                    format: "roblox".to_owned(),
                    class: Some("Model".to_owned()),
                    path: Some("Workspace/PlinkoA".to_owned()),
                    properties: BTreeMap::new(),
                }),
            },
            AuthoringNode {
                id: "plinko-b".to_owned(),
                parent_id: Some("world".to_owned()),
                name: "Plinko".to_owned(),
                transform: Transform::default(),
                components: BTreeMap::new(),
                editor: EditorMetadata::default(),
                source: Some(SourceMetadata {
                    format: "roblox".to_owned(),
                    class: Some("Model".to_owned()),
                    path: Some("Workspace/PlinkoB".to_owned()),
                    properties: BTreeMap::new(),
                }),
            },
        ],
    };
    let outline = SceneOutline::parse_with_authoring_scene(
        r#"{"startWorld":"world","worlds":{"world":{}}}"#,
        &scene,
    )
    .expect("duplicate imported models should render in the outline");
    assert_eq!(outline.root.find("plinko-a").unwrap().label, "Plinko 1");
    assert_eq!(outline.root.find("plinko-b").unwrap().label, "Plinko 2");
    assert_eq!(scene.nodes[1].name, "Plinko");
    assert_eq!(scene.nodes[2].name, "Plinko");
}

#[test]
fn authoring_outline_keeps_multiple_roots_when_no_world_root_matches() {
    let manifest = r#"{
        "startWorld": "course",
        "worlds": { "course": { "world": {} } }
    }"#;
    let scene = parse_authoring_scene(
        r#"{
            "formatVersion": 1,
            "nodes": [
                { "id": "first", "name": "First", "components": {} },
                { "id": "second", "name": "Second", "components": {} }
            ]
        }"#,
    )
    .unwrap();
    let outline = SceneOutline::parse_with_authoring_scene(manifest, &scene).unwrap();
    assert!(outline.root.find("first").is_some());
    assert!(outline.root.find("second").is_some());
}

#[test]
fn scene_search_indexes_a_large_lightweight_tree() {
    let root = SceneNode {
        id: "game".to_owned(),
        label: "Game".to_owned(),
        kind: "Game",
        icon: Icon::World,
        detail: None,
        properties: Vec::new(),
        children: (0..10_000)
            .map(|index| SceneNode {
                id: format!("node-{index}"),
                label: format!("Node {index}"),
                kind: "Group",
                icon: Icon::Folder,
                detail: None,
                properties: Vec::new(),
                children: Vec::new(),
            })
            .collect(),
    };
    let outline = SceneOutline {
        root,
        assets: Vec::new(),
        initial_selection: "game".to_owned(),
        initial_expanded: BTreeSet::from(["game".to_owned()]),
        authoring_world_transforms: BTreeMap::new(),
        placeable_objects: Vec::new(),
    };
    let matches = outline.search_matches("Node 9999");
    assert!(matches.contains("game"));
    assert!(matches.contains("node-9999"));
    assert!(!matches.contains("node-9998"));
    assert_eq!(outline.search_result_count("Node 9999"), 1);
}

#[test]
fn scene_outline_surfaces_runtime_game_ui_without_exposing_source_files() {
    let mut outline = SceneOutline::parse(
        r#"{
            "id": "course",
            "displayName": "Course",
            "worlds": { "starter-world": { "world": {}, "blocks": [] } }
        }"#,
    )
    .unwrap();
    outline.set_runtime_ui_nodes(&[cubacadabra_client::StudioUiNode {
        id: "sky-greeting".to_owned(),
        kind: "Text".to_owned(),
        text: "hi there".to_owned(),
    }]);

    let node = outline
        .root
        .find("game/interface/sky-greeting")
        .expect("runtime text should appear in the Scene tree");
    assert_eq!(node.label, "sky-greeting");
    assert_eq!(node.kind, "Text");
    assert_eq!(node.detail.as_deref(), Some("hi there"));
    assert!(!scene_text(&outline.root).contains(".luau"));
}

fn scene_text(node: &SceneNode) -> String {
    let mut text = format!("{} {} {:?}", node.label, node.kind, node.properties);
    for child in &node.children {
        text.push_str(&scene_text(child));
    }
    text
}

#[cfg(target_os = "macos")]
#[test]
fn macos_system_font_uses_the_wider_text_optical_cut() {
    let bytes = fs::read(REGULAR_FONT_PATHS[0]).expect("macOS should provide its system UI font");
    let display_width = rendered_label_width(FontData::from_owned(bytes.clone()));
    let text_width = rendered_label_width(platform_font_data(bytes, REGULAR_FONT_WEIGHT));

    assert!(
        text_width > display_width * 1.08,
        "SF Text should be materially wider than SF Display: {text_width} <= {display_width}"
    );
}

#[cfg(target_os = "macos")]
fn rendered_label_width(font: FontData) -> f32 {
    let context = egui::Context::default();
    let mut fonts = FontDefinitions::default();
    let name = "optical-size-test".to_owned();
    fonts.font_data.insert(name.clone(), Arc::new(font));
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, name);
    context.set_fonts(fonts);
    let output = context.run_ui(egui::RawInput::default(), |ui| {
        ui.label(RichText::new("Environment").size(TYPE.primary));
    });
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.size().x),
            _ => None,
        })
        .expect("the label should produce a text shape")
}

#[test]
fn menu_labels_keep_padding_and_share_the_workspace_baseline() {
    for scale in [1.0, 2.0] {
        let context = egui::Context::default();
        configure_context(&context);
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(scale);
        let output = context.run_ui(input, |ui| {
            egui::MenuBar::new().style(menu_bar_style).ui(ui, |ui| {
                for label in ["File", "Edit", "Window"] {
                    ui.menu_button(RichText::new(label).size(TYPE.primary), |_| {});
                }
                ui.add_space(12.0);
                for workspace in Workspace::ALL {
                    workspace_tab(ui, workspace.label(), workspace == Workspace::World);
                }
            });
        });
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|clipped| {
                if let egui::Shape::Text(text) = &clipped.shape {
                    Some((
                        text.galley.job.text.as_str(),
                        Rect::from_min_size(text.pos, text.galley.size()),
                    ))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            labels.iter().map(|(label, _)| *label).collect::<Vec<_>>(),
            ["File", "Edit", "Window", "World", "Files", "Morphs",]
        );
        for (_, rect) in &labels {
            assert!(
                (rect.min.y - labels[0].1.min.y).abs() <= 1.0 / scale,
                "menu and workspace baselines differ at {scale}x: {labels:?}"
            );
        }
        for pair in labels[..3].windows(2) {
            let gap = pair[1].1.min.x - pair[0].1.max.x;
            assert!(
                (gap - LABEL_PADDING * 2.0).abs() <= 1.0 / scale,
                "menu label padding changed at {scale}x: gap {gap}"
            );
        }
    }
}

#[test]
fn editor_headers_keep_their_height_with_actions_at_different_widths() {
    for width in [224.0, 768.0, 1280.0, 1440.0] {
        let context = egui::Context::default();
        configure_context(&context);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 800.0),
            )),
            ..Default::default()
        };
        let _ = context.run_ui(input, |ui| {
            let available = ui.available_width();
            let rect = editor_header(ui, |ui| {
                inline_icon(ui, Icon::World, palette(ui).muted);
                ui.label("Scene");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    icon_button(ui, Icon::Filter, "Filter scene", false);
                    icon_button(ui, Icon::Plus, "Add object", false);
                });
            });
            assert_eq!(rect.height(), EDITOR_HEADER_HEIGHT);
            assert_eq!(rect.width(), available);
        });
    }
}

#[test]
fn toolbar_buttons_size_their_hitbox_to_their_label() {
    let context = egui::Context::default();
    configure_context(&context);
    let mut button_rect = Rect::NOTHING;
    let output = context.run_ui(egui::RawInput::default(), |ui| {
        button_rect = toolbar_button(ui, Icon::Character, "Sign in", false).rect;
    });
    let label_rect = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == "Sign in" => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
        .expect("the toolbar button should paint its label");

    assert!(button_rect.width() > 54.0);
    assert!(button_rect.contains(label_rect.right_center()));
}

#[test]
fn chatgpt_status_uses_readable_plan_names() {
    let account = ChatGptAccount {
        email: Some("player@example.com".to_owned()),
        plan_type: Some("self_serve_business_usage_based".to_owned()),
    };
    assert_eq!(chatgpt_account_label(&account), "ChatGPT · Business");

    let unknown_plan = ChatGptAccount {
        email: None,
        plan_type: Some("unknown".to_owned()),
    };
    assert_eq!(chatgpt_account_label(&unknown_plan), "ChatGPT");
}

#[test]
fn codex_chat_picker_exposes_requested_models_and_levels() {
    assert_eq!(
        CODEX_CHAT_MODELS
            .iter()
            .map(|(model, _)| *model)
            .collect::<Vec<_>>(),
        vec![
            "gpt-6-astra",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
        ]
    );
    assert_eq!(
        CODEX_CHAT_EFFORTS
            .iter()
            .map(|(effort, _)| *effort)
            .collect::<Vec<_>>(),
        vec!["medium", "high", "xhigh", "max"]
    );
    assert_eq!(
        codex_chat_model_label(CODEX_CHAT_DEFAULT_MODEL, CODEX_CHAT_CURRENT_MODEL),
        "gpt-6-astra (default)"
    );
    assert_eq!(
        codex_chat_model_label(CODEX_CHAT_CURRENT_MODEL, CODEX_CHAT_CURRENT_MODEL),
        "gpt-5.6-luna (current)"
    );
    assert_eq!(
        codex_chat_model_description("gpt-5.6-terra"),
        "Balanced agentic coding model for everyday work."
    );
}

#[test]
fn codex_agent_messages_remain_active_progress() {
    assert_eq!(
        CodexActivity::Thinking.after_agent_progress(),
        CodexActivity::Working
    );
    assert!(CodexActivity::Working.is_active());
    assert!(CodexActivity::Rebuilding.is_active());
    assert_eq!(
        CodexActivity::Rebuilding.after_agent_progress(),
        CodexActivity::Rebuilding
    );
}

#[test]
fn codex_live_activity_hides_code_and_luau_paths() {
    assert!(super::studio_state::is_human_readable_codex_line(
        "I’m updating the countdown behavior."
    ));
    assert!(!super::studio_state::is_human_readable_codex_line(
        "src/main.luau"
    ));
    assert!(!super::studio_state::is_human_readable_codex_line(
        "Reviewing src/main before editing it."
    ));
    assert!(!super::studio_state::is_human_readable_codex_line(
        "local timer = 30"
    ));
    assert!(!super::studio_state::is_human_readable_codex_line(
        "game.on_update(function()"
    ));
    assert!(!super::studio_state::is_human_readable_codex_line(
        "return finish_course()"
    ));
}

#[test]
fn codex_live_activity_queues_complete_readable_phrases() {
    use super::studio_state::{codex_live_chunk_end, codex_live_display_time};

    let activity = "Reviewing the scene layout. Updating the moving sky message next.";
    let first_end = codex_live_chunk_end(activity, false).expect("complete sentence");
    assert_eq!(&activity[..first_end], "Reviewing the scene layout.");
    assert_eq!(
        codex_live_chunk_end("Still reviewing the current behavior", false),
        None
    );
    assert_eq!(
        codex_live_chunk_end("Still reviewing the current behavior", true),
        Some("Still reviewing the current behavior".len())
    );
    assert!(codex_live_display_time("A longer readable activity update.") > Duration::from_secs(1));
}

#[test]
fn codex_live_activity_never_splits_a_word() {
    use super::studio_state::codex_live_chunk_end;

    let activity = "Reviewing the existing game behavior and visual layout before making a focused change that preserves everything else in the project safely";
    let end = codex_live_chunk_end(activity, false).expect("long phrase should be chunked");
    assert!(
        activity[..end]
            .chars()
            .next_back()
            .is_some_and(|character| !character.is_whitespace())
    );
    assert!(activity[end..].starts_with(' '));
}

#[test]
fn morph_preview_detects_screen_space_slivers() {
    assert!(
        projected_triangle_area([
            egui::pos2(10.0, 10.0),
            egui::pos2(10.005, 10.0),
            egui::pos2(10.0, 100.0),
        ]) < 0.5
    );
    assert!(
        projected_triangle_area([
            egui::pos2(10.0, 10.0),
            egui::pos2(30.0, 10.0),
            egui::pos2(10.0, 30.0),
        ]) > 0.5
    );
}

#[test]
fn shell_palette_follows_the_system_appearance() {
    for (theme, expected) in [
        (egui::Theme::Dark, DARK_PALETTE),
        (egui::Theme::Light, LIGHT_PALETTE),
    ] {
        let context = egui::Context::default();
        configure_context(&context);
        assert_eq!(
            context.options(|options| options.theme_preference),
            egui::ThemePreference::System
        );

        let input = egui::RawInput {
            system_theme: Some(theme),
            ..Default::default()
        };
        let _ = context.run_ui(input, |ui| {
            let actual = palette(ui);
            assert_eq!(ui.visuals().dark_mode, theme == egui::Theme::Dark);
            assert_eq!(actual.panel, expected.panel);
            assert_eq!(actual.panel_header, expected.panel_header);
            assert_eq!(actual.field, expected.field);
            assert_eq!(actual.text, expected.text);
            assert_eq!(ui.visuals().panel_fill, expected.panel);
        });
    }
}
