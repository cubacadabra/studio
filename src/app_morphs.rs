use super::*;
impl StudioApp {
    pub(crate) fn import_morph_glb(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB model", &["glb"])
            .set_title("Import morph GLB")
            .pick_file()
        else {
            return;
        };
        let display_path = path.display().to_string();
        let result = fs::read(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))
            .and_then(|bytes| {
                let preview = decode_source_glb_preview(&bytes)
                    .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                let summary = inspect_source_glb_structure(&bytes)
                    .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                let lod_previews: [Option<MorphGlbPreviewMesh>; 3] = std::array::from_fn(|index| {
                    let level = ["near", "mid", "far"][index];
                    summary
                        .lod_candidates
                        .get(level)
                        .filter(|candidates| candidates.len() == 1)
                        .and_then(|candidates| candidates.first())
                        .and_then(|node| decode_source_glb_preview_node(&bytes, node).ok())
                });
                Ok((preview, summary, lod_previews))
            });
        if let Some(shell) = &mut self.shell {
            match result {
                Ok((preview, summary, lod_previews)) => {
                    shell.set_morph_preview(display_path, preview, summary);
                    for (level, preview) in lod_previews.into_iter().enumerate() {
                        if let Some(preview) = preview {
                            shell.set_morph_lod_preview(level, preview);
                        }
                    }
                }
                Err(message) => shell.set_morph_import_error(message),
            }
        }
        self.request_redraw();
    }

    pub(crate) fn export_morph_sidecar(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_sidecar_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, json)| {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph sidecar", &["json"])
                .set_file_name(&suggested_name)
                .set_title("Export morph sidecar")
                .save_file()
            else {
                return Err("Sidecar export cancelled.".to_owned());
            };
            fs::write(&path, json)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_sidecar_export_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn export_morph_draft(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_draft_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, json)| {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph draft", &["json"])
                .set_file_name(&suggested_name)
                .set_title("Save morph draft")
                .save_file()
            else {
                return Err("Morph draft save cancelled.".to_owned());
            };
            fs::write(&path, json)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_draft_export_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn import_morph_sidecar(&mut self) {
        let Some(sidecar_path) = rfd::FileDialog::new()
            .add_filter("Morph sidecar", &["json"])
            .set_title("Open morph sidecar")
            .pick_file()
        else {
            return;
        };
        let result = fs::read_to_string(&sidecar_path)
            .map_err(|error| format!("Could not read {}: {error}", sidecar_path.display()))
            .and_then(|manifest_source| {
                let draft = if is_morph_draft_json(&manifest_source) {
                    Some(parse_morph_draft_json(&manifest_source)?)
                } else {
                    None
                };
                let geometry_file = match &draft {
                    Some(draft) => draft.geometry_file.clone(),
                    None => source_manifest_geometry_file(&manifest_source)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?,
                };
                let glb_path = sidecar_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(geometry_file);
                let glb = fs::read(&glb_path).map_err(|error| {
                    format!(
                        "Could not read referenced GLB {}: {error}",
                        glb_path.display()
                    )
                })?;
                let (manifest, draft, preview, summary, lod_previews) = match draft {
                    Some(draft) => {
                        let preview = decode_source_glb_preview(&glb)
                            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                        let summary = inspect_source_glb_structure(&glb)
                            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                        let lod_previews: [Option<MorphGlbPreviewMesh>; 3] =
                            std::array::from_fn(|index| {
                                if draft.lod_nodes[index].is_empty() {
                                    None
                                } else {
                                    decode_source_glb_preview_node(&glb, &draft.lod_nodes[index])
                                        .ok()
                                }
                            });
                        (None, Some(draft), preview, summary, lod_previews)
                    }
                    None => {
                        let (manifest, preview, summary) =
                            inspect_source_sidecar(&manifest_source, &glb).map_err(
                                |diagnostics| Self::format_morph_diagnostics(&diagnostics),
                            )?;
                        let lod_previews: [Option<MorphGlbPreviewMesh>; 3] =
                            std::array::from_fn(|index| {
                                let level = ["near", "mid", "far"][index];
                                manifest.geometry.lod_nodes.get(level).and_then(|node| {
                                    decode_source_glb_preview_node(&glb, node).ok()
                                })
                            });
                        (Some(manifest), None, preview, summary, lod_previews)
                    }
                };
                Ok((
                    glb_path.display().to_string(),
                    manifest,
                    draft,
                    preview,
                    summary,
                    lod_previews,
                ))
            });
        if let Some(shell) = &mut self.shell {
            match result {
                Ok((glb_path, manifest, draft, preview, summary, lod_previews)) => {
                    if let Some(manifest) = manifest {
                        shell.set_morph_sidecar_preview(glb_path, manifest, preview, summary);
                    } else if let Some(draft) = draft {
                        shell.set_morph_draft_preview(glb_path, draft, preview, summary);
                    }
                    for (level, preview) in lod_previews.into_iter().enumerate() {
                        if let Some(preview) = preview {
                            shell.set_morph_lod_preview(level, preview);
                        }
                    }
                    shell.select_morph_preview_lod(Some(0));
                }
                Err(message) => shell.set_morph_import_error(message),
            }
        }
        self.request_redraw();
    }

    pub(crate) fn publish_morph_pack(&mut self) {
        let inputs = self
            .shell
            .as_ref()
            .map(StudioShell::morph_pack_inputs)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = inputs.and_then(|(glb_path, suggested_name, manifest_json)| {
            let glb = fs::read(&glb_path)
                .map_err(|error| format!("Could not read {}: {error}", glb_path))?;
            let (pack, summary) = compile_source_morph_pack(&manifest_json, &glb)
                .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Morph pack", &["morphpack"])
                .set_file_name(&suggested_name)
                .set_title("Publish morph pack")
                .save_file()
            else {
                return Err("Morph pack publish cancelled.".to_owned());
            };
            fs::write(&path, &pack)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
            Ok((summary.asset_id, summary.byte_len, pack))
        });
        let result = result.and_then(|(_, _, pack)| self.activate_morph_pack(&pack));
        if let Some(shell) = &mut self.shell {
            shell.set_morph_publish_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn add_morph_to_game(&mut self) {
        let result = if self.standalone_preview {
            Err("Open a game project before adding a character asset to it.".to_owned())
        } else {
            self.shell
                .as_ref()
                .map(StudioShell::morph_project_payload)
                .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()))
                .and_then(|(source_path, manifest_json, preview)| {
                    let glb = fs::read(&source_path)
                        .map_err(|error| format!("Could not read {}: {error}", source_path))?;
                    let manifest_json =
                        rewrite_manifest_geometry_file(&manifest_json, "source.glb")?;
                    let asset = source_manifest_asset(&manifest_json)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                    let asset_id = asset.id.to_string();
                    let slug = project_asset_slug(&asset_id)?;
                    let asset_directory = self.project_root.join("assets/characters").join(&slug);
                    fs::create_dir_all(&asset_directory).map_err(|error| {
                        format!(
                            "Could not create character asset directory {}: {error}",
                            asset_directory.display()
                        )
                    })?;
                    let (pack, _) = compile_source_morph_pack(&manifest_json, &glb)
                        .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
                    let thumbnail = encode_morph_thumbnail_png(&preview)?;
                    write_atomic(&asset_directory.join("source.glb"), &glb)?;
                    write_atomic(
                        &asset_directory.join("source.morph.json"),
                        manifest_json.as_bytes(),
                    )?;
                    write_atomic(&asset_directory.join("runtime.morphpack"), &pack)?;
                    write_atomic(&asset_directory.join("thumbnail.png"), &thumbnail)?;

                    let catalog_path = self.project_root.join("assets/characters/catalog.json");
                    update_project_morph_catalog(
                        &catalog_path,
                        &asset_id,
                        &format!("{slug}/source.morph.json"),
                    )?;
                    let activated = self.activate_morph_pack(&pack)?;
                    Ok(activated)
                })
        };
        if let Some(shell) = &mut self.shell {
            shell.set_morph_project_result(result);
        }
        self.request_redraw();
    }
}
