use super::*;
impl StudioShell {
    pub(crate) fn show_test(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::bottom("test_tools")
            .resizable(true)
            .default_size(112.0)
            .size_range(80.0..=260.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Test, colors.muted);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for tool in ["Sessions", "State", "Network", "Logs", "Performance"] {
                        if compact_tab(ui, tool, self.test_tool == tool).clicked() {
                            self.test_tool = tool;
                            self.notice = format!("{tool} inspector preview");
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, Icon::More, "Test options", false);
                    });
                });
                content_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        inline_icon(ui, tool_icon(self.test_tool), colors.faint);
                        ui.label(
                            RichText::new(format!("{} tools will appear here.", self.test_tool))
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                let gap = 1.0;
                let half_width = (available.width() - gap) * 0.5;
                let half_height = (available.height() - gap) * 0.5;
                let rects = [
                    Rect::from_min_size(available.min, Vec2::new(half_width, half_height)),
                    Rect::from_min_size(
                        egui::pos2(available.min.x + half_width + gap, available.min.y),
                        Vec2::new(half_width, half_height),
                    ),
                    Rect::from_min_size(
                        egui::pos2(available.min.x, available.min.y + half_height + gap),
                        Vec2::new(half_width, half_height),
                    ),
                    Rect::from_min_size(
                        egui::pos2(
                            available.min.x + half_width + gap,
                            available.min.y + half_height + gap,
                        ),
                        Vec2::new(half_width, half_height),
                    ),
                ];
                for (index, rect) in rects.into_iter().enumerate() {
                    let header = Rect::from_min_size(
                        rect.min,
                        Vec2::new(rect.width(), EDITOR_HEADER_HEIGHT),
                    );
                    if index > 0 {
                        ui.painter().rect_filled(rect, 0.0, colors.surface);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Session preview",
                            FontId::proportional(TYPE.secondary),
                            colors.muted,
                        );
                    }
                    ui.painter().rect_filled(header, 0.0, colors.panel_header);
                    let icon_rect = Rect::from_center_size(
                        header.left_center() + egui::vec2(14.0, 0.0),
                        Vec2::splat(UI.icon),
                    );
                    paint_icon(ui.painter(), icon_rect, Icon::Camera, colors.faint);
                    ui.painter().text(
                        header.left_center() + egui::vec2(27.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("Player {}", index + 1),
                        medium_font(TYPE.secondary),
                        colors.text,
                    );
                    let status_center = header.right_center() - egui::vec2(13.0, 0.0);
                    ui.painter().circle_filled(
                        status_center,
                        3.0,
                        if index == 0 {
                            colors.accent
                        } else {
                            colors.faint
                        },
                    );
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(1.0, colors.border),
                        StrokeKind::Inside,
                    );
                    if index == 0 {
                        self.runtime_viewport = Rect::from_min_max(
                            egui::pos2(rect.min.x + 1.0, header.max.y),
                            egui::pos2(rect.max.x - 1.0, rect.max.y - 1.0),
                        );
                    }
                }
            });
    }

    pub(crate) fn viewport_panel(&mut self, root: &mut egui::Ui, mode: &str, title: &str) {
        let colors = palette(root);
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                let selected_projection = self
                    .scene_object_projections
                    .iter()
                    .find(|projection| projection.id == self.selected_scene);
                let selected_is_placeable = selected_projection.is_some();
                let selected_can_resize = selected_projection
                    .is_some_and(|item| item.size.is_some() && item.screen_corners.is_some());
                if selected_is_placeable
                    && !selected_can_resize
                    && self.scene_viewport_tool == SceneViewportTool::Resize
                {
                    self.scene_viewport_tool = SceneViewportTool::Move;
                }
                let header = editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Camera, colors.muted);
                    ui.label(
                        RichText::new(title)
                            .font(semibold_font(TYPE.primary))
                            .color(colors.text),
                    );
                    vertical_separator(ui, 12.0);
                    ui.label(
                        RichText::new(mode)
                            .size(TYPE.secondary)
                            .color(colors.secondary_text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for preset in ReviewCameraPreset::ALL.into_iter().rev() {
                            if ui
                                .selectable_label(self.review_camera == preset, preset.label())
                                .on_hover_text(match preset {
                                    ReviewCameraPreset::Gameplay => "Player camera: drag to orbit, scroll to zoom",
                                    _ => "Drag to orbit · right/middle drag to pan · scroll/pinch to zoom · click preset again to frame the world",
                                })
                                .clicked()
                            {
                                self.set_review_camera(preset);
                            }
                        }
                        if self.project_editable && !self.playing && selected_is_placeable {
                            ui.add_space(8.0);
                            if selected_can_resize {
                                if ui
                                    .selectable_label(
                                        self.scene_viewport_tool == SceneViewportTool::Resize,
                                        "Resize",
                                    )
                                    .on_hover_text("Resize the selected object on the X/Z plane")
                                    .clicked()
                                {
                                    self.scene_viewport_tool = SceneViewportTool::Resize;
                                }
                            }
                            if ui
                                .selectable_label(
                                    self.scene_viewport_tool == SceneViewportTool::Move,
                                    "Move",
                                )
                                .on_hover_text("Move the selected object on the X/Z plane")
                                .clicked()
                            {
                                self.scene_viewport_tool = SceneViewportTool::Move;
                            }
                        }
                    });
                });
                self.runtime_viewport = Rect::from_min_max(
                    egui::pos2(available.min.x + 1.0, header.max.y),
                    egui::pos2(available.max.x - 1.0, available.max.y - 1.0),
                );
                ui.painter().rect_stroke(
                    available,
                    0.0,
                    Stroke::new(1.0, colors.border),
                    StrokeKind::Inside,
                );
                self.show_scene_object_handles(ui);
                if let Some(selected) = self.scene_outline.root.find(&self.selected_scene) {
                    let badge = Rect::from_min_size(
                        self.runtime_viewport.min + egui::vec2(12.0, 12.0),
                        egui::vec2(220.0, 42.0),
                    );
                    ui.painter()
                        .rect_filled(badge, UI.radius, colors.panel_raised);
                    ui.painter().rect_stroke(
                        badge,
                        UI.radius,
                        Stroke::new(1.0, colors.asset_selection_stroke),
                        StrokeKind::Inside,
                    );
                    ui.painter().text(
                        badge.min + egui::vec2(10.0, 13.0),
                        Align2::LEFT_CENTER,
                        format!("Selected · {}", selected.label),
                        semibold_font(TYPE.meta),
                        colors.text,
                    );
                    let hint = match (
                        selected_is_placeable,
                        selected_can_resize,
                        self.scene_viewport_tool,
                    ) {
                        (true, _, SceneViewportTool::Move)
                            if self.project_editable && !self.playing =>
                        {
                            "Drag the outline to move on X / Z"
                        }
                        (true, true, SceneViewportTool::Resize)
                            if self.project_editable && !self.playing =>
                        {
                            "Drag a corner to resize on X / Z"
                        }
                        (true, _, _) if self.playing => "Stop Play to edit this object",
                        (true, _, _) => "Open a source project to edit this object",
                        _ => "Properties appear in the Inspector",
                    };
                    ui.painter().text(
                        badge.min + egui::vec2(10.0, 29.0),
                        Align2::LEFT_CENTER,
                        hint,
                        FontId::proportional(TYPE.meta - 1.0),
                        colors.secondary_text,
                    );
                }
                ui.allocate_rect(self.runtime_viewport, Sense::hover());
            });
    }

    fn show_scene_object_handles(&mut self, ui: &mut egui::Ui) {
        let colors = palette(ui);
        let viewport = self.runtime_viewport;
        let projections = self.scene_object_projections.clone();
        for projection in projections.into_iter().rev() {
            let bounds = projection.bounds();
            if !bounds.intersects(viewport) {
                continue;
            }
            let selected = self.selected_scene == projection.id;
            let sense = if selected
                && self.project_editable
                && !self.playing
                && self.scene_viewport_tool == SceneViewportTool::Move
            {
                Sense::click_and_drag()
            } else {
                Sense::click()
            };
            let response = ui.interact(
                bounds.intersect(viewport),
                ui.id().with(("scene-object", &projection.id)),
                sense,
            );
            let pointer_over_shape = response
                .interact_pointer_pos()
                .is_some_and(|point| projection.contains(point));
            if response.clicked() && pointer_over_shape {
                self.select_scene_node(&projection.id);
                self.notice = if projection.size.is_some() {
                    "Object selected — use Move or Resize in the viewport"
                } else {
                    "Object selected — use Move in the viewport"
                }
                .to_owned();
            }
            if selected || (response.hovered() && pointer_over_shape) {
                let stroke = Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        colors.accent
                    } else {
                        colors.secondary_text
                    },
                );
                if let Some(corners) = projection.screen_corners {
                    for index in 0..4 {
                        ui.painter()
                            .line_segment([corners[index], corners[(index + 1) % 4]], stroke);
                    }
                } else {
                    ui.painter()
                        .circle_filled(projection.center_screen, 6.0, colors.panel_raised);
                    ui.painter()
                        .circle_stroke(projection.center_screen, 7.0, stroke);
                    ui.painter().line_segment(
                        [
                            projection.center_screen - egui::vec2(10.0, 0.0),
                            projection.center_screen + egui::vec2(10.0, 0.0),
                        ],
                        stroke,
                    );
                    ui.painter().line_segment(
                        [
                            projection.center_screen - egui::vec2(0.0, 10.0),
                            projection.center_screen + egui::vec2(0.0, 10.0),
                        ],
                        stroke,
                    );
                }
            }
            if selected
                && self.project_editable
                && !self.playing
                && self.scene_viewport_tool == SceneViewportTool::Move
            {
                let drag_id = response.id.with("origin");
                if response.drag_started() && pointer_over_shape {
                    ui.data_mut(|data| {
                        data.insert_temp(drag_id, (projection.position, projection.size));
                    });
                }
                if response.dragged()
                    && let Some(current_screen) = response.interact_pointer_pos()
                    && let Some((origin_position, size)) =
                        ui.data(|data| data.get_temp::<([f32; 3], Option<[f32; 3]>)>(drag_id))
                {
                    self.scene_viewport_edit_requested = Some(SceneViewportEditRequest::Move {
                        target: projection.id.clone(),
                        origin_screen: current_screen - response.drag_delta(),
                        current_screen,
                        origin_position,
                        size,
                    });
                }
                if response.drag_stopped() {
                    ui.data_mut(|data| data.remove::<([f32; 3], Option<[f32; 3]>)>(drag_id));
                }
            }
            response.clone().context_menu(|ui| {
                if self.project_editable {
                    if ui.button("Duplicate").clicked() {
                        self.scene_edit_requested = Some(SceneEditRequest::DuplicateObject {
                            target: projection.id.clone(),
                        });
                        ui.close();
                    }
                    if ui.button("Delete").clicked() {
                        self.scene_edit_requested = Some(SceneEditRequest::DeleteObject {
                            target: projection.id.clone(),
                        });
                        ui.close();
                    }
                }
            });

            if selected
                && self.project_editable
                && !self.playing
                && self.scene_viewport_tool == SceneViewportTool::Resize
                && let (Some(screen_corners), Some(world_corners), Some(size)) = (
                    projection.screen_corners,
                    projection.world_corners,
                    projection.size,
                )
            {
                for index in 0..4 {
                    let center = screen_corners[index];
                    let handle = Rect::from_center_size(center, Vec2::splat(12.0));
                    let handle_response = ui
                        .interact(
                            handle,
                            ui.id().with(("scene-object-resize", &projection.id, index)),
                            Sense::drag(),
                        )
                        .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
                    let fill = if handle_response.hovered() || handle_response.dragged() {
                        colors.accent
                    } else {
                        colors.panel_raised
                    };
                    ui.painter().rect(
                        handle.shrink(2.0),
                        2.0,
                        fill,
                        Stroke::new(1.0, colors.accent),
                        StrokeKind::Inside,
                    );
                    let drag_id = handle_response.id.with("origin");
                    if handle_response.drag_started() {
                        ui.data_mut(|data| {
                            data.insert_temp(
                                drag_id,
                                (projection.position, size, world_corners[(index + 2) % 4]),
                            );
                        });
                    }
                    if handle_response.dragged()
                        && let Some(current_screen) = handle_response.interact_pointer_pos()
                        && let Some((origin_position, origin_size, fixed_corner)) =
                            ui.data(|data| data.get_temp::<([f32; 3], [f32; 3], [f32; 3])>(drag_id))
                    {
                        self.scene_viewport_edit_requested =
                            Some(SceneViewportEditRequest::Resize {
                                target: projection.id.clone(),
                                current_screen,
                                fixed_corner,
                                origin_position,
                                origin_size,
                            });
                    }
                    if handle_response.drag_stopped() {
                        ui.data_mut(|data| {
                            data.remove::<([f32; 3], [f32; 3], [f32; 3])>(drag_id);
                        });
                    }
                }
            }
        }
    }
}
