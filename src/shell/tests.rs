use super::*;

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
            [
                "File",
                "Edit",
                "Window",
                "World",
                "Assets",
                "Materials",
                "Test"
            ]
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
                inline_icon(ui, Icon::World, MUTED);
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
