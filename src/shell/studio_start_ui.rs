use super::*;

impl StudioShell {
    pub(crate) fn show_start_screen(&mut self, root: &mut egui::Ui) {
        let log_layout = !self.start_screen_logged;
        if log_layout {
            log::info!(
                "start screen: rendering {} recent project(s): {}",
                self.recent_projects.len(),
                crate::recent_project_log_list(&self.recent_projects)
            );
            self.start_screen_logged = true;
        }
        let colors = palette(root);
        egui::CentralPanel::default()
            .frame(editor_frame(colors.surface_deep))
            .show(root, |ui| {
                let content_width = ui.available_width().min(640.0);
                let content_height = ui.available_height();
                ui.vertical_centered(|ui| {
                    // Keep the chooser column centered at a consistent width.
                    ui.set_width(content_width);
                    ui.add_space((content_height - 430.0).max(28.0) * 0.42);
                    ui.add(
                        egui::Image::from_texture(&self.logo_texture)
                            .fit_to_exact_size(egui::vec2(48.0, 48.0))
                            .sense(Sense::hover()),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Cubacadabra Studio")
                            .font(semibold_font(22.0))
                            .color(colors.text),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Open a project to begin")
                            .size(TYPE.secondary)
                            .color(colors.secondary_text),
                    );
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        let button_width = (ui.available_width() - 8.0) * 0.5;
                        if ui
                            .add_sized(
                                [button_width, 34.0],
                                egui::Button::new(
                                    RichText::new("Open Project…")
                                        .font(medium_font(TYPE.secondary))
                                        .color(if ui.visuals().dark_mode {
                                            colors.surface_deep
                                        } else {
                                            Color32::WHITE
                                        }),
                                )
                                .fill(colors.accent)
                                .stroke(Stroke::NONE)
                                .corner_radius(4.0),
                            )
                            .clicked()
                        {
                            self.open_project_requested = true;
                            self.notice = "Choose a project folder…".to_owned();
                        }
                        if ui
                            .add_sized(
                                [button_width, 34.0],
                                egui::Button::new(
                                    RichText::new("New Project…")
                                        .font(medium_font(TYPE.secondary))
                                        .color(colors.text),
                                )
                                .fill(colors.panel_raised)
                                .stroke(Stroke::new(1.0, colors.border))
                                .corner_radius(4.0),
                            )
                            .clicked()
                        {
                            self.execute_command(StudioCommand::NewProject);
                        }
                    });
                    ui.add_space(28.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Recent Projects")
                                .font(semibold_font(TYPE.primary))
                                .color(colors.text),
                        );
                    });
                    ui.add_space(6.0);
                    if self.recent_projects.is_empty() {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(content_width, 44.0), Sense::hover());
                        ui.painter().rect(
                            rect,
                            4.0,
                            colors.panel,
                            Stroke::new(1.0, colors.border),
                            StrokeKind::Inside,
                        );
                        ui.painter().text(
                            rect.left_center() + egui::vec2(12.0, 0.0),
                            Align2::LEFT_CENTER,
                            "Your opened projects will appear here.",
                            FontId::proportional(TYPE.secondary),
                            colors.muted,
                        );
                    } else {
                        let recent_projects = self.recent_projects.clone();
                        for project in recent_projects {
                            let project_name = crate::game_name(&project);
                            let path = project.display().to_string();
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(content_width, 48.0),
                                Sense::click(),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    format!("Open {project_name}"),
                                )
                            });
                            if log_layout {
                                log::info!(
                                    "start screen: allocated recent row for {} at {:?}",
                                    project.display(),
                                    rect
                                );
                            }
                            ui.painter().rect(
                                rect,
                                4.0,
                                if response.hovered() {
                                    colors.panel_raised
                                } else if ui.visuals().dark_mode {
                                    colors.panel
                                } else {
                                    colors.field
                                },
                                Stroke::new(1.0, colors.border),
                                StrokeKind::Inside,
                            );
                            paint_icon(
                                ui.painter(),
                                Rect::from_center_size(
                                    rect.left_center() + egui::vec2(18.0, 0.0),
                                    Vec2::splat(UI.icon),
                                ),
                                Icon::Folder,
                                colors.muted,
                            );
                            let text_rect = Rect::from_min_max(
                                rect.min + egui::vec2(34.0, 4.0),
                                rect.max - egui::vec2(62.0, 4.0),
                            );
                            let text_painter = ui.painter().with_clip_rect(text_rect);
                            text_painter.text(
                                egui::pos2(text_rect.min.x, rect.center().y - 8.0),
                                Align2::LEFT_CENTER,
                                &project_name,
                                medium_font(TYPE.secondary),
                                colors.text,
                            );
                            text_painter.text(
                                egui::pos2(text_rect.min.x, rect.center().y + 9.0),
                                Align2::LEFT_CENTER,
                                &path,
                                FontId::proportional(TYPE.meta),
                                colors.muted,
                            );
                            ui.painter().text(
                                rect.right_center() - egui::vec2(12.0, 0.0),
                                Align2::RIGHT_CENTER,
                                "Open",
                                medium_font(TYPE.secondary),
                                colors.secondary_text,
                            );
                            paint_focus(ui, &response);
                            if response.clicked() {
                                self.recent_project_requested = Some(project);
                                self.notice = format!("Opening {project_name}…");
                            }
                            response.on_hover_text(path);
                            ui.add_space(4.0);
                        }
                    }
                });
            });
    }
}
