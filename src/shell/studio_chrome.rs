use super::*;
impl StudioShell {
    pub(crate) fn show_top_bar(&mut self, root: &mut egui::Ui, project_name: &str) {
        let colors = palette(root);
        egui::Panel::top("studio_top_bar")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(editor_frame(colors.surface).inner_margin(Margin::symmetric(8, 0)))
            .show(root, |ui| {
                egui::MenuBar::new().style(menu_bar_style).ui(ui, |ui| {
                    ui.add(
                        egui::Image::from_texture(&self.logo_texture)
                            .fit_to_exact_size(egui::vec2(20.0, 20.0))
                            .sense(Sense::hover()),
                    );

                    #[cfg(not(target_os = "macos"))]
                    {
                        ui.menu_button(RichText::new("File").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            if menu_entry(ui, Icon::Plus, "New Project…", "Ctrl+N", true).clicked()
                            {
                                self.execute_command(StudioCommand::NewProject);
                                ui.close();
                            }
                            if menu_entry(ui, Icon::Open, "Open Project…", "Ctrl+O", true).clicked()
                            {
                                self.execute_command(StudioCommand::OpenProject);
                                ui.close();
                            }
                            if menu_entry(ui, Icon::Save, "Save", "Ctrl+S", true).clicked() {
                                self.execute_command(StudioCommand::Save);
                                ui.close();
                            }
                        });
                        ui.menu_button(RichText::new("Edit").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            ui.label(
                                RichText::new("Undo and redo are not available yet")
                                    .size(TYPE.meta)
                                    .color(palette(ui).muted),
                            );
                        });
                        ui.menu_button(RichText::new("Window").size(TYPE.primary), |ui| {
                            ui.set_min_width(220.0);
                            if menu_entry(ui, Icon::Stop, "Close Window", "", true).clicked() {
                                self.execute_command(StudioCommand::CloseWindow);
                                ui.close();
                            }
                        });
                    }

                    if !self.start_screen {
                        ui.add_space(8.0);
                        for workspace in Workspace::ALL {
                            if workspace_tab(ui, workspace.label(), self.workspace == workspace)
                                .clicked()
                            {
                                self.execute_command(workspace.command());
                            }
                        }
                    }

                    if !self.start_screen {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if self.auth_pending {
                                ui.label(
                                    RichText::new("Signing in…")
                                        .size(TYPE.meta)
                                        .color(colors.muted),
                                );
                            } else if let Some(user) = &self.auth_user {
                                ui.label(
                                    RichText::new(&user.name)
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                            } else if toolbar_button(ui, Icon::Character, "Sign in", false)
                                .clicked()
                            {
                                self.auth_requested = true;
                            }
                            vertical_separator(ui, 14.0);
                            self.show_chatgpt_control(ui, colors);
                            let play_label = if self.playing { "Stop" } else { "Play" };
                            let play_width = toolbar_button_width(ui, play_label);
                            if ui.available_width() >= play_width + 48.0 {
                                let live = ui.allocate_response(
                                    egui::vec2(40.0, CONTROL_HEIGHT),
                                    Sense::hover(),
                                );
                                paint_status_label(ui, live.rect, colors.live, "Live");
                            }
                            let play_icon = if self.playing { Icon::Stop } else { Icon::Play };
                            if toolbar_button(ui, play_icon, play_label, self.playing).clicked() {
                                if self.playing {
                                    self.playing = false;
                                    self.notice = "Play session stopped".to_owned();
                                } else {
                                    self.playing = true;
                                    self.restart_requested = true;
                                    self.notice = "Restarting preview…".to_owned();
                                }
                            }
                            if self.project_editable
                                && toolbar_button(ui, Icon::Save, "Save", self.project_dirty)
                                    .clicked()
                            {
                                self.execute_command(StudioCommand::Save);
                            }
                            if self.project_editable
                                && toolbar_button(
                                    ui,
                                    Icon::Play,
                                    "Rebuild & Play",
                                    self.preview_stale,
                                )
                                .clicked()
                            {
                                self.request_rebuild_and_play();
                            }
                            if self.project_editable
                                && toolbar_button(ui, Icon::Play, "Restart", false).clicked()
                            {
                                self.restart_requested = true;
                                self.notice = "Restarting preview…".to_owned();
                            }
                            let project_width = (ui.available_width() - 17.0).min(180.0);
                            if project_width >= 72.0 {
                                vertical_separator(ui, 14.0);
                                ui.add_sized(
                                    [project_width, CONTROL_HEIGHT],
                                    egui::Label::new(
                                        RichText::new(project_name)
                                            .size(TYPE.secondary)
                                            .color(colors.secondary_text),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(project_name);
                            }
                        });
                    }
                });
            });
    }

    pub(crate) fn show_chatgpt_control(&mut self, ui: &mut egui::Ui, colors: Palette) {
        if self.chatgpt_pending {
            toolbar_status(ui, Icon::Sparkles, "Connecting ChatGPT…", colors.muted)
                .on_hover_text("Finish signing in with ChatGPT in your browser");
            return;
        }

        if let Some(account) = &self.chatgpt_account {
            let label = chatgpt_account_label(account);
            let tooltip = account
                .email
                .as_deref()
                .map(|email| format!("ChatGPT connected as {email}"))
                .unwrap_or_else(|| "ChatGPT connected".to_owned());
            if toolbar_button(ui, Icon::Sparkles, &label, self.codex_chat_open)
                .on_hover_text(format!("{tooltip}. Open Codex chat"))
                .clicked()
            {
                self.codex_chat_open = true;
                self.codex_chat_open_requested = true;
                self.codex_chat_error = None;
            }
            return;
        }

        if self.chatgpt_available {
            if toolbar_button(ui, Icon::Sparkles, "Connect ChatGPT", false)
                .on_hover_text("Use your ChatGPT subscription with Codex in Studio")
                .clicked()
            {
                self.chatgpt_auth_requested = true;
                self.set_chatgpt_pending();
            }
            return;
        }

        toolbar_status(ui, Icon::Sparkles, "ChatGPT unavailable", colors.faint).on_hover_text(
            self.chatgpt_error
                .as_deref()
                .unwrap_or("Codex App Server is unavailable"),
        );
    }

    pub(crate) fn show_status_bar(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::bottom("studio_status_bar")
            .exact_size(STATUS_BAR_HEIGHT)
            .frame(editor_frame(colors.panel_raised).inner_margin(Margin::symmetric(8, 0)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(STATUS_BAR_HEIGHT);
                    ui.spacing_mut().interact_size.y = 16.0;
                    let status_color = if self.project_error.is_some() {
                        colors.axis_x
                    } else if self.project_dirty || self.preview_stale {
                        colors.accent
                    } else {
                        colors.muted
                    };
                    inline_icon(
                        ui,
                        if self.project_error.is_some() {
                            Icon::Stop
                        } else {
                            Icon::Check
                        },
                        status_color,
                    );
                    let notice_width = (ui.available_width() - 124.0).max(40.0);
                    ui.add_sized(
                        [notice_width, 16.0],
                        egui::Label::new(
                            RichText::new(&self.notice)
                                .size(TYPE.meta)
                                .color(status_color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.notice);
                });
            });
    }
}
