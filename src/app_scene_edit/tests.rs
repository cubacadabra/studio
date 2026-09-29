use super::*;

fn block_node(id: &str, position: [f32; 3], size: [f32; 3]) -> AuthoringNode {
    AuthoringNode {
        id: id.to_owned(),
        parent_id: Some("world".to_owned()),
        name: id.to_owned(),
        transform: Transform {
            position,
            ..Transform::default()
        },
        components: BTreeMap::from([(
            "primitive".to_owned(),
            serde_json::json!({ "shape": "box", "size": size }),
        )]),
        editor: EditorMetadata::default(),
        source: None,
    }
}

fn scene_with(nodes: Vec<AuthoringNode>) -> AuthoringScene {
    let mut all_nodes = vec![AuthoringNode {
        id: "world".to_owned(),
        parent_id: None,
        name: "World".to_owned(),
        transform: Transform::default(),
        components: BTreeMap::new(),
        editor: EditorMetadata::default(),
        source: None,
    }];
    all_nodes.extend(nodes);
    AuthoringScene {
        format_version: 1,
        world_id: Some("starter-world".to_owned()),
        nodes: all_nodes,
    }
}

#[test]
fn scene_history_records_one_transaction_and_invalidates_redo() {
    let mut undo = vec![SceneHistoryEntry {
        kind: SceneHistoryKind::Snapshot {
            before: "old".to_owned(),
            after: "middle".to_owned(),
            target: "node".to_owned(),
        },
    }];
    let mut redo = vec![SceneHistoryEntry {
        kind: SceneHistoryKind::Snapshot {
            before: "middle".to_owned(),
            after: "new".to_owned(),
            target: "node".to_owned(),
        },
    }];
    record_scene_history(
        &mut undo,
        &mut redo,
        "middle".to_owned(),
        "latest".to_owned(),
        "node",
    );
    assert_eq!(undo.len(), 2);
    assert!(redo.is_empty());
    let SceneHistoryKind::Snapshot { before, after, .. } = &undo[1].kind else {
        panic!("expected snapshot history entry");
    };
    assert_eq!(before, "middle");
    assert_eq!(after, "latest");
}

#[test]
fn new_blocks_use_the_nearest_open_ground_slot() {
    let empty = scene_with(Vec::new());
    assert_eq!(next_block_position(&empty), [0.0, 1.0, 0.0]);

    let one_block = scene_with(vec![block_node("block-1", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE)]);
    assert_eq!(next_block_position(&one_block), [2.5, 1.0, 0.0]);

    let wide_block = scene_with(vec![block_node(
        "block-wide",
        [0.0, 1.0, 0.0],
        [6.0, 2.0, 2.0],
    )]);
    assert_eq!(next_block_position(&wide_block), [0.0, 1.0, 2.5]);
}

#[test]
fn duplicated_blocks_are_offset_by_their_visible_width() {
    let mut block = block_node("block-1", [3.0, 1.0, 4.0], [2.0, 2.0, 2.0]);
    block.transform.scale = [1.5, 1.0, 1.0];

    offset_duplicate(&mut block);

    assert_eq!(block.transform.position, [6.5, 1.0, 4.0]);
}

#[test]
fn imported_roblox_nodes_are_not_deletable_yet() {
    let mut block = block_node("block-1", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE);
    block.source = Some(cubacadabra_scene::SourceMetadata {
        format: "roblox".to_owned(),
        class: Some("Part".to_owned()),
        path: Some("Workspace:Workspace[1]/Part:Block[1]".to_owned()),
        properties: BTreeMap::new(),
    });
    assert!(is_roblox_source_linked(&block));
}

#[test]
fn structured_component_properties_can_be_added_and_removed() {
    let mut block = block_node("block-1", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE);
    set_authoring_component_property(
        &mut block,
        "primitive.material",
        Value::String("builtin:grass".to_owned()),
    )
    .unwrap();
    assert_eq!(block.components["primitive"]["material"], "builtin:grass");

    let removed = remove_authoring_component_property(&mut block, "primitive.material").unwrap();
    assert_eq!(removed, "builtin:grass");
    assert!(block.components["primitive"].get("material").is_none());
}

#[test]
fn editing_legacy_primitive_appearance_writes_canonical_fields() {
    let mut block = block_node("block-1", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE);
    block.components.insert(
        "primitive".to_owned(),
        serde_json::json!({
            "shape": "box",
            "size": NEW_BLOCK_SIZE,
            "material": "#767F91",
            "runtimeMaterial": "builtin:rock"
        }),
    );

    set_authoring_component_property(
        &mut block,
        "primitive.color",
        Value::String("#62A85A".to_owned()),
    )
    .unwrap();

    assert_eq!(block.components["primitive"]["color"], "#62A85A");
    assert_eq!(block.components["primitive"]["material"], "builtin:rock");
    assert!(
        block.components["primitive"]
            .get("runtimeMaterial")
            .is_none()
    );
}

#[test]
fn multi_selection_keeps_only_top_level_targets() {
    let mut parent = block_node("parent", [0.0, 1.0, 0.0], NEW_BLOCK_SIZE);
    parent.parent_id = Some("world".to_owned());
    let mut child = block_node("child", [1.0, 0.0, 0.0], NEW_BLOCK_SIZE);
    child.parent_id = Some("parent".to_owned());
    let sibling = block_node("sibling", [4.0, 1.0, 0.0], NEW_BLOCK_SIZE);
    let scene = scene_with(vec![parent, child, sibling]);

    let targets = top_level_scene_targets(
        &scene,
        &[
            "parent".to_owned(),
            "child".to_owned(),
            "sibling".to_owned(),
        ],
    );

    assert_eq!(targets, vec!["parent", "sibling"]);
}
