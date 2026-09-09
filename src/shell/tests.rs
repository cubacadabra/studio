use super::*;

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
