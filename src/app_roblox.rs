use super::*;
use crate::app_scene_migration::migrate_manifest_to_scene;
use cubacadabra_reference_import::{
    import_roblox_authoring_scene, roblox_source_files, write_roblox_place,
};
use cubacadabra_scene::serialize_authoring_scene;
use sha2::{Digest, Sha256};

impl StudioApp {
    pub(crate) fn handle_roblox_interchange_requests(&mut self) {
        let import_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_roblox_import_request);
        if import_requested {
            self.choose_and_import_roblox_place();
        }
        let export_requested = self
            .shell
            .as_mut()
            .is_some_and(StudioShell::take_roblox_export_request);
        if export_requested {
            self.choose_and_export_roblox_place();
        }
    }

    fn choose_and_import_roblox_place(&mut self) {
        if !self
            .shell
            .as_ref()
            .is_some_and(StudioShell::project_is_editable)
        {
            self.set_roblox_error("Open a source project before importing a Roblox place");
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .set_title("Import from Roblox")
            .add_filter("Roblox XML place", &["rbxlx"]);
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window);
        }
        let Some(source_path) = dialog.pick_file() else {
            return;
        };
        if let Err(error) = self.import_roblox_place(&source_path) {
            self.set_roblox_error(&format!("Could not import Roblox place: {error}"));
        }
    }

    fn import_roblox_place(&mut self, source_path: &Path) -> Result<(), String> {
        let bytes = fs::read(source_path)
            .map_err(|error| format!("could not read {}: {error}", source_path.display()))?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let stem = source_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| project_asset_slug(stem).ok())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "roblox-place".to_owned());
        let relative_source = format!("imports/roblox/{}-{}/source.rbxlx", stem, &hash[..8]);

        let (base_scene, migrated_manifest) = match self.authoring_scene.clone() {
            Some(scene) => (scene, None),
            None => {
                let migrated = migrate_manifest_to_scene(&self.authored_manifest_source)?;
                let manifest =
                    serde_json::to_string_pretty(&migrated.cleaned_manifest).map_err(|error| {
                        format!("could not serialize the migrated manifest: {error}")
                    })? + "\n";
                (migrated.scene, Some(manifest))
            }
        };
        let imported = import_roblox_authoring_scene(source_path, &base_scene, &relative_source)?;
        let scene_source = serialize_authoring_scene(&imported.scene)?;
        let destination = self.project_root.join(Path::new(&relative_source));
        let destination_was_new = !destination.exists();
        if !destination_was_new && fs::read(&destination).ok().as_deref() != Some(bytes.as_slice())
        {
            return Err(format!(
                "the preserved source path already contains different data: {}",
                destination.display()
            ));
        }
        if destination_was_new {
            write_atomic(&destination, &bytes)?;
        }

        self.authoring_scene = Some(imported.scene.clone());
        self.authoring_scene_indices = authoring_scene_indices(&imported.scene);
        self.authored_scene_source = Some(scene_source.clone());
        self.invalidate_scene_history();
        if let Some(manifest) = migrated_manifest {
            self.authored_manifest_source = manifest.clone();
            if let Some(shell) = &mut self.shell {
                shell.set_source_manifest(&manifest, true);
            }
        }
        if let Some(shell) = &mut self.shell {
            shell.set_source_scene(Some(&scene_source), true);
            shell.select_scene_node(&imported.import_root_id);
            if destination_was_new {
                shell.record_imported_asset(destination);
            }
            shell.set_source_directories(load_source_directories(&self.project_root));
            shell.set_notice(format!(
                "Imported {} editable Part{}; preserved {} other Roblox object{} — save to keep the import",
                imported.editable_parts,
                if imported.editable_parts == 1 { "" } else { "s" },
                imported.preserved_instances,
                if imported.preserved_instances == 1 { "" } else { "s" },
            ));
        }
        self.request_redraw();
        Ok(())
    }

    fn choose_and_export_roblox_place(&mut self) {
        let Some(scene) = self.authoring_scene.as_ref() else {
            self.set_roblox_error("This project has no authoring scene to export");
            return;
        };
        let suggested_name = format!(
            "{}.rbxlx",
            self.project_root
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| project_asset_slug(name).ok())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "game".to_owned())
        );
        let mut dialog = rfd::FileDialog::new()
            .set_title("Export to Roblox")
            .add_filter("Roblox XML place", &["rbxlx"])
            .set_file_name(suggested_name);
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window);
        }
        let Some(mut output_path) = dialog.save_file() else {
            return;
        };
        if output_path.extension().is_none() {
            output_path.set_extension("rbxlx");
        }
        let source_files = roblox_source_files(scene);
        let preserved_source = match source_files.as_slice() {
            [relative] => match safe_project_path(&self.project_root, relative) {
                Ok(path) if path.is_file() => Some(path),
                Ok(path) => {
                    self.set_roblox_error(&format!(
                        "The preserved Roblox source is missing: {}",
                        path.display()
                    ));
                    return;
                }
                Err(error) => {
                    self.set_roblox_error(&error);
                    return;
                }
            },
            [] => None,
            _ => {
                self.set_roblox_error(
                    "This scene contains multiple preserved Roblox places; export one imported place at a time",
                );
                return;
            }
        };
        match write_roblox_place(scene, preserved_source.as_deref(), &output_path) {
            Ok(report) => {
                if let Some(shell) = &mut self.shell {
                    let warning = if report.warnings.is_empty() {
                        String::new()
                    } else {
                        format!("; {} compatibility warning(s)", report.warnings.len())
                    };
                    shell.set_notice(format!(
                        "Exported {} updated and {} new Part{} to {}{}",
                        report.updated_parts,
                        report.added_parts,
                        if report.added_parts == 1 { "" } else { "s" },
                        output_path.display(),
                        warning,
                    ));
                }
            }
            Err(error) => self.set_roblox_error(&format!("Could not export Roblox place: {error}")),
        }
        self.request_redraw();
    }

    fn set_roblox_error(&mut self, message: &str) {
        if let Some(shell) = &mut self.shell {
            shell.set_project_error(message.to_owned());
        }
        self.request_redraw();
    }
}

fn safe_project_path(project_root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("The Roblox source reference is not a safe project-relative path".to_owned());
    }
    Ok(project_root.join(relative))
}
