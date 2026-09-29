use super::*;

#[test]
fn play_overlay_keeps_one_full_view_and_nonoverlapping_previews() {
    let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 720.0));
    for players in [3, 6, 9] {
        for controlled in 0..players {
            let mut player_slots: Vec<_> = (0..players).collect();
            player_slots.swap(0, controlled);
            let tiles = play_overlay_rects(viewport, &player_slots);
            assert_eq!(tiles.len(), players);
            assert_eq!(tiles[controlled], viewport);
            assert!(tiles.iter().all(|tile| viewport.contains_rect(*tile)));
            for (index, tile) in tiles.iter().enumerate() {
                if index == controlled {
                    continue;
                }
                assert!(tiles.iter().enumerate().all(|(other_index, other)| {
                    other_index == controlled || other_index == index || !tile.intersects(*other)
                }));
            }
        }
    }
}

#[test]
fn nine_player_layout_places_eight_previews_around_the_full_view() {
    for size in [
        egui::vec2(390.0, 844.0),
        egui::vec2(768.0, 1024.0),
        egui::vec2(1280.0, 800.0),
        egui::vec2(1440.0, 900.0),
    ] {
        let viewport = Rect::from_min_size(Pos2::ZERO, size);
        let mut player_slots: Vec<_> = (0..9).collect();
        let tiles = play_overlay_rects(viewport, &player_slots);
        assert_eq!(tiles[0], viewport);
        assert!(tiles[1..].iter().all(|tile| viewport.contains_rect(*tile)));
        assert!(tiles[1].center().x == viewport.center().x);
        assert!(tiles[8].center().x == viewport.center().x);
        assert!(tiles[2].max.x < tiles[1].min.x);
        assert!(tiles[3].min.x > tiles[1].max.x);
        assert!(tiles[8].min.y > tiles[6].min.y);
        player_slots.swap(0, 4);
        let switched = play_overlay_rects(viewport, &player_slots);
        assert_eq!(switched[4], viewport);
        assert_eq!(switched[0], tiles[4]);
        assert_eq!(switched[2], tiles[2]);
    }
}

fn projection(corners: [Pos2; 4]) -> SceneObjectProjection {
    SceneObjectProjection {
        id: "block".to_owned(),
        position: [0.0, 1.0, 0.0],
        rotation: [0.0; 3],
        local_rotation: [0.0; 3],
        size: Some([4.0, 1.0, 4.0]),
        scale: None,
        base_size: Some([4.0, 1.0, 4.0]),
        primitive_size: false,
        editable: true,
        center_screen: egui::pos2(15.0, 15.0),
        world_corners: None,
        screen_corners: Some(corners),
        bottom_screen_corners: None,
    }
}

#[test]
fn move_drag_uses_total_delta_to_hit_test_the_press_origin() {
    let projection = projection([
        egui::pos2(10.0, 10.0),
        egui::pos2(20.0, 10.0),
        egui::pos2(20.0, 20.0),
        egui::pos2(10.0, 20.0),
    ]);

    assert_eq!(
        scene_move_drag_origin(&projection, egui::pos2(35.0, 35.0), egui::vec2(20.0, 20.0)),
        Some(egui::pos2(15.0, 15.0))
    );
    assert_eq!(
        scene_move_drag_origin(&projection, egui::pos2(35.0, 35.0), egui::vec2(5.0, 5.0)),
        None
    );
}

#[test]
fn move_hit_test_includes_visible_cube_sides() {
    let mut projection = projection([
        egui::pos2(10.0, 10.0),
        egui::pos2(30.0, 10.0),
        egui::pos2(30.0, 30.0),
        egui::pos2(10.0, 30.0),
    ]);
    projection.bottom_screen_corners = Some([
        egui::pos2(10.0, 40.0),
        egui::pos2(30.0, 40.0),
        egui::pos2(30.0, 60.0),
        egui::pos2(10.0, 60.0),
    ]);

    assert!(projection.contains(egui::pos2(20.0, 35.0)));
    assert!(projection.bounds().contains(egui::pos2(20.0, 35.0)));
}
