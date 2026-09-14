use super::*;
impl StudioShell {
    pub(crate) fn show_codex_chat(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::right("codex_chat_panel")
            .resizable(true)
            .default_size(360.0)
            .size_range(280.0..=480.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Codex chat")
                                .font(semibold_font(TYPE.primary))
                                .color(colors.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .button(RichText::new("×").size(TYPE.primary))
                                .on_hover_text("Close Codex chat")
                                .clicked()
                            {
                                self.codex_chat_open = false;
                            }
                        });
                    });
                    let project_root = self.codex_project_root.display().to_string();
                    ui.label(
                        RichText::new(format!("Working in {project_root}"))
                            .size(TYPE.meta)
                            .color(colors.muted),
                    )
                    .on_hover_text(project_root);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Model").size(TYPE.meta).color(colors.muted));
                        let selected_model = self.codex_chat_model;
                        egui::ComboBox::from_id_salt("codex_chat_model")
                            .selected_text(codex_chat_model_label(selected_model, selected_model))
                            .width(156.0)
                            .show_ui(ui, |ui| {
                                for &(model, description) in &CODEX_CHAT_MODELS {
                                    ui.selectable_value(
                                        &mut self.codex_chat_model,
                                        model,
                                        codex_chat_model_label(model, selected_model),
                                    )
                                    .on_hover_text(description);
                                }
                            });
                        ui.label(RichText::new("Level").size(TYPE.meta).color(colors.muted));
                        egui::ComboBox::from_id_salt("codex_chat_reasoning_effort")
                            .selected_text(self.codex_chat_reasoning_effort)
                            .width(72.0)
                            .show_ui(ui, |ui| {
                                for &(effort, description) in &CODEX_CHAT_EFFORTS {
                                    ui.selectable_value(
                                        &mut self.codex_chat_reasoning_effort,
                                        effort,
                                        effort,
                                    )
                                    .on_hover_text(description);
                                }
                            });
                    });
                    ui.label(
                        RichText::new(codex_chat_model_description(self.codex_chat_model))
                            .size(TYPE.meta)
                            .color(colors.muted),
                    );
                    ui.separator();

                    // Keep the composer in the panel's visible region. With
                    // `auto_shrink(false)`, an unconstrained scroll area
                    // consumes all remaining height and lays the composer
                    // out below the panel clip rect.
                    let reserved_chat_controls_height = 150.0;
                    let messages_height =
                        (ui.available_height() - reserved_chat_controls_height).max(64.0);
                    egui::ScrollArea::vertical()
                        .id_salt("codex_chat_messages")
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .max_height(messages_height)
                        .show(ui, |ui| {
                            if self.codex_chat_messages.is_empty() {
                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new(
                                        "Describe a code change. Codex applies it, then Studio rebuilds the preview.",
                                    )
                                    .size(TYPE.secondary)
                                    .color(colors.secondary_text),
                                );
                            }
                            for message in &self.codex_chat_messages {
                                let (label, color) = match message.role {
                                    CodexChatRole::User => ("You", colors.accent),
                                    CodexChatRole::Assistant => ("Codex", colors.secondary_text),
                                };
                                ui.label(
                                    RichText::new(label)
                                        .font(semibold_font(TYPE.meta))
                                        .color(color),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&message.text)
                                            .size(TYPE.secondary)
                                            .color(colors.text),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(12.0);
                            }
                            if self.codex_activity.is_active() {
                                self.advance_codex_live_activity();
                                ui.label(
                                    RichText::new("Codex")
                                        .font(semibold_font(TYPE.meta))
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Spinner::new()
                                            .size(16.0)
                                            .color(colors.accent),
                                    );
                                    ui.label(
                                        RichText::new(match self.codex_activity {
                                            CodexActivity::Rebuilding => "Rebuilding preview",
                                            CodexActivity::Cancelling => "Stopping Codex",
                                            _ => "Codex is working",
                                        })
                                            .font(semibold_font(TYPE.secondary))
                                            .color(colors.text),
                                    );
                                    if self.codex_activity.is_cancellable()
                                        && ui.small_button("Cancel").clicked()
                                    {
                                        self.set_codex_chat_cancelling();
                                    }
                                });
                                if self.codex_live_update_count > 0 {
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "Live activity  ·  {} updates",
                                            self.codex_live_update_count
                                        ))
                                            .size(TYPE.meta)
                                            .color(colors.faint),
                                    );
                                    let excerpt = if self.codex_live_excerpt.is_empty() {
                                        "Receiving the requested change…"
                                    } else {
                                        &self.codex_live_excerpt
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(excerpt)
                                                .size(TYPE.meta)
                                                .color(colors.secondary_text),
                                        )
                                        .wrap(),
                                    );
                                }
                                ui.add_space(12.0);
                                ui.ctx().request_repaint_after(Duration::from_millis(16));
                            }
                            if let Some(error) = &self.codex_chat_error {
                                ui.label(
                                    RichText::new(error).size(TYPE.meta).color(colors.axis_x),
                                );
                                ui.add_space(12.0);
                            }
                        });

                    if !self.codex_chat_ready {
                        ui.label(
                            RichText::new("Starting Codex…")
                                .size(TYPE.meta)
                            .color(colors.muted),
                        );
                    }
                    if self.codex_source_change_count > 0 || self.codex_change_files.is_some() {
                        let files = self.codex_change_files.clone().unwrap_or_default();
                        let changed_count = if self.codex_source_change_count > 0 {
                            self.codex_source_change_count
                        } else {
                            files.len()
                        };
                        let changed_label = if self.codex_source_change_count > 0 {
                            format!(
                                "Codex changed {} source file{}",
                                changed_count,
                                if changed_count == 1 { "" } else { "s" }
                            )
                        } else {
                            format!(
                                "Changed {} file{}",
                                changed_count,
                                if changed_count == 1 { "" } else { "s" }
                            )
                        };
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(changed_label)
                                .font(semibold_font(TYPE.meta))
                                .color(colors.text),
                            );
                            if !files.is_empty()
                                && ui
                                    .button(if self.codex_change_review_open {
                                        "Hide files"
                                    } else {
                                        "Show files"
                                    })
                                    .clicked()
                            {
                                self.codex_change_review_open = !self.codex_change_review_open;
                            }
                            let undo_enabled =
                                self.project_loading.is_none() && !self.codex_activity.is_active();
                            if ui
                                .add_enabled(undo_enabled, egui::Button::new("Undo this change"))
                                .on_disabled_hover_text("Undo is available after the rebuild finishes")
                                .clicked()
                            {
                                self.codex_undo_requested = true;
                            }
                        });
                        if self.codex_change_review_open {
                            Frame::NONE
                                .fill(colors.surface)
                                .inner_margin(Margin::symmetric(8, 5))
                                .show(ui, |ui| {
                                    for file in &files {
                                        ui.label(
                                            RichText::new(file)
                                                .size(TYPE.meta)
                                                .color(colors.secondary_text),
                                        );
                                    }
                                });
                        }
                    }
                    ui.separator();
                    let can_send = self.codex_chat_ready
                        && !self.codex_activity.is_active()
                        && self.chatgpt_account.is_some()
                        && !self.project_dirty
                        && self.project_loading.is_none();
                    let mut submit_requested = false;
                    ui.add_enabled_ui(can_send, |ui| {
                        let draft_response = ui.add(
                            egui::TextEdit::multiline(&mut self.codex_chat_draft)
                                .desired_rows(3)
                                .hint_text("Ask Codex to make a change…")
                                .desired_width(f32::INFINITY),
                        );
                        let enter_pressed = draft_response.has_focus()
                            && ui.input(|input| {
                                input.key_pressed(egui::Key::Enter) && !input.modifiers.shift
                            });
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if self.project_dirty {
                                    "Save scene changes before asking Codex to edit code"
                                } else {
                                    "Codex edits code; Studio rebuilds when it finishes"
                                })
                                    .size(TYPE.meta)
                                    .color(colors.muted),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add_sized([68.0, 28.0], egui::Button::new("Send"))
                                    .clicked()
                                {
                                    submit_requested = true;
                                }
                            });
                        });
                        if enter_pressed {
                            submit_requested = true;
                        }
                    });
                    if submit_requested {
                        self.submit_codex_chat();
                    }
                });
            });
    }
}
