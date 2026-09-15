use super::*;
impl StudioShell {
    pub(crate) fn scene_tree(&mut self, ui: &mut egui::Ui) {
        if self.project_loading.is_some() {
            return;
        }
        let previous_selection = self.selected_scene.clone();
        panel_header(ui, Icon::World, "Scene", |ui| {
            if self.project_editable {
                if ui.button("Add block").clicked() {
                    self.scene_edit_requested = Some(SceneEditRequest::AddBlock);
                    self.notice = "Adding platform…".to_owned();
                }
            } else {
                ui.label(
                    RichText::new("Read-only")
                        .size(TYPE.meta)
                        .color(palette(ui).muted),
                )
                .on_hover_text("Open a raw source project to edit the scene");
            }
        });
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::NONE
                    .inner_margin(Margin::symmetric(0, 4))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        show_scene_node(
                            ui,
                            &self.scene_outline.root,
                            0,
                            &mut self.expanded_scene,
                            &mut self.selected_scene,
                        );
                    });
            });
        if self.selected_scene != previous_selection
            && let Some(selected) = self.scene_outline.root.find(&self.selected_scene)
        {
            self.notice = format!("Selected {}", selected.label);
        }
    }

    pub(crate) fn inspector(&mut self, ui: &mut egui::Ui) {
        panel_header(ui, Icon::Sliders, "Inspector", |_| {});
        let selected = self
            .scene_outline
            .root
            .find(&self.selected_scene)
            .cloned()
            .unwrap_or_else(|| self.scene_outline.root.clone());
        content_frame().show(ui, |ui| {
            selected_object_header(ui, &selected.label, selected.kind);
            ui.add_space(2.0);
            if selected.kind == "Block" {
                self.block_inspector(ui, &selected);
            } else if selected.kind == "Sign" {
                self.sign_inspector(ui, &selected);
            } else if selected.properties.is_empty() {
                ui.label(
                    RichText::new(if selected.kind == "Collection" {
                        "Select an item in this group to inspect it."
                    } else {
                        "No editable properties"
                    })
                    .size(TYPE.secondary)
                    .color(palette(ui).muted),
                );
            } else {
                property_section(ui, "Manifest", |ui| {
                    for (label, value) in &selected.properties {
                        property_row(ui, label, value);
                    }
                });
                ui.label(
                    RichText::new("Manifest data is read-only in Studio.")
                        .size(TYPE.meta)
                        .color(palette(ui).muted),
                );
            }
        });
    }

    pub(crate) fn block_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        if self.scene_editor_target != selected.id {
            self.scene_editor_target = selected.id.clone();
            self.scene_editor_position = vector_property(selected, "Position").unwrap_or([0.0; 3]);
            self.scene_editor_size = vector_property(selected, "Size").unwrap_or([1.0; 3]);
        }

        property_section(ui, "Transform", |ui| {
            let mut position_changed = false;
            let mut size_changed = false;
            ui.add_enabled_ui(self.project_editable, |ui| {
                position_changed =
                    vector_editor(ui, "Position", &mut self.scene_editor_position, 0.1);
                size_changed = vector_editor(ui, "Size", &mut self.scene_editor_size, 0.1);
            });
            if position_changed || size_changed {
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = Some(SceneEditRequest::UpdateBlock {
                    target: selected.id.clone(),
                    position: self.scene_editor_position,
                    size: self.scene_editor_size,
                });
                self.notice = "Platform changed — save to keep it".to_owned();
            }
        });
        property_section(ui, "Actions", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.project_editable, egui::Button::new("Duplicate"))
                    .on_disabled_hover_text("Open a raw source project to edit the scene")
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DuplicateBlock {
                        target: selected.id.clone(),
                    });
                    self.notice = "Duplicating platform…".to_owned();
                }
                if ui
                    .add_enabled(self.project_editable, egui::Button::new("Delete"))
                    .on_disabled_hover_text("Open a raw source project to edit the scene")
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DeleteBlock {
                        target: selected.id.clone(),
                    });
                    self.notice = "Deleting platform…".to_owned();
                }
            });
        });
        property_section(ui, "Manifest", |ui| {
            for (label, value) in &selected.properties {
                if label != "Position" && label != "Size" {
                    property_row(ui, label, value);
                }
            }
        });
        if !self.project_editable {
            ui.label(
                RichText::new("Open a raw source project to edit scene objects.")
                    .size(TYPE.meta)
                    .color(colors.muted),
            );
        }
    }

    pub(crate) fn sign_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        if self.scene_editor_target != selected.id {
            self.scene_editor_target = selected.id.clone();
            self.scene_editor_text = selected
                .properties
                .iter()
                .find(|(label, _)| label == "Text")
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
        }

        property_section(ui, "Content", |ui| {
            ui.label(
                RichText::new("Text")
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
            );
            if ui
                .add_enabled(
                    self.project_editable,
                    egui::TextEdit::multiline(&mut self.scene_editor_text)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text("Sign text"),
                )
                .changed()
            {
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = Some(SceneEditRequest::UpdateSignText {
                    target: selected.id.clone(),
                    text: self.scene_editor_text.clone(),
                });
                self.notice = "Sign text changed — save to keep it".to_owned();
            }
        });
        ui.label(
            RichText::new(if self.project_editable {
                "Save, then Rebuild & Play to see the updated sign in the game."
            } else {
                "Open a raw source project to edit this sign."
            })
            .size(TYPE.meta)
            .color(colors.muted),
        );
    }
}
