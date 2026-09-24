use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddCategory {
    Build,
    Characters,
    Gameplay,
}

impl AddCategory {
    const fn label(self) -> &'static str {
        match self {
            Self::Build => "BUILD",
            Self::Characters => "CHARACTERS",
            Self::Gameplay => "GAMEPLAY",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct AddPaletteItem {
    kind: SceneObjectKind,
    category: AddCategory,
    icon: Icon,
    label: &'static str,
    description: &'static str,
    aliases: &'static [&'static str],
}

const ADD_PALETTE_ITEMS: [AddPaletteItem; 9] = [
    AddPaletteItem {
        kind: SceneObjectKind::Block,
        category: AddCategory::Build,
        icon: Icon::Object,
        label: "Block",
        description: "Solid editable shape",
        aliases: &["cube", "part", "platform", "brick"],
    },
    AddPaletteItem {
        kind: SceneObjectKind::Group,
        category: AddCategory::Build,
        icon: Icon::Folder,
        label: "Group",
        description: "Organize related scene nodes",
        aliases: &["folder", "model", "container", "collection"],
    },
    AddPaletteItem {
        kind: SceneObjectKind::Sign,
        category: AddCategory::Build,
        icon: Icon::Object,
        label: "Sign",
        description: "World-space text",
        aliases: &["text", "label", "message"],
    },
    AddPaletteItem {
        kind: SceneObjectKind::Ladder,
        category: AddCategory::Build,
        icon: Icon::Object,
        label: "Ladder",
        description: "Climbable volume",
        aliases: &["climb", "climbable", "stairs"],
    },
    AddPaletteItem {
        kind: SceneObjectKind::Actor,
        category: AddCategory::Characters,
        icon: Icon::Character,
        label: "Actor",
        description: "Stationary authored character",
        aliases: &["character", "npc", "humanoid", "guide", "rig"],
    },
    AddPaletteItem {
        kind: SceneObjectKind::Interaction,
        category: AddCategory::Gameplay,
        icon: Icon::Object,
        label: "Interaction",
        description: "Trigger an action",
        aliases: &["trigger", "prompt", "action", "zone"],
    },
    AddPaletteItem {
        kind: SceneObjectKind::Checkpoint,
        category: AddCategory::Gameplay,
        icon: Icon::Object,
        label: "Checkpoint",
        description: "Save player progress",
        aliases: &["spawn", "progress", "save"],
    },
    AddPaletteItem {
        kind: SceneObjectKind::Hazard,
        category: AddCategory::Gameplay,
        icon: Icon::Object,
        label: "Hazard",
        description: "Damage players",
        aliases: &["damage", "kill", "danger", "lava"],
    },
    AddPaletteItem {
        kind: SceneObjectKind::SafeZone,
        category: AddCategory::Gameplay,
        icon: Icon::Object,
        label: "Safe Zone",
        description: "Heal and protect players",
        aliases: &["heal", "safety", "protect", "recovery"],
    },
];

pub(crate) struct AddPaletteState {
    query: String,
    selected: usize,
    focus_requested: bool,
    world_id: Option<String>,
}

impl AddPaletteState {
    pub(crate) fn new(world_id: Option<String>) -> Self {
        Self {
            query: String::new(),
            selected: 0,
            focus_requested: true,
            world_id,
        }
    }
}

impl StudioShell {
    pub(crate) fn open_add_palette(&mut self, world_id: Option<String>) -> bool {
        if !self.editor_shortcuts_active() {
            return false;
        }
        self.add_palette = Some(AddPaletteState::new(world_id));
        true
    }

    pub(crate) fn show_add_palette(&mut self, context: &egui::Context) {
        let Some(mut state) = self.add_palette.take() else {
            return;
        };
        if !self.editor_shortcuts_active() {
            return;
        }

        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        let width = (context.content_rect().width() - 32.0).clamp(280.0, 520.0);
        let mut chosen = None;
        let mut close_requested = false;
        let response = egui::Modal::new(egui::Id::new("scene_add_palette"))
            .backdrop_color(Color32::from_black_alpha(96))
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.border_strong))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(12)),
            )
            .show(context, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Add to World")
                            .font(semibold_font(18.0))
                            .color(colors.text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("Shift+A").size(TYPE.meta).color(colors.muted));
                    });
                });
                ui.add_space(8.0);
                let search = search_field_with_hint(
                    ui,
                    &mut state.query,
                    ui.available_width(),
                    "Search objects…",
                    "Search objects to add",
                );
                if std::mem::take(&mut state.focus_requested) {
                    search.request_focus();
                }
                if search.changed() {
                    state.selected = 0;
                }

                let matches = filtered_add_items(&state.query);
                if !matches.is_empty() {
                    if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                        state.selected = (state.selected + 1).min(matches.len() - 1);
                    }
                    if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                        state.selected = state.selected.saturating_sub(1);
                    }
                    state.selected = state.selected.min(matches.len() - 1);
                    if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        chosen = Some(matches[state.selected].kind);
                    }
                } else {
                    state.selected = 0;
                }

                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .id_salt("scene_add_results")
                    .auto_shrink([false, true])
                    .max_height((context.content_rect().height() - 210.0).clamp(132.0, 360.0))
                    .show(ui, |ui| {
                        if matches.is_empty() {
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new("No objects match this search.")
                                    .size(TYPE.secondary)
                                    .color(colors.muted),
                            );
                            ui.add_space(12.0);
                            return;
                        }
                        let mut category = None;
                        for (index, item) in matches.iter().enumerate() {
                            if category != Some(item.category) {
                                if category.is_some() {
                                    ui.add_space(5.0);
                                }
                                category = Some(item.category);
                                ui.label(
                                    RichText::new(item.category.label())
                                        .font(medium_font(TYPE.meta))
                                        .color(colors.muted),
                                );
                                ui.add_space(2.0);
                            }
                            let row = add_palette_row(ui, item, state.selected == index);
                            if row.hovered() {
                                state.selected = index;
                            }
                            if row.clicked() {
                                chosen = Some(item.kind);
                            }
                        }
                    });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(
                    RichText::new("↑↓ Navigate   Enter Add   Esc Close")
                        .size(TYPE.meta)
                        .color(colors.muted),
                );
            });

        if let Some(kind) = chosen {
            let group_targets = (kind == SceneObjectKind::Group)
                .then(|| {
                    self.selected_scenes
                        .iter()
                        .filter_map(|id| self.scene_outline.root.find(id))
                        .filter(|node| {
                            is_authoring_node(node) && scene_parent_id_value(node).is_some()
                        })
                        .map(|node| node.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            self.scene_edit_requested = if group_targets.is_empty() {
                Some(SceneEditRequest::AddObject {
                    world_id: state.world_id.take(),
                    kind,
                })
            } else {
                Some(SceneEditRequest::GroupObjects {
                    targets: group_targets,
                })
            };
            self.notice = format!("Adding {}…", kind.label());
            close_requested = true;
        }
        if !close_requested && !response.should_close() {
            self.add_palette = Some(state);
        }
    }
}

fn filtered_add_items(query: &str) -> Vec<&'static AddPaletteItem> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return ADD_PALETTE_ITEMS.iter().collect();
    }
    let tokens = query.split_whitespace().collect::<Vec<_>>();
    let mut matches = ADD_PALETTE_ITEMS
        .iter()
        .filter(|item| {
            tokens.iter().all(|token| {
                item.label.to_ascii_lowercase().contains(token)
                    || item.description.to_ascii_lowercase().contains(token)
                    || item.aliases.iter().any(|alias| alias.contains(token))
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|item| add_item_score(item, &query));
    matches
}

fn add_item_score(item: &AddPaletteItem, query: &str) -> u8 {
    let label = item.label.to_ascii_lowercase();
    if label == query {
        0
    } else if label.starts_with(query) {
        1
    } else if item.aliases.contains(&query) {
        2
    } else if label.contains(query) {
        3
    } else {
        4
    }
}

fn add_palette_row(ui: &mut egui::Ui, item: &AddPaletteItem, selected: bool) -> egui::Response {
    let colors = palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            selected,
            item.label,
        )
    });
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            UI.radius,
            if selected {
                colors.selection
            } else {
                colors.surface
            },
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(18.0, 0.0),
            Vec2::splat(18.0),
        ),
        item.icon,
        if selected { colors.text } else { colors.muted },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(36.0, -8.0),
        Align2::LEFT_CENTER,
        item.label,
        medium_font(TYPE.primary),
        colors.text,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(36.0, 9.0),
        Align2::LEFT_CENTER,
        item.description,
        FontId::proportional(TYPE.meta),
        colors.muted,
    );
    paint_focus(ui, &response);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_catalog_covers_every_current_scene_object_once() {
        let kinds = ADD_PALETTE_ITEMS
            .iter()
            .map(|item| item.kind)
            .collect::<Vec<_>>();
        assert_eq!(kinds, SceneObjectKind::ALL);
    }

    #[test]
    fn add_search_matches_familiar_aliases() {
        assert_eq!(
            filtered_add_items("platform")[0].kind,
            SceneObjectKind::Block
        );
        assert_eq!(
            filtered_add_items("trigger")[0].kind,
            SceneObjectKind::Interaction
        );
        assert_eq!(
            filtered_add_items("heal")[0].kind,
            SceneObjectKind::SafeZone
        );
        assert_eq!(filtered_add_items("npc")[0].kind, SceneObjectKind::Actor);
        assert_eq!(filtered_add_items("model")[0].kind, SceneObjectKind::Group);
    }

    #[test]
    fn add_search_handles_labels_and_multiple_tokens() {
        assert_eq!(
            filtered_add_items("safe zone")[0].kind,
            SceneObjectKind::SafeZone
        );
        assert!(filtered_add_items("unicorn").is_empty());
    }
}
