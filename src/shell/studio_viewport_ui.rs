use super::*;
impl StudioShell {
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
                    vertical_separator(ui, 12.0);
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            self.scene_tool.label(),
                            self.scene_tool.shortcut()
                        ))
                        .font(medium_font(TYPE.secondary))
                        .color(colors.accent),
                    )
                    .on_hover_text("Right-click an object to change editing mode");
                    if self.project_editable {
                        vertical_separator(ui, 12.0);
                        let add = ui
                            .add_enabled(!self.playing, egui::Button::new("Add"))
                            .on_hover_text(if self.playing {
                                "Stop Preview to add objects"
                            } else {
                                "Add object (Shift+A)"
                            });
                        if add.clicked() {
                            self.open_add_palette(
                                scene_world_id(&self.selected_scene).map(str::to_owned),
                            );
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
                self.play_viewports.clear();
                if self.playing && self.play_player_count > 1 {
                    self.play_viewports = play_overlay_rects(
                        self.runtime_viewport,
                        &self.play_player_slots,
                    );
                    for (index, tile) in self.play_viewports.iter().copied().enumerate() {
                        let selected = index == self.controlled_player;
                        if selected {
                            continue;
                        }
                        ui.painter().rect_stroke(
                            tile,
                            0.0,
                            Stroke::new(1.0, colors.border),
                            StrokeKind::Inside,
                        );
                        let label = self
                            .play_player_name(index)
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("Player {}", index + 1));
                        let badge = Rect::from_min_size(
                            tile.min + egui::vec2(8.0, 8.0),
                            egui::vec2(76.0_f32.min((tile.width() - 16.0).max(0.0)), 24.0),
                        );
                        ui.painter().rect_filled(badge, UI.radius, Color32::from_black_alpha(185));
                        ui.painter().text(badge.center(), Align2::CENTER_CENTER, label, medium_font(TYPE.meta), Color32::WHITE);
                    }
                    let badge = Rect::from_min_size(
                        self.runtime_viewport.min + egui::vec2(12.0, 12.0),
                        egui::vec2(154.0, 24.0),
                    );
                    ui.painter().rect_filled(badge, UI.radius, Color32::from_black_alpha(185));
                    ui.painter().text(
                        badge.center(),
                        Align2::CENTER_CENTER,
                        format!(
                            "{} · Controlling",
                            self.play_player_name(self.controlled_player)
                                .unwrap_or("Player")
                        ),
                        medium_font(TYPE.meta),
                        Color32::WHITE,
                    );
                }
                ui.painter().rect_stroke(
                    available,
                    0.0,
                    Stroke::new(1.0, colors.border),
                    StrokeKind::Inside,
                );
                // Scene handles belong to authoring mode. Keep the current
                // selection available to the inspector while playing, but do
                // not draw editor bounds over the live game view or let them
                // compete with gameplay input.
                if !self.playing {
                    self.show_scene_object_handles(ui);
                }
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
                        if self.selected_scenes.len() > 1 {
                            format!("Selected · {} objects", self.selected_scenes.len())
                        } else {
                            format!("Selected · {}", selected.label)
                        },
                        semibold_font(TYPE.meta),
                        colors.text,
                    );
                    let hint = if self.playing {
                        "Stop Play to edit this object"
                    } else if !self.project_editable {
                        "Open a source project to edit this object"
                    } else {
                        self.scene_tool.hint(selected_can_resize)
                    };
                    ui.painter().text(
                        badge.min + egui::vec2(10.0, 29.0),
                        Align2::LEFT_CENTER,
                        hint,
                        FontId::proportional(TYPE.meta - 1.0),
                        colors.secondary_text,
                    );
                }
                let clicked_empty = !self.playing
                    && ui.input(|input| {
                        input.pointer.primary_clicked()
                            && input.pointer.latest_pos().is_some_and(|point| {
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
                let right_clicked_empty = !self.playing
                    && self.project_editable
                    && ui.input(|input| {
                        input.pointer.secondary_clicked()
                            && input.pointer.latest_pos().is_some_and(|point| {
                                self.runtime_viewport.contains(point)
                                    && !self.scene_object_projections.iter().any(|projection| {
                                        projection.bounds().contains(point)
                                    })
                            })
                    });
                if right_clicked_empty {
                    self.open_add_palette(
                        scene_world_id(&self.selected_scene).map(str::to_owned),
                    );
                }
                ui.allocate_rect(self.runtime_viewport, Sense::hover());
            });
    }

    fn show_scene_object_handles(&mut self, ui: &mut egui::Ui) {
        let colors = palette(ui);
        let viewport = self.runtime_viewport;
        let projections = std::mem::take(&mut self.scene_object_projections);
        for projection in projections.iter().rev() {
            let bounds = projection.bounds();
            if !bounds.intersects(viewport) {
                continue;
            }
            let selected = self.selected_scenes.contains(&projection.id);
            let primary = self.selected_scene == projection.id;
            let sense = if primary
                && self.project_editable
                && !self.playing
                && projection.editable
                && self.scene_tool.moves()
            {
                Sense::click_and_drag()
            } else {
                Sense::click()
            };
            let interaction_bounds = if primary
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
                let additive = ui.input(|input| input.modifiers.shift || input.modifiers.command);
                if additive {
                    self.toggle_scene_node_selection(&projection.id);
                } else {
                    self.select_scene_node(&projection.id);
                }
                let label = self
                    .scene_outline
                    .root
                    .find(&projection.id)
                    .map(|node| node.label.as_str())
                    .unwrap_or("Object");
                self.notice = format!(
                    "{label} selected — {}",
                    self.scene_tool.hint(projection.size.is_some())
                );
            }
            if response.secondary_clicked() && pointer_over_shape {
                if !selected {
                    self.select_scene_node(&projection.id);
                }
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
            if primary
                && self.project_editable
                && !self.playing
                && projection.editable
                && self.scene_tool.moves()
            {
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
                if self.project_editable && !self.playing {
                    if ui.button("Add…").clicked() {
                        self.add_palette = Some(AddPaletteState::new(
                            scene_world_id(&projection.id).map(str::to_owned),
                        ));
                        ui.close();
                    }
                    ui.separator();
                    ui.label(
                        RichText::new("Editing mode")
                            .size(TYPE.meta)
                            .color(palette(ui).muted),
                    );
                    for tool in SceneTool::ALL {
                        let label = format!("{}    {}", tool.label(), tool.shortcut());
                        if ui
                            .selectable_label(self.scene_tool == tool, label)
                            .on_hover_text(tool.hint(projection.size.is_some()))
                            .clicked()
                        {
                            self.set_scene_tool(tool);
                            ui.close();
                        }
                    }
                    ui.separator();
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
                && self.scene_tool.resizes()
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
                                    projection.rotation,
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
                                origin_rotation: projection.rotation,
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
                            origin_rotation,
                            origin_size,
                            fixed_corner,
                            origin_scale,
                            base_size,
                        )) = ui.data(|data| {
                            data.get_temp::<(
                                [f32; 3],
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
                                origin_rotation,
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
                                origin_rotation,
                                origin_size,
                                fixed_corner,
                                origin_scale,
                                base_size,
                            )) = ui.data(|data| {
                                data.get_temp::<(
                                    [f32; 3],
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
                                    origin_rotation,
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
                && self.scene_tool.moves()
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
                && self.scene_tool.resizes()
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

            if selected
                && self.project_editable
                && !self.playing
                && projection.editable
                && self.scene_tool.rotates()
            {
                let bounds = projection.bounds();
                let center = projection.center_screen;
                let radius = (bounds.width().max(bounds.height()) * 0.5 + 18.0).clamp(28.0, 112.0);
                let handle_center = center + egui::vec2(radius, 0.0);
                let handle = Rect::from_center_size(handle_center, Vec2::splat(14.0));
                let handle_response = ui
                    .interact(
                        handle,
                        ui.id().with(("scene-object-turn", &projection.id)),
                        Sense::drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                    .on_hover_text("Turn around the vertical axis");
                ui.painter().circle_stroke(
                    center,
                    radius,
                    Stroke::new(1.5, colors.accent.gamma_multiply(0.72)),
                );
                let fill = if handle_response.hovered() || handle_response.dragged() {
                    colors.accent
                } else {
                    colors.panel_raised
                };
                ui.painter()
                    .circle(handle_center, 6.0, fill, Stroke::new(1.5, colors.accent));
                ui.painter().text(
                    handle_center + egui::vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    format!("{:.0}°", projection.local_rotation[1].to_degrees()),
                    medium_font(TYPE.meta),
                    colors.accent,
                );
                let drag_id = handle_response.id.with("origin");
                if handle_response.drag_started() {
                    ui.data_mut(|data| {
                        data.insert_temp(
                            drag_id,
                            (
                                handle_center,
                                projection.position,
                                projection.local_rotation,
                            ),
                        );
                    });
                    self.scene_viewport_edit_requested =
                        Some(SceneViewportEditRequest::RotateYaw {
                            phase: SceneViewportEditPhase::Begin,
                            target: projection.id.clone(),
                            origin_screen: handle_center,
                            current_screen: handle_center,
                            origin_position: projection.position,
                            origin_rotation: projection.local_rotation,
                        });
                }
                if handle_response.dragged()
                    && let Some(current_screen) = handle_response.interact_pointer_pos()
                    && let Some((origin_screen, origin_position, origin_rotation)) =
                        ui.data(|data| data.get_temp::<(Pos2, [f32; 3], [f32; 3])>(drag_id))
                {
                    self.scene_viewport_edit_requested =
                        Some(SceneViewportEditRequest::RotateYaw {
                            phase: SceneViewportEditPhase::Update,
                            target: projection.id.clone(),
                            origin_screen,
                            current_screen,
                            origin_position,
                            origin_rotation,
                        });
                }
                if handle_response.drag_stopped() {
                    if let Some(current_screen) = handle_response.interact_pointer_pos()
                        && let Some((origin_screen, origin_position, origin_rotation)) =
                            ui.data(|data| data.get_temp::<(Pos2, [f32; 3], [f32; 3])>(drag_id))
                    {
                        self.scene_viewport_edit_requested =
                            Some(SceneViewportEditRequest::RotateYaw {
                                phase: SceneViewportEditPhase::Commit,
                                target: projection.id.clone(),
                                origin_screen,
                                current_screen,
                                origin_position,
                                origin_rotation,
                            });
                    }
                    ui.data_mut(|data| {
                        data.remove::<(Pos2, [f32; 3], [f32; 3])>(drag_id);
                    });
                }
            }
        }
        self.scene_object_projections = projections;
    }
}

pub(super) fn play_overlay_rects(viewport: Rect, player_slots: &[usize]) -> Vec<Rect> {
    let width = (viewport.width() * 0.28)
        .min(viewport.height() * 0.32)
        .min(320.0);
    let height = width * 9.0 / 16.0;
    let inset_x = (viewport.width() * 0.022).max(8.0);
    let inset_y = (viewport.height() * 0.025).max(8.0);
    let left = viewport.left() + inset_x;
    let right = viewport.right() - inset_x - width;
    let top = viewport.top() + inset_y;
    let bottom = viewport.bottom() - inset_y - height;
    let side_top = viewport.top() + viewport.height() * 0.08;
    let side_bottom = viewport.bottom() - viewport.height() * 0.15 - height;
    let side_middle = (side_top + side_bottom) * 0.5;
    let center = viewport.center().x - width * 0.5;
    let slots = match player_slots.len() {
        3 => vec![(left, side_middle), (right, side_middle)],
        6 => vec![
            (center, top),
            (left, side_top),
            (right, side_top),
            (left, side_bottom),
            (right, side_bottom),
        ],
        _ => vec![
            (center, top),
            (left, side_top),
            (right, side_top),
            (left, side_middle),
            (right, side_middle),
            (left, side_bottom),
            (right, side_bottom),
            (center, bottom),
        ],
    };
    player_slots
        .iter()
        .map(|&slot| {
            if slot == 0 {
                viewport
            } else {
                let (x, y) = slots[slot - 1];
                Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, height))
            }
        })
        .collect()
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
#[path = "studio_viewport_ui_tests.rs"]
mod tests;
