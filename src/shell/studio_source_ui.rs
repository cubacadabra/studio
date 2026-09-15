use super::*;

impl StudioShell {
    pub(crate) fn set_source_files(&mut self, files: BTreeMap<PathBuf, String>) {
        let selected = self
            .selected_source_file
            .clone()
            .filter(|path| files.contains_key(path))
            .or_else(|| files.keys().next().cloned());
        self.source_files = files;
        self.selected_source_file = None;
        if let Some(path) = selected {
            self.select_source_file(path);
        } else {
            self.source_editor_text.clear();
        }
    }

    pub(crate) fn source_files_for_save(&self) -> Vec<(PathBuf, String)> {
        self.source_files
            .iter()
            .map(|(path, source)| (path.clone(), source.clone()))
            .collect()
    }

    pub(crate) fn mark_source_files_saved(&mut self) {
        self.project_dirty = false;
    }

    fn select_source_file(&mut self, path: PathBuf) {
        let Some(source) = self.source_files.get(&path).cloned() else {
            return;
        };
        self.selected_source_file = Some(path);
        self.source_editor_text = source;
    }

    pub(crate) fn show_scripts(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::left("source_files")
            .resizable(true)
            .default_size(224.0)
            .size_range(180.0..=360.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                panel_header(ui, Icon::Logs, "Files", |ui| {
                    ui.label(
                        RichText::new(format!("{}", self.source_files.len()))
                            .size(TYPE.meta)
                            .color(colors.muted),
                    );
                });
                content_frame().show(ui, |ui| {
                    if self.source_files.is_empty() {
                        ui.label(
                            RichText::new("No manifest or Luau files found.")
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                        return;
                    }
                    let mut selected = None;
                    egui::ScrollArea::vertical()
                        .id_salt("source_file_list")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for path in self.source_files.keys() {
                                let is_manifest = path == Path::new("manifest.json");
                                let icon = if is_manifest {
                                    Icon::Folder
                                } else {
                                    Icon::Logs
                                };
                                let label = path.to_string_lossy();
                                if navigation_row(
                                    ui,
                                    icon,
                                    &label,
                                    self.selected_source_file.as_ref() == Some(path),
                                    false,
                                )
                                .clicked()
                                {
                                    selected = Some(path.clone());
                                }
                            }
                        });
                    if let Some(path) = selected {
                        self.select_source_file(path);
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(editor_frame(colors.surface))
            .show(root, |ui| {
                let Some(path) = self.selected_source_file.clone() else {
                    panel_header(ui, Icon::Logs, "Source", |_| {});
                    content_frame().show(ui, |ui| {
                        ui.label(
                            RichText::new("Select a source file to start editing.")
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                    });
                    return;
                };

                let label = path.to_string_lossy().into_owned();
                panel_header(ui, Icon::Logs, &label, |ui| {
                    if self.project_dirty && self.project_editable {
                        if toolbar_button(ui, Icon::Save, "Save", true).clicked() {
                            self.execute_command(StudioCommand::Save);
                        }
                        ui.label(
                            RichText::new("Unsaved")
                                .size(TYPE.meta)
                                .color(colors.accent),
                        );
                    }
                });
                Frame::NONE
                    .fill(colors.surface_deep)
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        let code_theme = if ui.visuals().dark_mode {
                            ColorTheme::GITHUB_DARK
                        } else {
                            ColorTheme::GITHUB_LIGHT
                        };
                        let response = ui
                            .scope(|ui| {
                                // The editor's caret should communicate focus, not blink in and
                                // out while the user is typing.
                                ui.visuals_mut().text_cursor.blink = false;
                                ui.visuals_mut().text_cursor.stroke =
                                    Stroke::new(2.0, colors.accent);
                                self.source_editor
                                    .clone()
                                    .id_source(format!("source-editor-{label}"))
                                    .with_fontsize(14.0)
                                    .with_theme(code_theme)
                                    .with_numlines(true)
                                    .with_rows(28)
                                    .vscroll(true)
                                    .show(ui, &mut self.source_editor_text, &self.source_syntax)
                            })
                            .inner;
                        let editor_has_focus = response.response.has_focus();
                        if response.response.changed() {
                            let source = self.source_editor_text.clone();
                            self.source_files.insert(path.clone(), source.clone());
                            self.project_dirty = true;
                            self.preview_stale = true;
                            self.notice = format!("Edited {label}");
                            if path == Path::new("manifest.json") {
                                if !self.set_source_manifest(&source, true) {
                                    self.project_dirty = true;
                                    self.preview_stale = true;
                                }
                            }
                        }
                        if editor_has_focus
                            && ui.input_mut(|input| {
                                input.consume_shortcut(&egui::KeyboardShortcut::new(
                                    egui::Modifiers::COMMAND,
                                    egui::Key::S,
                                ))
                            })
                        {
                            self.execute_command(StudioCommand::Save);
                        }
                    });
            });
    }
}
