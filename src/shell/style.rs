use super::*;

pub(crate) fn configure_context(context: &egui::Context) {
    configure_fonts(context);
    configure_style(context);
}

pub(crate) fn configure_fonts(context: &egui::Context) {
    // egui rasterizes fonts itself, so use each desktop OS's installed UI face
    // and retain its bundled fonts as fallbacks for missing glyphs or files.
    let mut fonts = FontDefinitions::default();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_REGULAR,
        REGULAR_FONT_PATHS,
        REGULAR_FONT_WEIGHT,
    ) {
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .expect("egui should define its proportional fallback family")
            .insert(0, SYSTEM_UI_REGULAR.to_owned());
    }
    let proportional = fonts
        .families
        .get(&FontFamily::Proportional)
        .expect("egui should define its proportional fallback family")
        .clone();

    let mut medium_family = Vec::new();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_MEDIUM,
        MEDIUM_FONT_PATHS,
        MEDIUM_FONT_WEIGHT,
    ) {
        medium_family.push(SYSTEM_UI_MEDIUM.to_owned());
    }
    medium_family.extend(proportional.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(MEDIUM_FONT_FAMILY.into()), medium_family);

    let mut semibold_family = Vec::new();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_SEMIBOLD,
        SEMIBOLD_FONT_PATHS,
        SEMIBOLD_FONT_WEIGHT,
    ) {
        semibold_family.push(SYSTEM_UI_SEMIBOLD.to_owned());
    }
    semibold_family.extend(proportional);
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD_FONT_FAMILY.into()),
        semibold_family,
    );
    context.set_fonts(fonts);
}

pub(crate) fn install_font_face(
    fonts: &mut FontDefinitions,
    name: &str,
    paths: &[&str],
    weight: f32,
) -> bool {
    let Some(bytes) = read_first_font(paths) else {
        return false;
    };
    fonts
        .font_data
        .insert(name.to_owned(), Arc::new(platform_font_data(bytes, weight)));
    true
}

pub(crate) fn platform_font_data(bytes: Vec<u8>, weight: f32) -> FontData {
    let data = FontData::from_owned(bytes);
    #[cfg(target_os = "macos")]
    {
        // SFNS defaults to its narrower display cut outside AppKit. Select the
        // text optical size explicitly so small editor labels match native UI.
        let mut tweak = FontTweak::default();
        tweak.coords.push(b"opsz", UI_OPTICAL_SIZE);
        tweak.coords.push(b"wght", weight);
        tweak.hinting = Some(true);
        tweak.subpixel_binning = Some(false);
        data.tweak(tweak)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = weight;
        data
    }
}

pub(crate) fn read_first_font(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| fs::read(path).ok())
}

pub(crate) fn medium_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM_FONT_FAMILY.into()))
}

pub(crate) fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD_FONT_FAMILY.into()))
}

pub(crate) fn configure_style(context: &egui::Context) {
    configure_theme_style(context, egui::Theme::Dark, DARK_PALETTE);
    configure_theme_style(context, egui::Theme::Light, LIGHT_PALETTE);
    context.set_theme(egui::ThemePreference::System);
}

pub(crate) fn configure_theme_style(context: &egui::Context, theme: egui::Theme, palette: Palette) {
    let mut style = (*context.style_of(theme)).clone();
    style.spacing.item_spacing = egui::vec2(4.0, 1.0);
    style.spacing.button_padding = egui::vec2(6.0, 1.0);
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.spacing.indent = 12.0;
    style.spacing.menu_margin = Margin::same(4);
    style.animation_time = 0.15;
    style.visuals.dark_mode = theme == egui::Theme::Dark;
    style.visuals.text_options.font_hinting = true;
    style.visuals.text_options.subpixel_binning = false;
    style.visuals.panel_fill = palette.panel;
    style.visuals.window_fill = palette.panel_raised;
    style.visuals.window_stroke = Stroke::new(1.0, palette.border);
    style.visuals.window_corner_radius = egui::CornerRadius::same(2);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(3);
    style.visuals.extreme_bg_color = palette.surface_deep;
    style.visuals.text_edit_bg_color = Some(palette.field);
    style.visuals.faint_bg_color = palette.surface;
    style.visuals.indent_has_left_vline = false;
    style.visuals.selection.bg_fill = palette.selection;
    style.visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.muted);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.hovered.bg_fill = palette.panel_raised;
    style.visuals.widgets.hovered.weak_bg_fill = palette.panel_raised;
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.active.bg_fill = palette.selection;
    style.visuals.widgets.active.weak_bg_fill = palette.selection;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.open.bg_fill = palette.field;
    style.visuals.widgets.open.weak_bg_fill = palette.field;
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(2);
    // Egui removes text padding for frameless buttons, including menu labels.
    // Keep the frame geometry and make inactive menu frames transparent instead.
    style.visuals.button_frame = true;
    style.visuals.slider_trailing_fill = true;
    style.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(TYPE.secondary));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(TYPE.secondary));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(TYPE.meta));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(TYPE.secondary));
    context.set_style_of(theme, style);
}

pub(crate) fn palette(ui: &egui::Ui) -> Palette {
    if ui.visuals().dark_mode {
        DARK_PALETTE
    } else {
        LIGHT_PALETTE
    }
}

pub(crate) fn editor_frame(fill: Color32) -> Frame {
    Frame::NONE.fill(fill).inner_margin(Margin::same(0))
}

pub(crate) fn load_logo_texture(context: &egui::Context) -> egui::TextureHandle {
    let image = image::load_from_memory(LOGO_BYTES)
        .expect("Cubacadabra Studio logo should be a valid image")
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    context.load_texture(
        "cubacadabra-studio-logo",
        color_image,
        egui::TextureOptions::LINEAR,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn install_system_icon_textures(context: &egui::Context) {
    let textures = MACOS_SYSTEM_SYMBOLS
        .iter()
        .filter_map(|&(icon, symbol)| {
            let png = crate::macos::system_symbol_png(symbol)?;
            let image = system_icon_color_image(&png)?;
            let texture = context.load_texture(
                format!("sf-symbol-{symbol}"),
                image,
                egui::TextureOptions::LINEAR,
            );
            Some((icon, texture))
        })
        .collect::<HashMap<_, _>>();
    context.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(SYSTEM_ICON_ATLAS_ID),
            Arc::new(textures) as SystemIconAtlas,
        );
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn system_icon_color_image(png: &[u8]) -> Option<egui::ColorImage> {
    let rgba = image::load_from_memory(png).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut bounds = None::<(u32, u32, u32, u32)>;
    for (x, y, pixel) in rgba.enumerate_pixels() {
        if pixel[3] == 0 {
            continue;
        }
        bounds = Some(match bounds {
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
            None => (x, y, x, y),
        });
    }
    let (min_x, min_y, max_x, max_y) = bounds?;
    let min_x = min_x.saturating_sub(1);
    let min_y = min_y.saturating_sub(1);
    let max_x = (max_x + 1).min(width - 1);
    let max_y = (max_y + 1).min(height - 1);
    let mut cropped =
        image::imageops::crop_imm(&rgba, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
            .to_image();
    for pixel in cropped.pixels_mut() {
        *pixel = image::Rgba([255, 255, 255, pixel[3]]);
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [cropped.width() as usize, cropped.height() as usize],
        cropped.as_raw(),
    ))
}

pub(crate) fn menu_bar_style(style: &mut egui::Style) {
    style.spacing.item_spacing.x = 0.0;
    style.spacing.button_padding = egui::vec2(LABEL_PADDING, 4.0);
    style.spacing.interact_size.y = TOP_BAR_HEIGHT;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.open.bg_stroke = Stroke::NONE;
}

pub(crate) fn content_frame() -> Frame {
    Frame::NONE.inner_margin(Margin::symmetric(6, 4))
}

pub(crate) fn workspace_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let colors = palette(ui);
    let font = if selected {
        medium_font(TYPE.primary)
    } else {
        FontId::proportional(TYPE.primary)
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        font,
        if selected {
            colors.text
        } else {
            colors.secondary_text
        },
    );
    let width = galley.size().x.ceil() + LABEL_PADDING * 2.0;
    let (slot, response) =
        ui.allocate_exact_size(egui::vec2(width, TOP_BAR_HEIGHT), Sense::click());
    let rect = Rect::from_min_max(slot.min, slot.max);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, UI.radius, colors.panel_raised);
    }
    if selected {
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 1.0,
            Stroke::new(1.0, colors.selection),
        );
    }
    ui.painter().galley(
        egui::pos2(
            rect.min.x + LABEL_PADDING,
            slot.center().y - galley.size().y * 0.5,
        ),
        galley,
        if selected {
            colors.text
        } else {
            colors.secondary_text
        },
    );
    paint_focus(ui, &response);
    response
}

// Allocate headers once: frame strokes/margins must never change their height.
pub(crate) fn editor_header(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) -> Rect {
    let colors = palette(ui);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), EDITOR_HEADER_HEIGHT),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, colors.panel_header);
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - 0.5,
        Stroke::new(1.0, colors.border),
    );
    let mut header = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(UI.inset, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    header.set_clip_rect(ui.clip_rect().intersect(rect));
    header.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
    content(&mut header);
    rect
}

pub(crate) fn panel_header(
    ui: &mut egui::Ui,
    icon: Icon,
    title: &str,
    actions: impl FnOnce(&mut egui::Ui),
) {
    let colors = palette(ui);
    editor_header(ui, |ui| {
        inline_icon(ui, icon, colors.muted);
        ui.label(
            RichText::new(title)
                .font(semibold_font(TYPE.primary))
                .color(colors.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), actions);
    });
}

pub(crate) fn selected_object_header(ui: &mut egui::Ui, name: &str, kind: &str) {
    let colors = palette(ui);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), UI.row),
        Layout::left_to_right(Align::Center),
        |ui| {
            inline_icon(ui, Icon::Object, colors.muted);
            ui.add(
                egui::Label::new(
                    RichText::new(name)
                        .font(semibold_font(TYPE.primary))
                        .color(colors.text),
                )
                .truncate(),
            )
            .on_hover_text(kind);
        },
    );
}

pub(crate) fn property_section(
    ui: &mut egui::Ui,
    title: &str,
    content: impl FnOnce(&mut egui::Ui),
) {
    let colors = palette(ui);
    egui::CollapsingHeader::new(
        RichText::new(title)
            .font(semibold_font(TYPE.secondary))
            .color(colors.text),
    )
    .default_open(true)
    .show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        content(ui);
        ui.add_space(1.0);
    });
}

pub(crate) fn property_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let colors = palette(ui);
    let field = property_field(ui, label);
    ui.painter().rect_filled(field, UI.radius, colors.surface);
    ui.put(
        field.shrink2(egui::vec2(6.0, 0.0)),
        egui::Label::new(RichText::new(value).size(TYPE.secondary).color(colors.text)).truncate(),
    )
    .on_hover_text(value);
}

pub(crate) fn property_field(ui: &mut egui::Ui, label: &str) -> Rect {
    let colors = palette(ui);
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CONTROL_HEIGHT),
        Sense::hover(),
    );
    let label_width = 84.0;
    ui.painter().text(
        egui::pos2(row.min.x + label_width - 8.0, row.center().y),
        Align2::RIGHT_CENTER,
        label,
        FontId::proportional(TYPE.secondary),
        colors.secondary_text,
    );
    Rect::from_min_max(row.min + egui::vec2(label_width, 0.0), row.max)
}

pub(crate) fn show_scene_node(
    ui: &mut egui::Ui,
    node: &SceneNode,
    depth: usize,
    expanded_nodes: &mut BTreeSet<String>,
    selected: &mut String,
) {
    let expanded = expanded_nodes.contains(&node.id);
    let has_children = !node.children.is_empty();
    let colors = palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), UI.row), Sense::click());
    let is_selected = *selected == node.id;
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            is_selected,
            &node.label,
        )
    });
    if is_selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0.0,
            if is_selected {
                colors.selection
            } else {
                colors.panel_raised
            },
        );
    }
    paint_focus(ui, &response);
    let x = rect.min.x + UI.inset + depth as f32 * 12.0;
    let disclosure_rect =
        Rect::from_center_size(egui::pos2(x + 5.0, rect.center().y), Vec2::splat(14.0));
    let disclosure = has_children.then(|| {
        ui.interact(
            disclosure_rect,
            ui.id().with(("scene-disclosure", &node.id)),
            Sense::click(),
        )
    });
    if has_children {
        paint_icon(
            ui.painter(),
            disclosure_rect.shrink(2.0),
            if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            colors.faint,
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(egui::pos2(x + 19.0, rect.center().y), Vec2::splat(UI.icon)),
        node.icon,
        if is_selected {
            colors.text
        } else {
            colors.faint
        },
    );
    let label_font = if is_selected || has_children {
        medium_font(TYPE.primary)
    } else {
        FontId::proportional(TYPE.primary)
    };
    let detail_width = node.detail.as_ref().map_or(0.0, |detail| {
        ui.painter()
            .layout_no_wrap(
                detail.clone(),
                FontId::proportional(TYPE.meta),
                colors.faint,
            )
            .size()
            .x
            + 12.0
    });
    let label_left = x + 28.0;
    let label_right = (rect.max.x - detail_width).max(label_left);
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            egui::pos2(label_left, rect.min.y),
            egui::pos2(label_right, rect.max.y),
        ))
        .text(
            egui::pos2(x + 30.0, rect.center().y),
            Align2::LEFT_CENTER,
            &node.label,
            label_font,
            if is_selected {
                colors.text
            } else {
                colors.secondary_text
            },
        );
    if let Some(detail) = &node.detail {
        ui.painter().text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            detail,
            FontId::proportional(TYPE.meta),
            if is_selected {
                colors.text
            } else {
                colors.faint
            },
        );
    }
    if disclosure.is_some_and(|response| response.clicked()) {
        if expanded {
            expanded_nodes.remove(&node.id);
        } else {
            expanded_nodes.insert(node.id.clone());
        }
    } else if response.clicked() {
        *selected = node.id.clone();
        if has_children && response.double_clicked() {
            if expanded {
                expanded_nodes.remove(&node.id);
            } else {
                expanded_nodes.insert(node.id.clone());
            }
        }
    }
    if expanded {
        for child in &node.children {
            show_scene_node(ui, child, depth + 1, expanded_nodes, selected);
        }
    }
}

pub(crate) fn asset_tile(
    ui: &mut egui::Ui,
    name: &str,
    asset_icon: Icon,
    kind: &'static str,
    selected: bool,
) -> egui::Response {
    let colors = palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(76.0, 62.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, name)
    });
    let border = if selected {
        colors.asset_selection_stroke
    } else if response.hovered() {
        colors.border_strong
    } else {
        Color32::TRANSPARENT
    };
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if selected {
                colors.asset_selection
            } else {
                colors.panel_raised
            },
        );
    }
    let preview = Rect::from_min_max(
        rect.min + egui::vec2(6.0, 4.0),
        egui::pos2(rect.max.x - 6.0, rect.max.y - 17.0),
    );
    ui.painter().rect_filled(preview, UI.radius, colors.surface);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(preview.center(), Vec2::splat(22.0)),
        asset_icon,
        colors.muted,
    );
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            egui::pos2(rect.min.x + 4.0, rect.max.y - 16.0),
            egui::pos2(rect.max.x - 4.0, rect.max.y),
        ))
        .text(
            egui::pos2(rect.center().x, rect.max.y - 8.0),
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(TYPE.meta),
            if selected { colors.text } else { colors.muted },
        );
    ui.painter().rect_stroke(
        rect,
        UI.radius,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    paint_focus(ui, &response);
    response.on_hover_text(format!("{name} · {} preview", kind.to_lowercase()))
}

pub(crate) fn compact_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let colors = palette(ui);
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(TYPE.secondary),
        if selected { colors.text } else { colors.muted },
    );
    let width = galley.size().x.ceil() + 12.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, UI.radius, colors.panel_raised);
    }
    if selected {
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 1.0,
            Stroke::new(1.0, colors.selection),
        );
    }
    ui.painter().galley(
        rect.center() - galley.size() * 0.5,
        galley,
        if selected { colors.text } else { colors.muted },
    );
    paint_focus(ui, &response);
    response
}

pub(crate) fn navigation_row(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    active: bool,
) -> egui::Response {
    let colors = palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), UI.row), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if selected {
                colors.selection
            } else {
                colors.panel_raised
            },
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(11.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        if selected {
            colors.accent
        } else {
            colors.muted
        },
    );
    let label_font = if selected {
        medium_font(TYPE.primary)
    } else {
        FontId::proportional(TYPE.primary)
    };
    ui.painter().text(
        rect.left_center() + egui::vec2(24.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        label_font,
        if selected { colors.text } else { colors.muted },
    );
    if active {
        ui.painter().text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            "ON",
            FontId::new(TYPE.meta, FontFamily::Name(MEDIUM_FONT_FAMILY.into())),
            colors.live,
        );
    }
    paint_focus(ui, &response);
    response
}

pub(crate) fn search_field(ui: &mut egui::Ui, query: &mut String, width: f32) {
    let colors = palette(ui);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.min(ui.available_width()).max(80.0), CONTROL_HEIGHT),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, UI.radius, colors.surface_deep);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(11.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        Icon::Search,
        colors.faint,
    );
    let text_rect = Rect::from_min_max(
        rect.min + egui::vec2(23.0, 1.0),
        rect.max - egui::vec2(4.0, 1.0),
    );
    let response = ui.put(
        text_rect,
        egui::TextEdit::singleline(query)
            .hint_text("Search assets…")
            .font(FontId::proportional(TYPE.secondary))
            .margin(Margin::ZERO)
            .frame(Frame::NONE),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Search assets")
    });
}

pub(crate) fn drop_target(ui: &mut egui::Ui, label: &str) {
    let colors = palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 52.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        UI.radius,
        if response.hovered() {
            colors.panel_raised
        } else {
            colors.surface
        },
    );
    ui.painter().rect_stroke(
        rect,
        UI.radius,
        Stroke::new(
            1.0,
            if response.hovered() {
                colors.accent
            } else {
                colors.border
            },
        ),
        StrokeKind::Inside,
    );
    let icon_rect =
        Rect::from_center_size(rect.center() - egui::vec2(0.0, 7.0), Vec2::splat(UI.icon));
    paint_icon(ui.painter(), icon_rect, Icon::Open, colors.muted);
    ui.painter().text(
        rect.center() + egui::vec2(0.0, 10.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(TYPE.secondary),
        colors.muted,
    );
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn menu_entry(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> egui::Response {
    let colors = palette(ui);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(220.0, 24.0), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if enabled && (response.hovered() || response.has_focus()) {
        ui.painter().rect_filled(rect, UI.radius, colors.selection);
    }
    let color = if enabled { colors.text } else { colors.faint };
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(13.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        if enabled { colors.muted } else { colors.faint },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(28.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(TYPE.primary),
        color,
    );
    if !shortcut.is_empty() {
        ui.painter().text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::proportional(TYPE.meta),
            colors.faint,
        );
    }
    response
}

pub(crate) fn toolbar_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    active: bool,
) -> egui::Response {
    let colors = palette(ui);
    let width = toolbar_button_width(ui, label);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if active {
                colors.selection
            } else {
                colors.panel_raised
            },
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(12.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        if active { colors.accent } else { colors.text },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(22.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        medium_font(TYPE.secondary),
        colors.text,
    );
    paint_focus(ui, &response);
    response
}

pub(crate) fn toolbar_status(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    color: Color32,
) -> egui::Response {
    let width = toolbar_button_width(ui, label);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, CONTROL_HEIGHT), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(12.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        color,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(22.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        medium_font(TYPE.secondary),
        color,
    );
    response
}

pub(crate) fn chatgpt_account_label(account: &ChatGptAccount) -> String {
    let plan = match account.plan_type.as_deref() {
        Some("free") => Some("Free"),
        Some("go") => Some("Go"),
        Some("plus") => Some("Plus"),
        Some("pro" | "prolite") => Some("Pro"),
        Some("team" | "self_serve_business_prolite" | "self_serve_business_usage_based") => {
            Some("Business")
        }
        Some("business") => Some("Business"),
        Some("ent26" | "enterprise_cbp_automation" | "enterprise_cbp_usage_based") => {
            Some("Enterprise")
        }
        Some("enterprise") => Some("Enterprise"),
        Some("edu" | "edu_plus" | "edu_pro") => Some("Edu"),
        _ => None,
    };
    plan.map_or_else(|| "ChatGPT".to_owned(), |plan| format!("ChatGPT · {plan}"))
}

pub(crate) fn codex_chat_model_label(model: &str, selected_model: &str) -> String {
    let marker = match (model == CODEX_CHAT_DEFAULT_MODEL, model == selected_model) {
        (true, true) => " (default · current)",
        (true, false) => " (default)",
        (false, true) => " (current)",
        (false, false) => "",
    };
    format!("{model}{marker}")
}

pub(crate) fn codex_chat_model_description(model: &str) -> &'static str {
    CODEX_CHAT_MODELS
        .iter()
        .find_map(|(candidate, description)| (*candidate == model).then_some(*description))
        .unwrap_or("Select a Codex model.")
}

pub(crate) fn toolbar_button_width(ui: &egui::Ui, label: &str) -> f32 {
    let label_width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            medium_font(TYPE.secondary),
            Color32::WHITE,
        )
        .size()
        .x;
    (label_width + 30.0).ceil().max(44.0)
}

pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    tooltip: &str,
    active: bool,
) -> egui::Response {
    let colors = palette(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CONTROL_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if active {
                colors.field
            } else {
                colors.panel_raised
            },
        );
    }
    paint_icon(
        ui.painter(),
        rect.shrink(3.0),
        icon,
        if active { colors.text } else { colors.muted },
    );
    paint_focus(ui, &response);
    response.on_hover_text(tooltip)
}

pub(crate) fn paint_focus(ui: &egui::Ui, response: &egui::Response) {
    let colors = palette(ui);
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            UI.radius,
            Stroke::new(1.0, colors.accent),
            StrokeKind::Inside,
        );
    }
}

pub(crate) fn inline_icon(ui: &mut egui::Ui, icon: Icon, color: Color32) {
    let response = ui.allocate_response(Vec2::splat(UI.icon), Sense::hover());
    paint_icon(ui.painter(), response.rect, icon, color);
}

pub(crate) fn paint_status_label(ui: &egui::Ui, rect: Rect, color: Color32, label: &str) {
    ui.painter()
        .circle_filled(rect.left_center() + egui::vec2(9.0, 0.0), 3.0, color);
    ui.painter().text(
        rect.left_center() + egui::vec2(18.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(TYPE.meta),
        color,
    );
}

pub(crate) fn vertical_separator(ui: &mut egui::Ui, height: f32) {
    let colors = palette(ui);
    let response = ui.allocate_response(egui::vec2(1.0, height), Sense::hover());
    ui.painter().line_segment(
        [response.rect.center_top(), response.rect.center_bottom()],
        Stroke::new(1.0, colors.border),
    );
}

pub(crate) fn paint_down_chevron(ui: &mut egui::Ui) {
    let colors = palette(ui);
    let response = ui.allocate_response(Vec2::splat(UI.icon), Sense::hover());
    paint_icon(ui.painter(), response.rect, Icon::ChevronDown, colors.faint);
}

pub(crate) fn tool_icon(tool: &str) -> Icon {
    match tool {
        "Sessions" => Icon::Test,
        "State" => Icon::Sliders,
        "Network" => Icon::Network,
        "Logs" => Icon::Logs,
        "Performance" => Icon::Gauge,
        _ => Icon::Test,
    }
}

pub(crate) fn asset_kind(name: &str) -> (Icon, &'static str) {
    if name.contains("grass") || name.contains("wood") {
        (Icon::Material, "MATERIAL")
    } else if name == "campfire" || name == "tree" || name == "castle" {
        (Icon::Object, "MODEL")
    } else {
        (Icon::Image, "IMAGE")
    }
}

pub(crate) fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

pub(crate) fn source_lod_status(summary: &MorphGlbSourceSummary, level: &str) -> String {
    match summary.lod_candidates.get(level) {
        Some(candidates) if candidates.len() == 1 => format!("Mapped: {}", candidates[0]),
        Some(candidates) if candidates.is_empty() => "Missing mapping".to_owned(),
        Some(candidates) => format!("Ambiguous: {} candidates", candidates.len()),
        None => "Missing mapping".to_owned(),
    }
}

pub(crate) fn morph_kind_label(kind: MorphAssetKind) -> &'static str {
    match kind {
        MorphAssetKind::Base => "Bases",
        MorphAssetKind::Face => "Faces",
        MorphAssetKind::Hair => "Hair",
        MorphAssetKind::Outfit => "Outfits",
        MorphAssetKind::Top => "Tops",
        MorphAssetKind::Outerwear => "Outerwear",
        MorphAssetKind::Bottom => "Bottoms",
        MorphAssetKind::OnePiece => "One-piece",
        MorphAssetKind::Footwear => "Footwear",
        MorphAssetKind::Headwear => "Headwear",
        MorphAssetKind::Facewear => "Facewear",
        MorphAssetKind::Accessory => "Accessories",
        MorphAssetKind::Tail => "Tails",
        MorphAssetKind::Wings => "Wings",
        MorphAssetKind::Horns => "Horns",
        MorphAssetKind::Ears => "Ears",
        MorphAssetKind::HeldItem => "Held items",
    }
}

pub(crate) fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    #[cfg(target_os = "macos")]
    if paint_system_icon(painter, rect, icon, color) {
        return;
    }

    let c = rect.center();
    let size = rect.width().min(rect.height()).max(1.0);
    let r = size * 0.42;
    let stroke = Stroke::new((size * 0.09).clamp(1.0, 1.5), color);
    let left = c.x - r;
    let right = c.x + r;
    let top = c.y - r;
    let bottom = c.y + r;
    match icon {
        Icon::Object => {
            let top_point = egui::pos2(c.x, top);
            let left_point = egui::pos2(left, c.y - r * 0.5);
            let right_point = egui::pos2(right, c.y - r * 0.5);
            let bottom_point = egui::pos2(c.x, bottom);
            painter.add(egui::Shape::closed_line(
                vec![
                    top_point,
                    right_point,
                    egui::pos2(right, c.y + r * 0.5),
                    bottom_point,
                    egui::pos2(left, c.y + r * 0.5),
                    left_point,
                ],
                stroke,
            ));
            painter.line_segment([c, bottom_point], stroke);
            painter.line_segment([left_point, c], stroke);
            painter.line_segment([right_point, c], stroke);
        }
        Icon::World => {
            painter.circle_stroke(c, r, stroke);
            painter.line_segment([egui::pos2(left, c.y), egui::pos2(right, c.y)], stroke);
            painter.line_segment([egui::pos2(c.x, top), egui::pos2(c.x, bottom)], stroke);
            painter.circle_stroke(c, r * 0.52, Stroke::new(stroke.width * 0.75, color));
        }
        Icon::Assets | Icon::Grid | Icon::Test => {
            let cell = r * 0.72;
            for offset in [
                egui::vec2(-cell, -cell),
                egui::vec2(cell, -cell),
                egui::vec2(-cell, cell),
                egui::vec2(cell, cell),
            ] {
                painter.rect_stroke(
                    Rect::from_center_size(c + offset * 0.48, Vec2::splat(cell * 0.78)),
                    1.0,
                    stroke,
                    StrokeKind::Inside,
                );
            }
        }
        Icon::Material => {
            painter.circle_stroke(c, r, stroke);
            painter.circle_filled(c - egui::vec2(r * 0.22, r * 0.22), r * 0.22, color);
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.8, c.y + r * 0.5),
                    egui::pos2(c.x + r * 0.65, c.y - r * 0.55),
                ],
                stroke,
            );
        }
        Icon::Folder | Icon::Open => {
            let points = [
                egui::pos2(left, top + r * 0.25),
                egui::pos2(c.x - r * 0.2, top + r * 0.25),
                egui::pos2(c.x, top + r * 0.55),
                egui::pos2(right, top + r * 0.55),
                egui::pos2(right, bottom),
                egui::pos2(left, bottom),
                egui::pos2(left, top + r * 0.25),
            ];
            painter.add(egui::Shape::line(points.to_vec(), stroke));
            if matches!(icon, Icon::Open) {
                painter.line_segment(
                    [egui::pos2(c.x, c.y), egui::pos2(c.x, bottom + r * 0.18)],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(c.x - r * 0.25, bottom - r * 0.05),
                        egui::pos2(c.x, bottom + r * 0.18),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(c.x + r * 0.25, bottom - r * 0.05),
                        egui::pos2(c.x, bottom + r * 0.18),
                    ],
                    stroke,
                );
            }
        }
        Icon::Image => {
            painter.rect_stroke(rect.shrink(size * 0.08), 1.0, stroke, StrokeKind::Inside);
            painter.circle_filled(
                egui::pos2(right - r * 0.25, top + r * 0.28),
                r * 0.13,
                color,
            );
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left + r * 0.2, bottom - r * 0.2),
                    egui::pos2(c.x - r * 0.15, c.y),
                    egui::pos2(c.x + r * 0.12, c.y + r * 0.25),
                    egui::pos2(right - r * 0.1, c.y - r * 0.15),
                ],
                stroke,
            ));
        }
        Icon::Character => {
            painter.circle_stroke(egui::pos2(c.x, top + r * 0.32), r * 0.28, stroke);
            painter.line_segment(
                [egui::pos2(c.x, c.y), egui::pos2(c.x, bottom - r * 0.1)],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(left + r * 0.18, c.y + r * 0.2),
                    egui::pos2(right - r * 0.18, c.y + r * 0.2),
                ],
                stroke,
            );
        }
        Icon::Sparkles => {
            let large = c - egui::vec2(r * 0.2, r * 0.12);
            painter.line_segment(
                [
                    egui::pos2(large.x, top),
                    egui::pos2(large.x, bottom - r * 0.08),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(left + r * 0.08, large.y),
                    egui::pos2(right - r * 0.35, large.y),
                ],
                stroke,
            );
            let small = c + egui::vec2(r * 0.55, r * 0.5);
            painter.line_segment(
                [
                    small - egui::vec2(0.0, r * 0.26),
                    small + egui::vec2(0.0, r * 0.26),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    small - egui::vec2(r * 0.26, 0.0),
                    small + egui::vec2(r * 0.26, 0.0),
                ],
                stroke,
            );
        }
        Icon::Search => {
            painter.circle_stroke(c - egui::vec2(r * 0.15, r * 0.15), r * 0.56, stroke);
            painter.line_segment(
                [
                    c + egui::vec2(r * 0.25, r * 0.25),
                    egui::pos2(right, bottom),
                ],
                stroke,
            );
        }
        Icon::Filter => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left, top),
                    egui::pos2(right, top),
                    egui::pos2(c.x + r * 0.18, c.y),
                    egui::pos2(c.x + r * 0.18, bottom),
                    egui::pos2(c.x - r * 0.18, bottom - r * 0.2),
                    egui::pos2(c.x - r * 0.18, c.y),
                ],
                stroke,
            ));
        }
        Icon::Camera => {
            painter.rect_stroke(
                Rect::from_min_max(egui::pos2(left, top + r * 0.22), egui::pos2(right, bottom)),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.circle_stroke(c + egui::vec2(0.0, r * 0.1), r * 0.28, stroke);
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.45, top + r * 0.22),
                    egui::pos2(c.x - r * 0.2, top),
                ],
                stroke,
            );
        }
        Icon::Sliders => {
            for (index, y) in [-0.55_f32, 0.0, 0.55].into_iter().enumerate() {
                let y = c.y + r * y;
                painter.line_segment([egui::pos2(left, y), egui::pos2(right, y)], stroke);
                let knob = if index == 1 {
                    c.x - r * 0.35
                } else {
                    c.x + r * 0.28
                };
                painter.circle_filled(egui::pos2(knob, y), r * 0.13, color);
            }
        }
        Icon::More => {
            for x in [-0.55_f32, 0.0, 0.55] {
                painter.circle_filled(egui::pos2(c.x + r * x, c.y), r * 0.13, color);
            }
        }
        Icon::Plus => {
            painter.line_segment([egui::pos2(left, c.y), egui::pos2(right, c.y)], stroke);
            painter.line_segment([egui::pos2(c.x, top), egui::pos2(c.x, bottom)], stroke);
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(left + r * 0.25, top),
                    egui::pos2(right, c.y),
                    egui::pos2(left + r * 0.25, bottom),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(r * 1.35)), 1.0, color);
        }
        Icon::Check => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left, c.y),
                    egui::pos2(c.x - r * 0.15, bottom - r * 0.1),
                    egui::pos2(right, top + r * 0.08),
                ],
                stroke,
            ));
        }
        Icon::ChevronDown => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left, c.y - r * 0.2),
                    egui::pos2(c.x, c.y + r * 0.3),
                    egui::pos2(right, c.y - r * 0.2),
                ],
                stroke,
            ));
        }
        Icon::ChevronRight => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - r * 0.2, top),
                    egui::pos2(c.x + r * 0.3, c.y),
                    egui::pos2(c.x - r * 0.2, bottom),
                ],
                stroke,
            ));
        }
        Icon::Eye => {
            for direction in [-1.0, 1.0] {
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                    [
                        egui::pos2(left, c.y),
                        egui::pos2(c.x - r * 0.4, c.y + direction * r * 0.9),
                        egui::pos2(c.x + r * 0.4, c.y + direction * r * 0.9),
                        egui::pos2(right, c.y),
                    ],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ));
            }
            painter.circle_stroke(c, r * 0.27, stroke);
        }
        Icon::Lock => {
            painter.rect_stroke(
                Rect::from_min_max(egui::pos2(left, c.y - r * 0.05), egui::pos2(right, bottom)),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - r * 0.5, c.y - r * 0.05),
                    egui::pos2(c.x - r * 0.5, top + r * 0.35),
                    egui::pos2(c.x + r * 0.5, top + r * 0.35),
                    egui::pos2(c.x + r * 0.5, c.y - r * 0.05),
                ],
                stroke,
            ));
        }
        #[cfg(not(target_os = "macos"))]
        Icon::Save => {
            painter.rect_stroke(rect.shrink(size * 0.08), 1.0, stroke, StrokeKind::Inside);
            painter.rect_stroke(
                Rect::from_min_max(
                    egui::pos2(c.x - r * 0.45, top + r * 0.12),
                    egui::pos2(c.x + r * 0.28, c.y - r * 0.05),
                ),
                0.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_max(
                    egui::pos2(c.x - r * 0.45, c.y + r * 0.25),
                    egui::pos2(c.x + r * 0.45, bottom - r * 0.1),
                ),
                0.0,
                stroke,
                StrokeKind::Inside,
            );
        }
        #[cfg(not(target_os = "macos"))]
        Icon::Undo | Icon::Redo => {
            let direction = if matches!(icon, Icon::Redo) {
                -1.0
            } else {
                1.0
            };
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x + direction * r, bottom - r * 0.1),
                    egui::pos2(c.x + direction * r * 0.85, top + r * 0.35),
                    egui::pos2(c.x - direction * r * 0.35, top + r * 0.2),
                    egui::pos2(c.x - direction * r, c.y),
                ],
                stroke,
            ));
            painter.line_segment(
                [
                    egui::pos2(c.x - direction * r, c.y),
                    egui::pos2(c.x - direction * r * 0.58, c.y - r * 0.42),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - direction * r, c.y),
                    egui::pos2(c.x - direction * r * 0.58, c.y + r * 0.42),
                ],
                stroke,
            );
        }
        #[cfg(not(target_os = "macos"))]
        Icon::Settings => {
            painter.circle_stroke(c, r * 0.42, stroke);
            painter.circle_stroke(c, r * 0.14, stroke);
            for direction in [
                egui::vec2(1.0, 0.0),
                egui::vec2(-1.0, 0.0),
                egui::vec2(0.0, 1.0),
                egui::vec2(0.0, -1.0),
            ] {
                painter.line_segment([c + direction * r * 0.5, c + direction * r], stroke);
            }
        }
        Icon::Network => {
            let nodes = [
                egui::pos2(c.x, top),
                egui::pos2(left, bottom),
                egui::pos2(right, bottom),
            ];
            painter.line_segment([nodes[0], nodes[1]], stroke);
            painter.line_segment([nodes[0], nodes[2]], stroke);
            painter.line_segment([nodes[1], nodes[2]], stroke);
            for node in nodes {
                painter.circle_filled(node, r * 0.16, color);
            }
        }
        Icon::Logs => {
            for y in [-0.55_f32, 0.0, 0.55] {
                let y = c.y + y * r;
                painter.circle_filled(egui::pos2(left, y), r * 0.09, color);
                painter.line_segment(
                    [egui::pos2(left + r * 0.3, y), egui::pos2(right, y)],
                    stroke,
                );
            }
        }
        Icon::Gauge => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(left, bottom),
                    egui::pos2(left + r * 0.15, c.y),
                    egui::pos2(c.x - r * 0.15, c.y + r * 0.2),
                    egui::pos2(c.x + r * 0.12, top + r * 0.25),
                    egui::pos2(right, top),
                ],
                stroke,
            ));
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn paint_system_icon(
    painter: &egui::Painter,
    rect: Rect,
    icon: Icon,
    color: Color32,
) -> bool {
    let atlas = painter
        .ctx()
        .data(|data| data.get_temp::<SystemIconAtlas>(egui::Id::new(SYSTEM_ICON_ATLAS_ID)));
    let Some(texture) = atlas.as_ref().and_then(|atlas| atlas.get(&icon)) else {
        return false;
    };
    let texture_size = texture.size_vec2();
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
        return false;
    }
    let scale = (rect.width() / texture_size.x).min(rect.height() / texture_size.y) * 0.94;
    let symbol_rect = Rect::from_center_size(rect.center(), texture_size * scale);
    painter.image(
        texture.id(),
        symbol_rect,
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        color,
    );
    true
}
