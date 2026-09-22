use super::*;
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
    search_field_with_hint(ui, query, width, "Search assets…", "Search assets");
}

pub(crate) fn search_field_with_hint(
    ui: &mut egui::Ui,
    query: &mut String,
    width: f32,
    hint: &str,
    accessible_label: &str,
) {
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
            .hint_text(hint)
            .font(FontId::proportional(TYPE.secondary))
            .margin(Margin::ZERO)
            .frame(Frame::NONE),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, accessible_label)
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
