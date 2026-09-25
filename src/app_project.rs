use super::*;
use cubacadabra_scene::parse_authoring_scene;
impl StudioApp {
    pub(crate) fn create_new_project(&mut self, title: &str, parent: &Path) {
        let result = game_creator::create_game(title, parent);
        match result {
            Ok(created) => {
                if let Some(pending) = &mut self.pending_roblox_import {
                    if pending.project.is_none() {
                        pending.project = Some(created.project.clone());
                    }
                }
                if let Some(shell) = &mut self.shell {
                    shell.set_new_project_created(&created.project);
                }
                self.start_project_load(created.project);
            }
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_new_project_error(error);
                }
            }
        }
        self.request_redraw();
    }

    pub(crate) fn import_source_images(&mut self, files: Vec<PathBuf>, target: PathBuf) {
        if !self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable)
        {
            return;
        }
        if target != Path::new("assets/images") {
            if let Some(shell) = &mut self.shell {
                shell.set_notice("Images can only be added to assets/images".to_owned());
            }
            return;
        }

        let destination = self.project_root.join(&target);
        if let Err(error) = fs::create_dir_all(&destination) {
            if let Some(shell) = &mut self.shell {
                shell.set_project_error(format!("Could not create image asset directory: {error}"));
            }
            return;
        }
        let mut manifest: Value = match serde_json::from_str(&self.authored_manifest_source) {
            Ok(manifest) => manifest,
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(format!("Manifest is no longer valid JSON: {error}"));
                }
                return;
            }
        };
        let mut imported = Vec::new();
        let mut errors = Vec::new();
        for file in files {
            let is_png = file
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
            if !is_png {
                errors.push(format!("{} is not a PNG file", file.display()));
                continue;
            }
            let Some(file_name) = file.file_name().and_then(|name| name.to_str()) else {
                errors.push(format!("{} has no valid filename", file.display()));
                continue;
            };
            let destination_file = unique_asset_path(&destination, file_name);
            let bytes = match fs::read(&file) {
                Ok(bytes) => bytes,
                Err(error) => {
                    errors.push(format!("could not read {}: {error}", file.display()));
                    continue;
                }
            };
            if bytes.len() > 8 * 1024 * 1024 {
                errors.push(format!("{} exceeds the 8 MiB image limit", file.display()));
                continue;
            }
            if image::load_from_memory(&bytes).is_err() {
                errors.push(format!("{} is not a valid PNG image", file.display()));
                continue;
            }
            if let Err(error) = write_atomic(&destination_file, &bytes) {
                errors.push(error);
                continue;
            }
            let Some(destination_name) = destination_file.file_name() else {
                errors.push(format!(
                    "could not determine the destination filename for {file_name}"
                ));
                continue;
            };
            let relative = target.join(destination_name);
            match add_image_asset(&mut manifest, &relative) {
                Ok(_) => imported.push(relative),
                Err(error) => {
                    let _ = fs::remove_file(&destination_file);
                    errors.push(error);
                }
            }
        }

        if imported.is_empty() {
            if let Some(shell) = &mut self.shell {
                shell.set_notice(if errors.is_empty() {
                    "No image files were selected".to_owned()
                } else {
                    errors.join("; ")
                });
            }
            return;
        }
        let source = match serde_json::to_string_pretty(&manifest) {
            Ok(source) => source + "\n",
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(format!(
                        "Could not update the project manifest: {error}"
                    ));
                }
                return;
            }
        };
        self.authored_manifest_source = source.clone();
        if let Some(shell) = &mut self.shell {
            shell.set_source_manifest(&source, true);
            shell.set_source_assets(load_source_assets(&self.project_root));
            shell.set_source_directories(load_source_directories(&self.project_root));
            for relative in &imported {
                shell.record_imported_asset(self.project_root.join(relative));
            }
            let message = if errors.is_empty() {
                format!(
                    "Imported {} image{} — save to keep the project change",
                    imported.len(),
                    if imported.len() == 1 { "" } else { "s" }
                )
            } else {
                format!(
                    "Imported {} image{}; {}",
                    imported.len(),
                    if imported.len() == 1 { "" } else { "s" },
                    errors.join("; ")
                )
            };
            shell.set_notice(message);
        }
        self.request_redraw();
    }

    pub(crate) fn save_project_source(&mut self) -> bool {
        let editable = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable);
        if !editable {
            return false;
        }
        let mut source_files = self
            .shell
            .as_ref()
            .map(StudioShell::source_files_for_save)
            .unwrap_or_default();
        if let Some(scene) = self.authoring_scene.as_ref() {
            let scene_source = match cubacadabra_scene::serialize_authoring_scene(scene) {
                Ok(source) => source,
                Err(error) => {
                    if let Some(shell) = &mut self.shell {
                        shell.set_project_error(format!("Cannot save scene.json: {error}"));
                    }
                    return false;
                }
            };
            if let Some((_, source)) = source_files
                .iter_mut()
                .find(|(relative_path, _)| relative_path == Path::new("scene.json"))
            {
                *source = scene_source;
            } else {
                source_files.push((PathBuf::from("scene.json"), scene_source));
            }
        }
        if !source_files
            .iter()
            .any(|(relative_path, _)| relative_path == Path::new("manifest.json"))
        {
            source_files.push((
                PathBuf::from("manifest.json"),
                self.authored_manifest_source.clone(),
            ));
        }
        if let Some((_, manifest_source)) = source_files
            .iter()
            .find(|(relative_path, _)| relative_path == Path::new("manifest.json"))
            && let Err(error) = serde_json::from_str::<Value>(manifest_source)
        {
            if let Some(shell) = &mut self.shell {
                shell.set_project_error(format!("Cannot save manifest: {error}"));
            }
            return false;
        }
        if let Some((_, scene_source)) = source_files
            .iter()
            .find(|(relative_path, _)| relative_path == Path::new("scene.json"))
            && let Err(error) = parse_authoring_scene(scene_source)
        {
            if let Some(shell) = &mut self.shell {
                shell.set_project_error(format!("Cannot save scene.json: {error}"));
            }
            return false;
        }
        let mut saved = Ok(());
        for (relative_path, source) in &source_files {
            if let Err(message) =
                write_atomic(&self.project_root.join(relative_path), source.as_bytes())
            {
                saved = Err(message);
                break;
            }
            if relative_path == Path::new("manifest.json") {
                self.authored_manifest_source = source.clone();
            } else if relative_path == Path::new("scene.json") {
                self.authored_scene_source = Some(source.clone());
            }
        }
        match saved {
            Ok(()) => {
                if let Some(shell) = &mut self.shell {
                    let preview_stale = shell.preview_is_stale();
                    shell.mark_source_files_saved();
                    shell.set_source_manifest(&self.authored_manifest_source, false);
                    shell.set_source_scene(self.authored_scene_source.as_deref(), false);
                    shell.set_notice(if preview_stale {
                        "Project saved — press Play to rebuild the preview".to_owned()
                    } else {
                        "Project saved".to_owned()
                    });
                }
                true
            }
            Err(message) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(message);
                }
                false
            }
        }
    }

    pub(crate) fn capture_codex_changes(&mut self) -> Vec<CodexFileChange> {
        let Some(before) = self.codex_checkpoint.take() else {
            self.codex_changes = None;
            return Vec::new();
        };
        let Ok(after) = snapshot_project_files(&self.project_root) else {
            self.codex_changes = None;
            return Vec::new();
        };
        let changes = diff_project_files(before, after);
        self.codex_changes = Some(changes.clone());
        changes
    }

    pub(crate) fn undo_codex_changes(&mut self) {
        let Some(changes) = self.codex_changes.take() else {
            if let Some(shell) = &mut self.shell {
                shell.set_notice("There are no Codex changes to undo".to_owned());
            }
            return;
        };
        let mut restored = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();
        for change in changes {
            let path = self.project_root.join(&change.relative_path);
            let current = fs::read(&path).ok();
            if current != change.after {
                skipped += 1;
                continue;
            }
            let result = match change.before {
                Some(bytes) => write_atomic(&path, &bytes),
                None => fs::remove_file(&path).map_err(|error| {
                    format!(
                        "could not remove {}: {error}",
                        change.relative_path.display()
                    )
                }),
            };
            match result {
                Ok(()) => restored += 1,
                Err(message) => errors.push(message),
            }
        }
        if let Some(shell) = &mut self.shell {
            shell.clear_codex_changes();
            if errors.is_empty() {
                let message = if skipped == 0 {
                    format!("Undid {restored} Codex change(s); rebuilding preview…")
                } else {
                    format!(
                        "Undid {restored} Codex change(s); kept {skipped} later edit(s); rebuilding preview…"
                    )
                };
                shell.set_notice(message);
            } else {
                shell.set_project_error(format!(
                    "Undo restored {restored} file(s), skipped {skipped}, and failed: {}",
                    errors.join("; ")
                ));
            }
        }
        self.refresh_authored_manifest_from_disk();
        self.start_project_reload();
    }

    pub(crate) fn refresh_authored_manifest_from_disk(&mut self) {
        let path = self.project_root.join("manifest.json");
        match fs::read_to_string(&path) {
            Ok(source) => {
                self.authored_manifest_source = source.clone();
                if let Some(shell) = &mut self.shell {
                    shell.set_source_manifest(&source, false);
                }
            }
            Err(error) => {
                if let Some(shell) = &mut self.shell {
                    shell.set_project_error(format!(
                        "Could not read the updated manifest {}: {error}",
                        path.display()
                    ));
                }
            }
        }
        let scene_path = self.project_root.join("scene.json");
        if scene_path.is_file()
            && let Ok(source) = fs::read_to_string(&scene_path)
        {
            self.authoring_scene = cubacadabra_scene::parse_authoring_scene(&source).ok();
            self.authoring_scene_indices = self
                .authoring_scene
                .as_ref()
                .map(authoring_scene_indices)
                .unwrap_or_default();
            self.authored_scene_source = Some(source.clone());
            if let Some(shell) = &mut self.shell {
                shell.set_source_scene(Some(&source), false);
            }
        }
    }
}

fn unique_asset_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("png");
    for suffix in 2.. {
        let candidate = directory.join(format!("{stem}-{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("the suffix range is finite only in theory")
}
