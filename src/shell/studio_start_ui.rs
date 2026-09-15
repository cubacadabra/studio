use super::*;

impl StudioShell {
    pub(crate) fn show_start_screen(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::CentralPanel::default()
            .frame(editor_frame(colors.surface_deep))
            .show(root, |ui| {
                let content_width = ui.available_width().min(640.0);
                let content_height = ui.available_height();
                ui.vertical_centered(|ui| {
                    ui.set_max_width(content_width);
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
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.label(
                            RichText::new("Recent Projects")
                                .font(semibold_font(TYPE.primary))
                                .color(colors.text),
                        );
                    });
                    ui.add_space(6.0);
                    if self.recent_projects.is_empty() {
                        Frame::NONE
                            .fill(colors.panel)
                            .stroke(Stroke::new(1.0, colors.border))
                            .corner_radius(4.0)
                            .inner_margin(Margin::symmetric(12, 14))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new("Your opened projects will appear here.")
                                        .size(TYPE.secondary)
                                        .color(colors.muted),
                                );
                            });
                    } else {
                        let recent_projects = self.recent_projects.clone();
                        for project in recent_projects {
                            let project_name = crate::game_name(&project);
                            let path = project.display().to_string();
                            Frame::NONE
                                .fill(if ui.visuals().dark_mode {
                                    colors.panel
                                } else {
                                    colors.field
                                })
                                .stroke(Stroke::new(1.0, colors.border))
                                .corner_radius(4.0)
                                .inner_margin(Margin::symmetric(10, 7))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        inline_icon(ui, Icon::Folder, colors.muted);
                                        ui.vertical(|ui| {
                                            ui.set_width((ui.available_width() - 78.0).max(40.0));
                                            ui.label(
                                                RichText::new(&project_name)
                                                    .font(medium_font(TYPE.secondary))
                                                    .color(colors.text),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&path)
                                                        .size(TYPE.meta)
                                                        .color(colors.muted),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        if ui
                                            .add_sized([64.0, 26.0], egui::Button::new("Open"))
                                            .clicked()
                                        {
                                            self.recent_project_requested = Some(project.clone());
                                            self.notice = format!("Opening {project_name}…");
                                        }
                                    });
                                });
                            ui.add_space(4.0);
                        }
                    }
                });
            });
    }
}
