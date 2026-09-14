use super::*;
impl StudioShell {
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn show_new_project_dialog(&mut self, context: &egui::Context) {
        if !self.new_project_dialog_open {
            return;
        }
        let mut close_requested = false;
        let dialog_width = (context.content_rect().width() - 40.0).clamp(280.0, 440.0);
        let response = egui::Modal::new(egui::Id::new("new_project_dialog"))
            .backdrop_color(Color32::from_black_alpha(128))
            .frame(
                Frame::NONE
                    .fill(if context.style_of(context.theme()).visuals.dark_mode {
                        DARK_PALETTE.panel_raised
                    } else {
                        LIGHT_PALETTE.surface_deep
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if context.style_of(context.theme()).visuals.dark_mode {
                            DARK_PALETTE.border_strong
                        } else {
                            LIGHT_PALETTE.border
                        },
                    ))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(20)),
            )
            .show(context, |ui| {
                let colors = palette(ui);
                ui.set_width(dialog_width);
                ui.label(
                    RichText::new("New Project")
                        .font(semibold_font(18.0))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Create a starter game with its manifest, Luau entry point, and local SDK.",
                    )
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
                );
                ui.add_space(18.0);
                ui.label(
                    RichText::new("Game title")
                        .font(medium_font(TYPE.secondary))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                let title_response = ui.add(
                    egui::TextEdit::singleline(&mut self.new_project_title)
                        .hint_text("The Wild West")
                        .desired_width(ui.available_width())
                        .min_size(egui::vec2(0.0, 28.0))
                        .vertical_align(Align::Center),
                );
                if std::mem::take(&mut self.new_project_title_focus_requested) {
                    title_response.request_focus();
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Location")
                        .font(medium_font(TYPE.secondary))
                        .color(colors.text),
                );
                ui.add_space(4.0);
                Frame::NONE
                    .fill(colors.field)
                    .stroke(Stroke::new(1.0, colors.border))
                    .corner_radius(4.0)
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_sized([78.0, 26.0], egui::Button::new("Choose…"))
                                .clicked()
                            {
                                self.new_project_folder_requested = true;
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.new_project_parent.display().to_string())
                                        .size(TYPE.secondary)
                                        .color(colors.secondary_text),
                                )
                                .truncate(),
                            )
                            .on_hover_text(self.new_project_parent.display().to_string());
                        });
                    });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "A new folder is created from the title. Existing projects are never overwritten.",
                    )
                    .size(TYPE.meta)
                    .color(colors.muted),
                );
                if let Some(error) = &self.new_project_error {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(error)
                            .size(TYPE.secondary)
                            .color(colors.axis_x),
                    );
                }
                ui.add_space(20.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_sized([82.0, 28.0], egui::Button::new("Cancel"))
                        .clicked()
                    {
                        close_requested = true;
                    }
                    let enabled = !self.new_project_title.trim().is_empty();
                    let button_text = if ui.visuals().dark_mode {
                        colors.surface_deep
                    } else {
                        Color32::WHITE
                    };
                    let create = ui.add_enabled_ui(enabled, |ui| {
                        ui.add_sized(
                            [110.0, 28.0],
                            egui::Button::new(
                                RichText::new("Create project")
                                    .font(medium_font(TYPE.secondary))
                                    .color(button_text),
                            )
                            .fill(colors.accent)
                            .stroke(Stroke::NONE)
                            .corner_radius(4.0),
                        )
                    });
                    let enter_pressed = title_response.has_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if create.inner.clicked() || (enabled && enter_pressed) {
                        self.new_project_create_requested = true;
                        close_requested = true;
                    }
                });
            });
        let escape_pressed = response.is_top_modal
            && !response.any_popup_open
            && context
                .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if close_requested || escape_pressed {
            self.new_project_dialog_open = false;
        }
    }
}
