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
                let selected_id = selected_projection.map(|item| item.id.clone());
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
                    if self.project_editable {
                        vertical_separator(ui, 12.0);
                        if ui
                            .button("+ Block")
                            .on_hover_text("Add a block beside the existing blocks")
                            .clicked()
                        {
                            self.set_playing(false);
                            self.scene_edit_requested = Some(SceneEditRequest::AddObject {
                                world_id: scene_world_id(&self.selected_scene).map(str::to_owned),
                                kind: SceneObjectKind::Block,
                            });
                            self.notice = "Adding block…".to_owned();
                        }
                        if let Some(target) = selected_id.as_ref()
                            && ui
                                .button("Duplicate")
                                .on_hover_text("Make a copy beside the selected object")
                                .clicked()
                        {
                            self.set_playing(false);
                            self.scene_edit_requested = Some(SceneEditRequest::DuplicateObject {
                                target: target.clone(),
                            });
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.playing {
                            ui.label(
                                RichText::new("Gameplay camera")
                                    .size(TYPE.secondary)
                                    .color(colors.secondary_text),
                            );
                        } else {
                            for preset in [
                                ReviewCameraPreset::Overview,
                                ReviewCameraPreset::Showcase,
                            ] {
                                if ui
                                    .selectable_label(self.review_camera == preset, preset.label())
                                    .on_hover_text("Right-drag to orbit · middle-drag to pan · scroll/pinch to zoom · click again to frame the world")
                                    .clicked()
                                {
                                    self.set_review_camera(preset);
                                }
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
                if selected_is_placeable
                    && let Some(selected) = self.scene_outline.root.find(&self.selected_scene)
                {
                    let badge = Rect::from_min_size(
                        self.runtime_viewport.min + egui::vec2(12.0, 12.0),
                        egui::vec2(330.0, 42.0),
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
                    let hint = match (selected_can_resize, self.project_editable, self.playing) {
                        (true, true, false) => {
                            "Drag to move · arrows nudge 0.25 · handles resize or lift"
                        }
                        (false, true, false) => "Drag to move · arrows nudge 0.25",
                        (_, _, true) => "Stop Play to edit this object",
                        _ => "Open a source project to edit this object",
                    };
                    ui.painter().text(
                        badge.min + egui::vec2(10.0, 29.0),
                        Align2::LEFT_CENTER,
                        hint,
                        FontId::proportional(TYPE.meta - 1.0),
                        colors.secondary_text,
                    );
                }
                let clicked_empty = ui.input(|input| {
                    input.pointer.primary_clicked()
                        && input
                            .pointer
                            .latest_pos()
                            .is_some_and(|point| {
                                self.runtime_viewport.contains(point)
                                    && !self
                                        .scene_object_projections
                                        .iter()
                                        .any(|projection| {
                                            projection
                                                .bounds()
                                                .expand(
                                                    (projection.id == self.selected_scene)
                                                        .then_some(36.0)
                                                        .unwrap_or(0.0),
                                                )
                                                .contains(point)
                                        })
                            })
                });
                if clicked_empty {
                    self.deselect_scene_object();
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
            let sense = if selected && self.project_editable && !self.playing && projection.editable
            {
                Sense::click_and_drag()
            } else {
                Sense::click()
            };
            let interaction_bounds = if selected
                && projection.screen_corners.is_some()
                && bounds.width() > 20.0
                && bounds.height() > 20.0
            {
                // Leave the edge handles a clear hit area while the block body
                // remains the large, forgiving move target.
                bounds.shrink(8.0)
            } else {
                bounds
            };
            let response = ui.interact(
                interaction_bounds.intersect(viewport),
                ui.id().with(("scene-object", &projection.id)),
                sense,
            );
            let pointer_over_shape = response
                .interact_pointer_pos()
                .is_some_and(|point| projection.contains(point));
            if response.clicked() && pointer_over_shape {
                self.select_scene_node(&projection.id);
                self.notice = if projection.size.is_some() {
                    "Object selected — drag it to move or use the handles to resize"
                } else {
                    "Object selected — drag it to move"
                }
                .to_owned();
            }
            if response.double_clicked() && pointer_over_shape {
                self.request_scene_focus();
            }
            if self.preview_is_stale()
                && !self.playing
                && let (Some(top), Some(bottom)) =
                    (projection.screen_corners, projection.bottom_screen_corners)
            {
                paint_scene_preview_cube(
                    ui.painter(),
                    top,
                    bottom,
                    if selected {
                        colors.accent
                    } else {
                        colors.faint
                    },
                );
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
                    if let Some(bottom) = projection.bottom_screen_corners {
                        for index in 0..4 {
                            ui.painter()
                                .line_segment([corners[index], bottom[index]], stroke);
                            ui.painter()
                                .line_segment([bottom[index], bottom[(index + 1) % 4]], stroke);
                        }
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
            if selected && self.project_editable && !self.playing && projection.editable {
                let drag_id = response.id.with("origin");
                if response.drag_started()
                    && let Some(current_screen) = response.interact_pointer_pos()
                    && let Some(total_drag_delta) = response.total_drag_delta()
                    && let Some(origin_screen) =
                        scene_move_drag_origin(&projection, current_screen, total_drag_delta)
                {
                    ui.data_mut(|data| {
                        data.insert_temp(drag_id, (origin_screen, projection.position));
                    });
                    self.scene_viewport_edit_requested = Some(SceneViewportEditRequest::Move {
                        phase: SceneViewportEditPhase::Begin,
                        target: projection.id.clone(),
                        origin_screen,
                        current_screen,
                        origin_position: projection.position,
                    });
                }
                if response.dragged()
                    && let Some(current_screen) = response.interact_pointer_pos()
                    && let Some((origin_screen, origin_position)) =
                        ui.data(|data| data.get_temp::<(Pos2, [f32; 3])>(drag_id))
                {
                    self.scene_viewport_edit_requested = Some(SceneViewportEditRequest::Move {
                        phase: SceneViewportEditPhase::Update,
                        target: projection.id.clone(),
                        origin_screen,
                        current_screen,
                        origin_position,
                    });
                }
                if response.drag_stopped() {
                    if let Some(current_screen) = response.interact_pointer_pos()
                        && let Some((origin_screen, origin_position)) =
                            ui.data(|data| data.get_temp::<(Pos2, [f32; 3])>(drag_id))
                    {
                        self.scene_viewport_edit_requested = Some(SceneViewportEditRequest::Move {
                            phase: SceneViewportEditPhase::Commit,
                            target: projection.id.clone(),
                            origin_screen,
                            current_screen,
                            origin_position,
                        });
                    }
                    ui.data_mut(|data| data.remove::<(Pos2, [f32; 3])>(drag_id));
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
                && projection.editable
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
                                (
                                    projection.position,
                                    size,
                                    world_corners[(index + 2) % 4],
                                    projection.scale,
                                    projection.base_size,
                                ),
                            );
                        });
                        self.scene_viewport_edit_requested =
                            Some(SceneViewportEditRequest::Resize {
                                phase: SceneViewportEditPhase::Begin,
                                target: projection.id.clone(),
                                current_screen: center,
                                fixed_corner: world_corners[(index + 2) % 4],
                                origin_position: projection.position,
                                origin_size: size,
                                origin_scale: projection.scale,
                                base_size: projection.base_size,
                                primitive_size: projection.primitive_size,
                            });
                    }
                    if handle_response.dragged()
                        && let Some(current_screen) = handle_response.interact_pointer_pos()
                        && let Some((
                            origin_position,
                            origin_size,
                            fixed_corner,
                            origin_scale,
                            base_size,
                        )) = ui.data(|data| {
                            data.get_temp::<(
                                [f32; 3],
                                [f32; 3],
                                [f32; 3],
                                Option<[f32; 3]>,
                                Option<[f32; 3]>,
                            )>(drag_id)
                        })
                    {
                        self.scene_viewport_edit_requested =
                            Some(SceneViewportEditRequest::Resize {
                                phase: SceneViewportEditPhase::Update,
                                target: projection.id.clone(),
                                current_screen,
                                fixed_corner,
                                origin_position,
                                origin_size,
                                origin_scale,
                                base_size,
                                primitive_size: projection.primitive_size,
                            });
                    }
                    if handle_response.drag_stopped() {
                        if let Some(current_screen) = handle_response.interact_pointer_pos()
                            && let Some((
                                origin_position,
                                origin_size,
                                fixed_corner,
                                origin_scale,
                                base_size,
                            )) = ui.data(|data| {
                                data.get_temp::<(
                                    [f32; 3],
                                    [f32; 3],
                                    [f32; 3],
                                    Option<[f32; 3]>,
                                    Option<[f32; 3]>,
                                )>(drag_id)
                            })
                        {
                            self.scene_viewport_edit_requested =
                                Some(SceneViewportEditRequest::Resize {
                                    phase: SceneViewportEditPhase::Commit,
                                    target: projection.id.clone(),
                                    current_screen,
                                    fixed_corner,
                                    origin_position,
                                    origin_size,
                                    origin_scale,
                                    base_size,
                                    primitive_size: projection.primitive_size,
                                });
                        }
                        ui.data_mut(|data| {
                            data.remove::<(
                                [f32; 3],
                                [f32; 3],
                                [f32; 3],
                                Option<[f32; 3]>,
                                Option<[f32; 3]>,
                            )>(drag_id);
                        });
                    }
                }
            }

            if selected
                && self.project_editable
                && !self.playing
                && projection.editable
                && let Some(screen_corners) = projection.screen_corners
            {
                let top = screen_corners[0].lerp(screen_corners[1], 0.5);
                let center = top - egui::vec2(0.0, 28.0);
                let handle = Rect::from_center_size(center, Vec2::splat(14.0));
                let handle_response = ui
                    .interact(
                        handle,
                        ui.id().with(("scene-object-lift", &projection.id)),
                        Sense::drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                    .on_hover_text("Raise or lower the block");
                ui.painter().line_segment(
                    [top, center + egui::vec2(0.0, 6.0)],
                    Stroke::new(1.0, colors.accent),
                );
                let fill = if handle_response.hovered() || handle_response.dragged() {
                    colors.accent
                } else {
                    colors.panel_raised
                };
                ui.painter()
                    .circle(center, 6.0, fill, Stroke::new(1.0, colors.accent));
                ui.painter().line_segment(
                    [center - egui::vec2(3.0, 0.0), center + egui::vec2(3.0, 0.0)],
                    Stroke::new(1.0, colors.accent),
                );
                let drag_id = handle_response.id.with("origin");
                if handle_response.drag_started() {
                    ui.data_mut(|data| {
                        data.insert_temp(drag_id, (center, projection.position));
                    });
                    self.scene_viewport_edit_requested =
                        Some(SceneViewportEditRequest::MoveHeight {
                            phase: SceneViewportEditPhase::Begin,
                            target: projection.id.clone(),
                            origin_screen: center,
                            current_screen: center,
                            origin_position: projection.position,
                        });
                }
                if handle_response.dragged()
                    && let Some(current_screen) = handle_response.interact_pointer_pos()
                    && let Some((origin_screen, origin_position)) =
                        ui.data(|data| data.get_temp::<(Pos2, [f32; 3])>(drag_id))
                {
                    self.scene_viewport_edit_requested =
                        Some(SceneViewportEditRequest::MoveHeight {
                            phase: SceneViewportEditPhase::Update,
                            target: projection.id.clone(),
                            origin_screen,
                            current_screen,
                            origin_position,
                        });
                }
                if handle_response.drag_stopped() {
                    if let Some(current_screen) = handle_response.interact_pointer_pos()
                        && let Some((origin_screen, origin_position)) =
                            ui.data(|data| data.get_temp::<(Pos2, [f32; 3])>(drag_id))
                    {
                        self.scene_viewport_edit_requested =
                            Some(SceneViewportEditRequest::MoveHeight {
                                phase: SceneViewportEditPhase::Commit,
                                target: projection.id.clone(),
                                origin_screen,
                                current_screen,
                                origin_position,
                            });
                    }
                    ui.data_mut(|data| data.remove::<(Pos2, [f32; 3])>(drag_id));
                }
            }

            if selected
                && self.project_editable
                && !self.playing
                && projection.editable
                && let (Some(screen_corners), Some(base_size)) =
                    (projection.screen_corners, projection.base_size)
            {
                let center = screen_corners[0].lerp(screen_corners[1], 0.5);
                let handle = Rect::from_center_size(center, Vec2::splat(12.0));
                let handle_response = ui
                    .interact(
                        handle,
                        ui.id().with(("scene-object-height", &projection.id)),
                        Sense::drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical);
                let fill = if handle_response.hovered() || handle_response.dragged() {
                    colors.accent
                } else {
                    colors.panel_raised
                };
                ui.painter()
                    .circle(center, 6.0, fill, Stroke::new(1.0, colors.accent));
                let drag_id = handle_response.id.with("origin");
                if handle_response.drag_started() {
                    ui.data_mut(|data| {
                        data.insert_temp(
                            drag_id,
                            (center, projection.position, projection.scale, base_size),
                        );
                    });
                }
                if handle_response.dragged()
                    && let Some(current_screen) = handle_response.interact_pointer_pos()
                    && let Some((origin_screen, origin_position, origin_scale, base_size)) = ui
                        .data(|data| {
                            data.get_temp::<(Pos2, [f32; 3], Option<[f32; 3]>, [f32; 3])>(drag_id)
                        })
                {
                    self.scene_viewport_edit_requested =
                        Some(SceneViewportEditRequest::ResizeHeight {
                            phase: SceneViewportEditPhase::Update,
                            target: projection.id.clone(),
                            origin_screen,
                            current_screen,
                            origin_position,
                            origin_scale,
                            base_size,
                            primitive_size: projection.primitive_size,
                        });
                }
                if handle_response.drag_started() {
                    self.scene_viewport_edit_requested =
                        Some(SceneViewportEditRequest::ResizeHeight {
                            phase: SceneViewportEditPhase::Begin,
                            target: projection.id.clone(),
                            origin_screen: center,
                            current_screen: center,
                            origin_position: projection.position,
                            origin_scale: projection.scale,
                            base_size,
                            primitive_size: projection.primitive_size,
                        });
                }
                if handle_response.drag_stopped()
                    && let Some(current_screen) = handle_response.interact_pointer_pos()
                    && let Some((origin_screen, origin_position, origin_scale, base_size)) = ui
                        .data(|data| {
                            data.get_temp::<(Pos2, [f32; 3], Option<[f32; 3]>, [f32; 3])>(drag_id)
                        })
                {
                    self.scene_viewport_edit_requested =
                        Some(SceneViewportEditRequest::ResizeHeight {
                            phase: SceneViewportEditPhase::Commit,
                            target: projection.id.clone(),
                            origin_screen,
                            current_screen,
                            origin_position,
                            origin_scale,
                            base_size,
                            primitive_size: projection.primitive_size,
                        });
                }
                if handle_response.drag_stopped() {
                    ui.data_mut(|data| {
                        data.remove::<(Pos2, [f32; 3], Option<[f32; 3]>, [f32; 3])>(drag_id);
                    });
                }
            }
        }
    }
}

fn scene_move_drag_origin(
    projection: &SceneObjectProjection,
    current_screen: Pos2,
    total_drag_delta: Vec2,
) -> Option<Pos2> {
    let origin_screen = current_screen - total_drag_delta;
    projection.contains(origin_screen).then_some(origin_screen)
}

fn paint_scene_preview_cube(
    painter: &egui::Painter,
    top: [Pos2; 4],
    bottom: [Pos2; 4],
    color: Color32,
) {
    let side = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 34);
    let top_fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 52);
    for index in [2, 1, 0, 3] {
        painter.add(egui::Shape::convex_polygon(
            vec![
                top[index],
                top[(index + 1) % 4],
                bottom[(index + 1) % 4],
                bottom[index],
            ],
            side,
            Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::convex_polygon(
        top.to_vec(),
        top_fill,
        Stroke::new(1.0, color),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection(corners: [Pos2; 4]) -> SceneObjectProjection {
        SceneObjectProjection {
            id: "block".to_owned(),
            position: [0.0, 1.0, 0.0],
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
}
