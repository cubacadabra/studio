use super::*;
impl StudioShell {
    pub(crate) fn scene_tree(&mut self, ui: &mut egui::Ui) {
        if self.project_loading.is_some() {
            return;
        }
        let previous_selection = self.selected_scene.clone();
        panel_header(ui, Icon::World, "Scene", |ui| {
            if self.project_editable {
                let world_id = scene_world_id(&self.selected_scene).map(str::to_owned);
                scene_add_menu(
                    ui,
                    world_id,
                    self.authoring_scene_source.is_some(),
                    &mut self.scene_edit_requested,
                );
            } else {
                ui.label(
                    RichText::new("Read-only")
                        .size(TYPE.meta)
                        .color(palette(ui).muted),
                )
                .on_hover_text("Open a raw source project to edit the scene");
            }
        });
        search_field_with_hint(
            ui,
            &mut self.scene_search_query,
            ui.available_width(),
            "Filter scene…",
            "Filter scene tree",
        );
        ui.add_space(4.0);
        if self.scene_search_matches_query != self.scene_search_query {
            self.scene_search_matches_query = self.scene_search_query.clone();
            self.scene_search_matches = self.scene_outline.search_matches(&self.scene_search_query);
            self.scene_search_result_count = self
                .scene_outline
                .search_result_count(&self.scene_search_query);
            self.scene_tree_rows_dirty = true;
        }
        let scene_filter = self.scene_search_query.trim();
        if !scene_filter.is_empty() {
            let result_count = self.scene_search_result_count;
            let label = if result_count == 1 {
                "1 matching item"
            } else {
                "matching items"
            };
            ui.label(
                RichText::new(if result_count == 1 {
                    label.to_owned()
                } else {
                    format!("{result_count} {label}")
                })
                .size(TYPE.meta)
                .color(palette(ui).muted),
            );
            ui.add_space(2.0);
        }
        if self.scene_tree_rows_dirty {
            self.scene_tree_rows.clear();
            flatten_scene_rows(
                &self.scene_outline.root,
                0,
                &self.expanded_scene,
                (!scene_filter.is_empty()).then_some(&self.scene_search_matches),
                &mut self.scene_tree_rows,
            );
            self.scene_tree_rows_dirty = false;
        }
        let expanded_before = self.expanded_scene.clone();
        if self.scene_tree_rows.is_empty() {
            ui.add_space(12.0);
            ui.label(
                RichText::new("No scene items match this filter.")
                    .size(TYPE.secondary)
                    .color(palette(ui).muted),
            );
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, UI.row, self.scene_tree_rows.len(), |ui, row_range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.spacing_mut().interact_size.y = UI.row;
                    for index in row_range {
                        let row = self.scene_tree_rows[index].clone();
                        show_scene_row(
                            ui,
                            &row,
                            &mut self.expanded_scene,
                            &mut self.selected_scene,
                            self.project_editable,
                            &mut self.scene_edit_requested,
                        );
                    }
                });
        }
        if self.expanded_scene != expanded_before {
            self.scene_tree_rows_dirty = true;
        }
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
            } else if self.project_editable
                && (is_scene_object(&selected.id) || is_authoring_node(&selected))
                && vector_property(&selected, "Position").is_some()
            {
                self.scene_object_inspector(ui, &selected);
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
            if self.project_editable && is_scene_object(&selected.id) {
                self.scene_object_actions(ui, &selected);
            }
        });
    }

    pub(crate) fn block_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        self.sync_scene_editor(selected);
        self.scene_transform_editor(ui, selected, true, false, false);
        self.scene_manifest_properties(ui, selected, &["Position", "Size"]);
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
        self.sync_scene_editor(selected);
        self.scene_transform_editor(ui, selected, false, false, false);

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
        self.scene_manifest_properties(ui, selected, &["Position", "Text"]);
        ui.label(
            RichText::new(if self.project_editable {
                "Press Play to see the updated sign in the game."
            } else {
                "Open a raw source project to edit this sign."
            })
            .size(TYPE.meta)
            .color(colors.muted),
        );
    }

    fn scene_object_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        self.sync_scene_editor(selected);
        let authoring_node = is_authoring_node(selected);
        let has_primitive_size = selected.kind == "Block" && authoring_node;
        let has_size =
            vector_property(selected, "Size").is_some() && (!authoring_node || has_primitive_size);
        let has_scale = authoring_node && !has_primitive_size;
        self.scene_transform_editor(ui, selected, has_size, has_scale, has_primitive_size);
        self.scene_manifest_properties(ui, selected, &["Position", "Scale"]);
    }

    fn sync_scene_editor(&mut self, selected: &SceneNode) {
        if self.scene_editor_target == selected.id {
            return;
        }
        self.scene_editor_target = selected.id.clone();
        self.scene_editor_position = vector_property(selected, "Position").unwrap_or([0.0; 3]);
        self.scene_editor_size = vector_property(selected, "Size").unwrap_or([1.0; 3]);
        self.scene_editor_scale = vector_property(selected, "Scale").unwrap_or([1.0; 3]);
        self.scene_editor_position_text = scene_vector_text(self.scene_editor_position);
        self.scene_editor_size_text = scene_vector_text(self.scene_editor_size);
        self.scene_editor_scale_text = scene_vector_text(self.scene_editor_scale);
        self.scene_editor_text = selected
            .properties
            .iter()
            .find(|(label, _)| label == "Text")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        self.scene_editor_properties = selected.properties.iter().cloned().collect();
    }

    fn scene_transform_editor(
        &mut self,
        ui: &mut egui::Ui,
        selected: &SceneNode,
        has_size: bool,
        has_scale: bool,
        has_primitive_size: bool,
    ) {
        property_section(ui, "Transform", |ui| {
            let mut position_changed = false;
            let mut size_changed = false;
            let mut scale_changed = false;
            ui.add_enabled_ui(
                self.project_editable && !self.playing && !scene_node_locked(selected),
                |ui| {
                    position_changed |= vector_editor(
                        ui,
                        "Position",
                        &mut self.scene_editor_position,
                        &mut self.scene_editor_position_text,
                    );
                    if has_size {
                        size_changed |= vector_editor(
                            ui,
                            "Size",
                            &mut self.scene_editor_size,
                            &mut self.scene_editor_size_text,
                        );
                    }
                    if has_scale {
                        scale_changed |= vector_editor(
                            ui,
                            "Scale",
                            &mut self.scene_editor_scale,
                            &mut self.scene_editor_scale_text,
                        );
                    }
                },
            );
            if scene_node_locked(selected) {
                ui.label(
                    RichText::new(
                        selected
                            .properties
                            .iter()
                            .find(|(label, _)| label == "Lock reason")
                            .map(|(_, reason)| reason.as_str())
                            .unwrap_or("This imported node is read-only."),
                    )
                    .size(TYPE.meta)
                    .color(palette(ui).muted),
                );
            } else if self.playing {
                ui.label(
                    RichText::new("Stop Play to edit the authoring scene.")
                        .size(TYPE.meta)
                        .color(palette(ui).muted),
                );
            }
            if position_changed || size_changed || scale_changed {
                if has_size {
                    self.scene_editor_size = self.scene_editor_size.map(|value| value.max(0.05));
                    self.scene_editor_size_text = scene_vector_text(self.scene_editor_size);
                }
                if has_scale {
                    self.scene_editor_scale = self.scene_editor_scale.map(|value| value.max(0.05));
                    self.scene_editor_scale_text = scene_vector_text(self.scene_editor_scale);
                }
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = if has_primitive_size && size_changed {
                    Some(SceneEditRequest::SetPrimitiveSize {
                        target: selected.id.clone(),
                        position: self.scene_editor_position,
                        size: self.scene_editor_size,
                    })
                } else {
                    Some(SceneEditRequest::SetTransform {
                        target: selected.id.clone(),
                        position: self.scene_editor_position,
                        scale: has_scale.then_some(self.scene_editor_scale),
                    })
                };
                self.notice = "Scene object changed — save to keep it".to_owned();
            }
        });
    }

    fn scene_object_actions(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        property_section(ui, "Actions", |ui| {
            ui.horizontal(|ui| {
                if ui.button("Duplicate").clicked() {
                    self.scene_edit_requested = Some(SceneEditRequest::DuplicateObject {
                        target: selected.id.clone(),
                    });
                    self.notice = "Duplicating scene object…".to_owned();
                }
                if ui.button("Delete").clicked() {
                    self.scene_edit_requested = Some(SceneEditRequest::DeleteObject {
                        target: selected.id.clone(),
                    });
                    self.notice = "Deleting scene object…".to_owned();
                }
            });
        });
    }

    fn scene_manifest_properties(
        &mut self,
        ui: &mut egui::Ui,
        selected: &SceneNode,
        excluded: &[&str],
    ) {
        let properties = selected
            .properties
            .iter()
            .filter(|(label, _)| !excluded.contains(&label.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if properties.is_empty() {
            return;
        }
        property_section(ui, "Properties", |ui| {
            for (index, (label, value)) in properties.into_iter().enumerate() {
                if self.project_editable
                    && let Some((key, kind)) = editable_scene_property(&label)
                {
                    self.scene_property_editor(ui, selected, &label, &value, key, kind, index);
                } else {
                    property_row(ui, &label, &value);
                }
            }
        });
    }

    fn scene_property_editor(
        &mut self,
        ui: &mut egui::Ui,
        selected: &SceneNode,
        label: &str,
        current: &str,
        key: &str,
        kind: ScenePropertyKind,
        index: usize,
    ) {
        let field = property_field(ui, label);
        let mut text = self
            .scene_editor_properties
            .remove(label)
            .unwrap_or_else(|| current.to_owned());
        let response = ui.put(
            field,
            egui::TextEdit::singleline(&mut text)
                .id_salt(("scene-property", &selected.id, key, index))
                .horizontal_align(Align::RIGHT),
        );
        let commit = response.lost_focus() && text.trim() != current;
        self.scene_editor_properties
            .insert(label.to_owned(), text.clone());
        if !commit {
            return;
        }
        let value = match kind.parse(&text) {
            Ok(value) => value,
            Err(message) => {
                self.notice = format!("{label}: {message}");
                return;
            }
        };
        self.project_dirty = true;
        self.project_error = None;
        self.scene_edit_requested = Some(SceneEditRequest::UpdateProperty {
            target: selected.id.clone(),
            key: key.to_owned(),
            value,
        });
        self.notice = format!("{label} changed — save to keep it");
    }
}

#[derive(Clone, Copy)]
enum ScenePropertyKind {
    Text,
    Number,
    Boolean,
}

impl ScenePropertyKind {
    fn parse(self, source: &str) -> Result<Value, &'static str> {
        let source = source.trim();
        match self {
            Self::Text if !source.is_empty() => Ok(Value::String(source.to_owned())),
            Self::Text => Err("enter a value"),
            Self::Number => source
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(Value::from)
                .ok_or("enter a valid number"),
            Self::Boolean
                if source.eq_ignore_ascii_case("yes") || source.eq_ignore_ascii_case("true") =>
            {
                Ok(Value::Bool(true))
            }
            Self::Boolean
                if source.eq_ignore_ascii_case("no") || source.eq_ignore_ascii_case("false") =>
            {
                Ok(Value::Bool(false))
            }
            Self::Boolean => Err("enter Yes or No"),
        }
    }
}

fn editable_scene_property(label: &str) -> Option<(&'static str, ScenePropertyKind)> {
    Some(match label {
        "Id" => ("id", ScenePropertyKind::Text),
        "Label" => ("label", ScenePropertyKind::Text),
        "Kind" => ("kind", ScenePropertyKind::Text),
        "Color" => ("color", ScenePropertyKind::Text),
        "Material" => ("material", ScenePropertyKind::Text),
        "Visual" => ("visual", ScenePropertyKind::Text),
        "Climb Axis" => ("climbAxis", ScenePropertyKind::Text),
        "Radius" => ("radius", ScenePropertyKind::Number),
        "Yaw" => ("yaw", ScenePropertyKind::Number),
        "Max Width" => ("maxWidth", ScenePropertyKind::Number),
        "Climb Speed" => ("climbSpeed", ScenePropertyKind::Number),
        "Damage Per Second" => ("damagePerSecond", ScenePropertyKind::Number),
        "Heal Per Second" => ("healPerSecond", ScenePropertyKind::Number),
        "Outline" => ("outline", ScenePropertyKind::Boolean),
        "Framed" => ("framed", ScenePropertyKind::Boolean),
        _ => return None,
    })
}

fn scene_add_menu(
    ui: &mut egui::Ui,
    world_id: Option<String>,
    _component_scene: bool,
    request: &mut Option<SceneEditRequest>,
) {
    ui.menu_button("Add", |ui| {
        for kind in SceneObjectKind::ALL {
            if ui.button(kind.label()).clicked() {
                *request = Some(SceneEditRequest::AddObject {
                    world_id: world_id.clone(),
                    kind,
                });
                ui.close();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_property_values_keep_their_manifest_types() {
        assert_eq!(
            ScenePropertyKind::Number.parse("3.5").unwrap(),
            serde_json::json!(3.5)
        );
        assert_eq!(
            ScenePropertyKind::Boolean.parse("Yes").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            ScenePropertyKind::Text.parse("signal").unwrap(),
            Value::String("signal".to_owned())
        );
        assert!(ScenePropertyKind::Number.parse("wide").is_err());
    }
}
