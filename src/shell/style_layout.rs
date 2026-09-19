use super::*;
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
    editable: bool,
    edit_request: &mut Option<SceneEditRequest>,
    filter_matches: Option<&BTreeSet<String>>,
) {
    if filter_matches.is_some_and(|matches| !matches.contains(&node.id)) {
        return;
    }
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
            .min(rect.width() * 0.38)
            + 12.0
    });
    let label_left = x + 28.0;
    let label_right = (rect.max.x - detail_width).max(label_left);
    let label_color = if is_selected {
        colors.text
    } else {
        colors.secondary_text
    };
    let mut label_job = LayoutJob::simple_singleline(node.label.clone(), label_font, label_color);
    label_job.wrap = TextWrapping::truncate_at_width((label_right - label_left).max(0.0));
    let label_galley = ui.painter().layout_job(label_job);
    ui.painter().galley(
        egui::pos2(label_left, rect.center().y - label_galley.size().y * 0.5),
        label_galley,
        label_color,
    );
    if let Some(detail) = &node.detail {
        let detail_color = if is_selected {
            colors.text
        } else {
            colors.faint
        };
        let detail_max_width = (detail_width - 12.0).max(0.0);
        let mut detail_job = LayoutJob::simple_singleline(
            detail.clone(),
            FontId::proportional(TYPE.meta),
            detail_color,
        );
        detail_job.wrap = TextWrapping::truncate_at_width(detail_max_width);
        let detail_galley = ui.painter().layout_job(detail_job);
        ui.painter().galley(
            egui::pos2(
                rect.max.x - 8.0 - detail_galley.size().x,
                rect.center().y - detail_galley.size().y * 0.5,
            ),
            detail_galley,
            detail_color,
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
    response.clone().context_menu(|ui| {
        if !editable {
            ui.label(RichText::new("Read-only preview").color(palette(ui).muted));
            return;
        }
        if let Some(world_id) = scene_world_id(&node.id).map(str::to_owned) {
            ui.menu_button("Add", |ui| {
                for kind in SceneObjectKind::ALL {
                    if ui.button(kind.label()).clicked() {
                        *edit_request = Some(SceneEditRequest::AddObject {
                            world_id: Some(world_id.clone()),
                            kind,
                        });
                        ui.close();
                    }
                }
            });
        }
        if is_scene_object(&node.id) {
            if ui.button("Duplicate").clicked() {
                *edit_request = Some(SceneEditRequest::DuplicateObject {
                    target: node.id.clone(),
                });
                ui.close();
            }
            if ui.button("Delete").clicked() {
                *edit_request = Some(SceneEditRequest::DeleteObject {
                    target: node.id.clone(),
                });
                ui.close();
            }
        }
    });
    if expanded || filter_matches.is_some() {
        for child in &node.children {
            show_scene_node(
                ui,
                child,
                depth + 1,
                expanded_nodes,
                selected,
                editable,
                edit_request,
                filter_matches,
            );
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
