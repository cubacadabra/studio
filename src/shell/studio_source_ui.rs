use super::*;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use rodio::Source;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::io::Cursor;

enum SourceTreeRow {
    Directory { path: PathBuf, depth: usize },
    File { path: PathBuf, depth: usize },
}

impl StudioShell {
    pub(crate) fn set_source_files(&mut self, files: BTreeMap<PathBuf, String>) {
        let selected = self
            .selected_source_file
            .clone()
            .filter(|path| files.contains_key(path));
        self.source_files = files;
        if self.selected_source_asset.is_some() {
            return;
        }
        self.selected_source_file = None;
        let default_file = self
            .source_files
            .contains_key(Path::new("manifest.json"))
            .then(|| PathBuf::from("manifest.json"))
            .or_else(|| self.source_files.keys().next().cloned());
        if let Some(path) = selected.or(default_file) {
            self.select_source_file(path);
        } else {
            self.source_editor_text.clear();
        }
    }

    pub(crate) fn set_source_assets(&mut self, assets: BTreeMap<PathBuf, SourceAsset>) {
        self.source_assets = assets;
        if let Some(path) = self.selected_source_asset.clone() {
            if self.source_assets.contains_key(&path) {
                self.select_source_asset(path);
            } else {
                self.selected_source_asset = None;
                self.source_asset_texture = None;
                self.stop_audio_preview();
            }
        }
        let paths = self.source_paths();
        self.source_collapsed_directories.retain(|directory| {
            self.source_directories.contains(directory)
                && paths.iter().any(|path| path.starts_with(directory))
        });
    }

    pub(crate) fn source_files_for_save(&self) -> Vec<(PathBuf, String)> {
        self.source_files
            .iter()
            .map(|(path, source)| (path.clone(), source.clone()))
            .collect()
    }

    pub(crate) fn mark_source_files_saved(&mut self) {
        self.project_dirty = false;
        self.imported_asset_paths.clear();
    }

    fn source_paths(&self) -> BTreeSet<PathBuf> {
        self.source_files
            .keys()
            .chain(self.source_assets.keys())
            .cloned()
            .collect()
    }

    fn select_source_file(&mut self, path: PathBuf) {
        let Some(source) = self.source_files.get(&path).cloned() else {
            return;
        };
        self.stop_audio_preview();
        self.selected_source_file = Some(path);
        self.selected_source_asset = None;
        self.source_asset_texture = None;
        self.source_syntax = source_syntax_for_path(
            self.selected_source_file
                .as_deref()
                .unwrap_or(Path::new("src/main.luau")),
        );
        self.source_editor_text = source;
    }

    fn select_source_asset(&mut self, path: PathBuf) {
        let Some(asset) = self.source_assets.get(&path) else {
            return;
        };
        let kind = asset.kind;
        let bytes = match fs::read(&asset.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.notice = format!("Could not read {}: {error}", path.display());
                return;
            }
        };
        self.stop_audio_preview();
        self.selected_source_file = None;
        self.selected_source_asset = Some(path.clone());
        self.source_asset_texture = None;

        if kind == SourceAssetKind::Image
            && let Ok(mut image) = image::load_from_memory(&bytes)
        {
            let longest = image.width().max(image.height());
            if longest > 2048 {
                let scale = 2048.0 / longest as f32;
                image = image.resize(
                    (image.width() as f32 * scale).round().max(1.0) as u32,
                    (image.height() as f32 * scale).round().max(1.0) as u32,
                    image::imageops::FilterType::Lanczos3,
                );
            }
            let image = image.to_rgba8();
            let dimensions = [image.width() as usize, image.height() as usize];
            let texture = self.context.load_texture(
                format!("source-asset-{}", path.display()),
                egui::ColorImage::from_rgba_unmultiplied(dimensions, image.as_raw()),
                egui::TextureOptions::LINEAR,
            );
            self.source_asset_texture = Some((path, texture, dimensions));
        }
    }

    fn stop_audio_preview(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if let Some(preview) = self.audio_preview.take() {
            preview.sink.stop();
        }
    }

    fn toggle_audio_preview(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.notice = "Audio preview is unavailable in this Linux build.".to_owned();
            return;
        }

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let Some(path) = self.selected_source_asset.clone() else {
                return;
            };
            let Some(asset) = self.source_assets.get(&path) else {
                return;
            };
            let kind = asset.kind;
            if kind != SourceAssetKind::Audio {
                return;
            }
            let bytes = match fs::read(&asset.path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.notice = format!("Could not read {}: {error}", path.display());
                    return;
                }
            };

            if let Some(preview) = &self.audio_preview
                && preview.path == path
                && !preview.sink.empty()
            {
                if preview.sink.is_paused() {
                    preview.sink.play();
                } else {
                    preview.sink.pause();
                }
                return;
            }

            self.stop_audio_preview();
            let stream = match rodio::OutputStreamBuilder::open_default_stream() {
                Ok(stream) => stream,
                Err(error) => {
                    self.notice = format!("Audio playback is unavailable: {error}");
                    return;
                }
            };
            let decoder = match rodio::Decoder::try_from(Cursor::new(bytes)) {
                Ok(decoder) => decoder,
                Err(error) => {
                    self.notice = format!("Could not decode {}: {error}", path.display());
                    return;
                }
            };
            let duration = decoder.total_duration();
            let sink = rodio::Sink::connect_new(stream.mixer());
            sink.append(decoder);
            sink.play();
            self.audio_preview = Some(SourceAudioPreview {
                path,
                _stream: stream,
                sink,
                duration,
            });
        }
    }

    pub(crate) fn show_scripts(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::Panel::left("source_files")
            .resizable(true)
            .default_size(224.0)
            .size_range(180.0..=360.0)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                let file_count = self.source_files.len() + self.source_assets.len();
                panel_header(ui, Icon::Logs, "Files", |ui| {
                    ui.label(
                        RichText::new(file_count.to_string())
                            .size(TYPE.meta)
                            .color(colors.muted),
                    );
                });
                content_frame().show(ui, |ui| {
                    let paths = self.source_paths();
                    if paths.is_empty() && self.source_directories.is_empty() {
                        ui.label(
                            RichText::new("No source files found.")
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                        return;
                    }
                    let rows = source_tree_rows(
                        &paths,
                        &self.source_directories,
                        &self.source_collapsed_directories,
                    );
                    let mut selected_file = None;
                    let mut selected_asset = None;
                    let mut toggled_directory = None;
                    egui::ScrollArea::vertical()
                        .id_salt("source_file_list")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for row in rows {
                                match row {
                                    SourceTreeRow::Directory { path, depth } => {
                                        let expanded =
                                            !self.source_collapsed_directories.contains(&path);
                                        let label = path
                                            .file_name()
                                            .map(|name| name.to_string_lossy())
                                            .unwrap_or_default();
                                        let (response, add_response) = source_tree_row(
                                            ui,
                                            &label,
                                            Icon::Folder,
                                            depth,
                                            false,
                                            Some(expanded),
                                            path == Path::new("assets/images"),
                                        );
                                        if add_response.is_some_and(|response| response.clicked()) {
                                            self.source_import_requested = Some(path.clone());
                                            self.notice =
                                                "Choose image files to import…".to_owned();
                                        } else if response
                                            .on_hover_text(path.display().to_string())
                                            .clicked()
                                        {
                                            toggled_directory = Some(path);
                                        }
                                    }
                                    SourceTreeRow::File { path, depth } => {
                                        let asset = self.source_assets.get(&path);
                                        let icon = asset
                                            .map(|asset| source_asset_icon(asset.kind))
                                            .unwrap_or(Icon::Logs);
                                        let selected = self.selected_source_file.as_ref()
                                            == Some(&path)
                                            || self.selected_source_asset.as_ref() == Some(&path);
                                        let label = path
                                            .file_name()
                                            .map(|name| name.to_string_lossy())
                                            .unwrap_or_default();
                                        let (response, _) = source_tree_row(
                                            ui, &label, icon, depth, selected, None, false,
                                        );
                                        if response
                                            .on_hover_text(path.display().to_string())
                                            .clicked()
                                        {
                                            if asset.is_some() {
                                                selected_asset = Some(path);
                                            } else {
                                                selected_file = Some(path);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    if self.project_editable {
                        ui.add_space(8.0);
                        drop_target(ui, "Drop PNG files here");
                        ui.label(
                            RichText::new("Dropped files are added to assets/images.")
                                .size(TYPE.meta)
                                .color(colors.muted),
                        );
                    }
                    if let Some(path) = toggled_directory {
                        if !self.source_collapsed_directories.insert(path.clone()) {
                            self.source_collapsed_directories.remove(&path);
                        }
                    }
                    if let Some(path) = selected_file {
                        self.select_source_file(path);
                    }
                    if let Some(path) = selected_asset {
                        self.select_source_asset(path);
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(editor_frame(colors.surface))
            .show(root, |ui| {
                if let Some(path) = self.selected_source_asset.clone() {
                    self.show_source_asset_preview(ui, path);
                    return;
                }

                let Some(path) = self.selected_source_file.clone() else {
                    panel_header(ui, Icon::Logs, "Files", |_| {});
                    content_frame().show(ui, |ui| {
                        ui.label(
                            RichText::new("Select a file to inspect or edit it.")
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
                                // A focus stroke around the entire multiline widget reads as a
                                // full-editor flash. Keep focus feedback at the insertion point.
                                ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
                                ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::NONE;
                                ui.visuals_mut().widgets.active.bg_stroke = Stroke::NONE;
                                ui.visuals_mut().widgets.open.bg_stroke = Stroke::NONE;
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
                        if response.response.has_focus()
                            && let Some(cursor_range) = response.cursor_range
                        {
                            let cursor = response
                                .galley
                                .pos_from_cursor(cursor_range.primary)
                                .translate(response.galley_pos.to_vec2());
                            let painter = ui.painter().with_clip_rect(response.text_clip_rect);
                            painter.line_segment(
                                [
                                    egui::pos2(cursor.min.x, cursor.min.y + 1.0),
                                    egui::pos2(cursor.max.x, cursor.max.y - 1.0),
                                ],
                                Stroke::new(2.0, colors.accent),
                            );
                        }
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

    fn show_source_asset_preview(&mut self, ui: &mut egui::Ui, path: PathBuf) {
        let colors = palette(ui);
        let Some(asset) = self.source_assets.get(&path) else {
            return;
        };
        let kind = asset.kind;
        let label = path.to_string_lossy().into_owned();
        let icon = source_asset_icon(kind);
        panel_header(ui, icon, &label, |ui| {
            ui.label(
                RichText::new(format_file_size(asset.bytes))
                    .size(TYPE.meta)
                    .color(colors.muted),
            );
        });
        egui::ScrollArea::vertical()
            .id_salt("source_asset_preview")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                content_frame().show(ui, |ui| match kind {
                    SourceAssetKind::Image => {
                        if let Some((_, texture, dimensions)) = &self.source_asset_texture {
                            ui.vertical_centered(|ui| {
                                ui.add_space(8.0);
                                let size = fit_preview_size(ui.available_size(), *dimensions);
                                ui.add(egui::Image::from_texture(texture).fit_to_exact_size(size));
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(format!("{} × {}", dimensions[0], dimensions[1]))
                                        .size(TYPE.meta)
                                        .color(colors.muted),
                                );
                            });
                        } else {
                            ui.label(
                                RichText::new("This image could not be decoded for preview.")
                                    .size(TYPE.secondary)
                                    .color(colors.muted),
                            );
                        }
                        ui.add_space(12.0);
                        property_section(ui, "World", |ui| {
                            let button = ui.add_enabled(
                                self.project_editable,
                                egui::Button::new("Use as floor"),
                            );
                            if button.clicked() {
                                self.scene_edit_requested =
                                    Some(SceneEditRequest::UseImageAsFloor {
                                        asset_path: path.clone(),
                                    });
                                self.notice = "Floor changed — press Play to preview it".to_owned();
                            }
                            ui.label(
                                RichText::new(
                                    "Applies this image to the ground in the active world.",
                                )
                                .size(TYPE.meta)
                                .color(colors.muted),
                            );
                            if !self.project_editable {
                                ui.label(
                                    RichText::new("Open a raw source project to edit the floor.")
                                        .size(TYPE.meta)
                                        .color(colors.muted),
                                );
                            }
                        });
                    }
                    SourceAssetKind::Audio => {
                        #[cfg(any(target_os = "macos", target_os = "windows"))]
                        let (playing, elapsed, duration) = self
                            .audio_preview
                            .as_ref()
                            .filter(|preview| preview.path == path)
                            .map(|preview| {
                                (
                                    !preview.sink.is_paused() && !preview.sink.empty(),
                                    preview.sink.get_pos(),
                                    preview.duration,
                                )
                            })
                            .unwrap_or((false, Duration::ZERO, None));
                        #[cfg(target_os = "linux")]
                        let (playing, elapsed, duration) =
                            (false, Duration::ZERO, None::<Duration>);
                        let mut toggle = false;
                        let mut stop = false;
                        ui.vertical_centered(|ui| {
                            ui.add_space(32.0);
                            ui.label(
                                RichText::new(
                                    path.file_name()
                                        .map(|name| name.to_string_lossy())
                                        .unwrap_or_default(),
                                )
                                .font(semibold_font(TYPE.primary))
                                .color(colors.text),
                            );
                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                if toolbar_button(
                                    ui,
                                    if playing { Icon::Stop } else { Icon::Play },
                                    if playing { "Pause" } else { "Play" },
                                    playing,
                                )
                                .clicked()
                                {
                                    toggle = true;
                                }
                                if toolbar_button(ui, Icon::Stop, "Stop", false).clicked() {
                                    stop = true;
                                }
                            });
                            ui.add_space(14.0);
                            let progress = duration
                                .map(|duration| {
                                    (elapsed.as_secs_f32() / duration.as_secs_f32().max(0.001))
                                        .clamp(0.0, 1.0)
                                })
                                .unwrap_or(0.0);
                            ui.add(
                                egui::ProgressBar::new(progress)
                                    .desired_width(ui.available_width().min(420.0))
                                    .show_percentage(),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!(
                                    "{} / {}",
                                    format_duration(elapsed),
                                    duration
                                        .map(format_duration)
                                        .unwrap_or_else(|| "—".to_owned())
                                ))
                                .size(TYPE.meta)
                                .color(colors.muted),
                            );
                        });
                        if stop {
                            self.stop_audio_preview();
                        } else if toggle {
                            self.toggle_audio_preview();
                        }
                    }
                    SourceAssetKind::Other => {
                        ui.label(
                            RichText::new(
                                "This asset is available to the game but has no Studio preview.",
                            )
                            .size(TYPE.secondary)
                            .color(colors.muted),
                        );
                    }
                });
            });
    }
}

fn source_tree_rows(
    paths: &BTreeSet<PathBuf>,
    explicit_directories: &BTreeSet<PathBuf>,
    collapsed_directories: &BTreeSet<PathBuf>,
) -> Vec<SourceTreeRow> {
    let mut rows = Vec::new();
    append_source_tree_rows(
        Path::new(""),
        0,
        paths,
        explicit_directories,
        collapsed_directories,
        &mut rows,
    );
    rows
}

fn append_source_tree_rows(
    directory: &Path,
    depth: usize,
    paths: &BTreeSet<PathBuf>,
    explicit_directories: &BTreeSet<PathBuf>,
    collapsed_directories: &BTreeSet<PathBuf>,
    rows: &mut Vec<SourceTreeRow>,
) {
    let mut directories = BTreeSet::new();
    let mut files = BTreeSet::new();
    for path in paths {
        let Ok(relative) = path.strip_prefix(directory) else {
            continue;
        };
        let mut components = relative.components();
        let Some(first) = components.next() else {
            continue;
        };
        let child = directory.join(first.as_os_str());
        if components.next().is_some() {
            directories.insert(child);
        } else {
            files.insert(child);
        }
    }
    for path in explicit_directories {
        let Ok(relative) = path.strip_prefix(directory) else {
            continue;
        };
        let mut components = relative.components();
        let Some(first) = components.next() else {
            continue;
        };
        if components.next().is_none() {
            directories.insert(directory.join(first.as_os_str()));
        }
    }
    for directory in directories {
        rows.push(SourceTreeRow::Directory {
            path: directory.clone(),
            depth,
        });
        if !collapsed_directories.contains(&directory) {
            append_source_tree_rows(
                &directory,
                depth + 1,
                paths,
                explicit_directories,
                collapsed_directories,
                rows,
            );
        }
    }
    for path in files {
        rows.push(SourceTreeRow::File { path, depth });
    }
}

fn source_asset_icon(kind: SourceAssetKind) -> Icon {
    match kind {
        SourceAssetKind::Image => Icon::Image,
        SourceAssetKind::Audio => Icon::Object,
        SourceAssetKind::Other => Icon::Assets,
    }
}

fn source_tree_row(
    ui: &mut egui::Ui,
    label: &str,
    icon: Icon,
    depth: usize,
    selected: bool,
    expanded: Option<bool>,
    addable: bool,
) -> (egui::Response, Option<egui::Response>) {
    let colors = palette(ui);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), UI.row), Sense::hover());
    let add_width = if addable { 28.0 } else { 0.0 };
    let row_rect = Rect::from_min_max(rect.min, egui::pos2(rect.max.x - add_width, rect.max.y));
    let response = ui.interact(
        row_rect,
        ui.id().with(("source-tree-row", label, depth)),
        Sense::click(),
    );
    if selected || response.hovered() {
        ui.painter().rect_filled(
            row_rect,
            UI.radius,
            if selected {
                colors.selection
            } else {
                colors.panel_raised
            },
        );
    }
    let indent = 6.0 + depth as f32 * 14.0;
    if let Some(expanded) = expanded {
        paint_icon(
            ui.painter(),
            Rect::from_center_size(
                rect.left_center() + egui::vec2(indent + 2.0, 0.0),
                Vec2::splat(12.0),
            ),
            if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            colors.faint,
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(indent + 16.0, 0.0),
            Vec2::splat(UI.icon),
        ),
        icon,
        if selected {
            colors.accent
        } else {
            colors.muted
        },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(indent + 29.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        if selected {
            medium_font(TYPE.primary)
        } else {
            FontId::proportional(TYPE.primary)
        },
        if selected {
            colors.text
        } else {
            colors.secondary_text
        },
    );
    paint_focus(ui, &response);
    let add_response = addable.then(|| {
        let add_rect = Rect::from_min_max(egui::pos2(rect.max.x - 28.0, rect.min.y), rect.max);
        let response = ui.interact(
            add_rect,
            ui.id().with(("source-tree-add", label, depth)),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter()
                .rect_filled(add_rect, UI.radius, colors.panel_raised);
        }
        paint_icon(
            ui.painter(),
            Rect::from_center_size(add_rect.center(), Vec2::splat(UI.icon)),
            Icon::Plus,
            if response.hovered() {
                colors.accent
            } else {
                colors.muted
            },
        );
        response.on_hover_text("Add image files")
    });
    (response, add_response)
}

fn fit_preview_size(available: Vec2, dimensions: [usize; 2]) -> Vec2 {
    let source = Vec2::new(dimensions[0] as f32, dimensions[1] as f32);
    let max = Vec2::new(available.x.min(760.0), available.y.max(180.0) - 48.0);
    let scale = (max.x / source.x.max(1.0)).min(max.y / source.y.max(1.0));
    source * scale.min(1.0).max(0.1)
}

fn format_file_size(bytes: usize) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f32 / 1024.0);
    }
    format!("{:.1} MB", bytes as f32 / (1024.0 * 1024.0))
}

fn format_duration(duration: Duration) -> String {
    format!("{}:{:02}", duration.as_secs() / 60, duration.as_secs() % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_tree_rows_include_real_nested_directories() {
        let paths = BTreeSet::from([
            PathBuf::from("manifest.json"),
            PathBuf::from("src/main.luau"),
            PathBuf::from("src/ui/actions.luau"),
            PathBuf::from("assets/audio/spell-cast.wav"),
        ]);
        let rows = source_tree_rows(&paths, &BTreeSet::new(), &BTreeSet::new());
        let labels = rows
            .iter()
            .map(|row| match row {
                SourceTreeRow::Directory { path, .. } | SourceTreeRow::File { path, .. } => {
                    path.to_string_lossy().into_owned()
                }
            })
            .collect::<Vec<_>>();
        assert!(labels.contains(&"src".to_owned()));
        assert!(labels.contains(&"src/ui".to_owned()));
        assert!(labels.contains(&"assets".to_owned()));
        assert!(labels.contains(&"assets/audio".to_owned()));
        assert!(labels.contains(&"assets/audio/spell-cast.wav".to_owned()));
    }
}
