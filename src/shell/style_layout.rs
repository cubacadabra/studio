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

#[derive(Clone)]
pub(crate) struct SceneTreeRow {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) icon: Icon,
    pub(crate) detail: Option<String>,
    pub(crate) depth: usize,
    pub(crate) has_children: bool,
    pub(crate) authoring: bool,
    pub(crate) placeable: bool,
    pub(crate) has_parent: bool,
    pub(crate) group: bool,
    pub(crate) locked: bool,
    pub(crate) roblox_linked: bool,
    pub(crate) ancestors: Vec<String>,
}

pub(crate) fn flatten_scene_rows(
    node: &SceneNode,
    depth: usize,
    expanded_nodes: &BTreeSet<String>,
    filter_matches: Option<&BTreeSet<String>>,
    rows: &mut Vec<SceneTreeRow>,
) {
    flatten_scene_rows_with_ancestors(
        node,
        depth,
        expanded_nodes,
        filter_matches,
        rows,
        &mut Vec::new(),
    );
}

fn flatten_scene_rows_with_ancestors(
    node: &SceneNode,
    depth: usize,
    expanded_nodes: &BTreeSet<String>,
    filter_matches: Option<&BTreeSet<String>>,
    rows: &mut Vec<SceneTreeRow>,
    ancestors: &mut Vec<String>,
) {
    if filter_matches.is_some_and(|matches| !matches.contains(&node.id)) {
        return;
    }
    rows.push(SceneTreeRow {
        id: node.id.clone(),
        label: node.label.clone(),
        icon: node.icon,
        detail: node.detail.clone(),
        depth,
        has_children: !node.children.is_empty(),
        authoring: is_authoring_node(node),
        placeable: is_authoring_placeable(node),
        has_parent: scene_parent_id_for_row(node).is_some(),
        group: node.kind == "Group" && is_authoring_transformable(node),
        locked: scene_node_locked(node),
        roblox_linked: scene_node_roblox_linked(node),
        ancestors: ancestors.clone(),
    });
    if expanded_nodes.contains(&node.id) || filter_matches.is_some() {
        ancestors.push(node.id.clone());
        for child in &node.children {
            flatten_scene_rows_with_ancestors(
                child,
                depth + 1,
                expanded_nodes,
                filter_matches,
                rows,
                ancestors,
            );
        }
        ancestors.pop();
    }
}

pub(crate) fn show_scene_row(
    ui: &mut egui::Ui,
    row: &SceneTreeRow,
    expanded_nodes: &mut BTreeSet<String>,
    primary_selection: &mut String,
    selected: &mut BTreeSet<String>,
    editable: bool,
    edit_request: &mut Option<SceneEditRequest>,
    add_palette: &mut Option<AddPaletteState>,
) {
    let expanded = expanded_nodes.contains(&row.id);
    let has_children = row.has_children;
    let colors = palette(ui);
    let sense = if editable && row.authoring && row.has_parent {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), UI.row), sense);
    let is_selected = selected.contains(&row.id);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            is_selected,
            &row.label,
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
    let drag_targets = if is_selected && selected.len() > 1 {
        selected.iter().cloned().collect::<Vec<_>>()
    } else {
        vec![row.id.clone()]
    };
    if editable && row.authoring && row.has_parent {
        response.dnd_set_drag_payload(SceneTreeDragPayload {
            targets: drag_targets,
        });
    }
    let valid_drop_target = row.group
        && response
            .dnd_hover_payload::<SceneTreeDragPayload>()
            .is_some_and(|payload| {
                !payload.targets.iter().any(|target| {
                    target == &row.id || row.ancestors.iter().any(|ancestor| ancestor == target)
                })
            });
    if valid_drop_target {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            2.0,
            Stroke::new(2.0, colors.accent),
            StrokeKind::Inside,
        );
    }
    if valid_drop_target
        && let Some(payload) = response.dnd_release_payload::<SceneTreeDragPayload>()
    {
        *edit_request = Some(SceneEditRequest::ReparentObjects {
            targets: payload.targets.clone(),
            parent_id: row.id.clone(),
        });
    }
    paint_focus(ui, &response);
    let x = rect.min.x + UI.inset + row.depth as f32 * 12.0;
    let disclosure_rect =
        Rect::from_center_size(egui::pos2(x + 5.0, rect.center().y), Vec2::splat(14.0));
    let disclosure = has_children.then(|| {
        ui.interact(
            disclosure_rect,
            ui.id().with(("scene-disclosure", &row.id)),
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
        row.icon,
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
    let detail_width = row.detail.as_ref().map_or(0.0, |detail| {
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
    let mut label_job = LayoutJob::simple_singleline(row.label.clone(), label_font, label_color);
    label_job.wrap = TextWrapping::truncate_at_width((label_right - label_left).max(0.0));
    let label_galley = ui.painter().layout_job(label_job);
    ui.painter().galley(
        egui::pos2(label_left, rect.center().y - label_galley.size().y * 0.5),
        label_galley,
        label_color,
    );
    if let Some(detail) = &row.detail {
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
            expanded_nodes.remove(&row.id);
        } else {
            expanded_nodes.insert(row.id.clone());
        }
    } else if response.clicked() {
        let additive = ui.input(|input| input.modifiers.shift || input.modifiers.command);
        if additive {
            if !selected.remove(&row.id) {
                selected.insert(row.id.clone());
                *primary_selection = row.id.clone();
            } else if *primary_selection == row.id {
                *primary_selection = selected
                    .iter()
                    .next_back()
                    .cloned()
                    .unwrap_or_else(|| row.id.clone());
            }
            if selected.is_empty() {
                selected.insert(row.id.clone());
                *primary_selection = row.id.clone();
            }
        } else {
            selected.clear();
            selected.insert(row.id.clone());
            *primary_selection = row.id.clone();
        }
        if has_children && response.double_clicked() {
            if expanded {
                expanded_nodes.remove(&row.id);
            } else {
                expanded_nodes.insert(row.id.clone());
            }
        }
    }
    response.clone().context_menu(|ui| {
        if !editable {
            ui.label(RichText::new("Editing unavailable").color(palette(ui).muted));
            return;
        }
        if ui.button("Add…").clicked() {
            *add_palette = Some(AddPaletteState::new(
                scene_world_id(&row.id).map(str::to_owned),
            ));
            ui.close();
        }
        if is_scene_object(&row.id) || (row.authoring && row.has_parent) {
            if (is_scene_object(&row.id) || row.placeable || row.group)
                && ui.button("Duplicate").clicked()
            {
                *edit_request = Some(SceneEditRequest::DuplicateObject {
                    target: row.id.clone(),
                });
                ui.close();
            }
            let can_delete = !row.roblox_linked && !row.locked;
            if ui
                .add_enabled(can_delete, egui::Button::new("Delete"))
                .on_hover_text(if can_delete {
                    "Delete scene object"
                } else {
                    "Deleting imported Roblox objects is not supported yet"
                })
                .clicked()
            {
                *edit_request = Some(SceneEditRequest::DeleteObject {
                    target: row.id.clone(),
                });
                ui.close();
            }
        }
    });
}

fn scene_parent_id_for_row(node: &SceneNode) -> Option<&str> {
    node.properties
        .iter()
        .find(|(label, value)| label == "Parent" && value != "—")
        .map(|(_, value)| value.as_str())
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
