use super::*;
impl StudioShell {
    pub(crate) fn set_remote_morph_catalog(&mut self, source: &str) -> Result<usize, String> {
        let published = crate::wardrobe::PublishedCatalog::parse(source)?;
        let count = published.catalog.assets.len();
        self.morph_catalog = published.catalog;
        self.morph_artifacts = published.artifacts;
        self.morph_catalog_ready = true;
        self.morph_catalog_error = None;
        Ok(count)
    }

    pub(crate) fn set_local_morph_catalog(&mut self, catalog: MorphCatalog) {
        self.morph_catalog = catalog;
        self.morph_artifacts.clear();
        self.morph_catalog_ready = true;
        self.morph_catalog_error = None;
    }

    pub(crate) fn morph_artifact(&self, id: &MorphAssetId) -> Option<&crate::wardrobe::Artifact> {
        self.morph_artifacts.get(id)
    }

    pub(crate) fn set_catalog_error(&mut self, message: String) {
        self.morph_catalog_error = Some(message);
    }

    pub(crate) fn set_morph_thumbnail(&mut self, url: String, bytes: &[u8]) {
        if let Ok(image) = image::load_from_memory(bytes) {
            if image.width() > 1024 || image.height() > 1024 {
                return;
            }
            let rgba = image.to_rgba8();
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            );
            let texture = self
                .context
                .load_texture(&url, image, egui::TextureOptions::LINEAR);
            self.morph_thumbnails.insert(url, texture);
        }
    }

    pub(crate) fn set_morph_loading(&mut self, loading: bool) {
        self.morph_loading = loading;
    }

    pub(crate) fn select_morph(&mut self, id: MorphAssetId) {
        self.selected_morph = id;
    }

    pub(crate) fn upsert_morph_asset(
        &mut self,
        definition: cubacadabra_morphs::MorphAssetDefinition,
    ) {
        self.morph_catalog
            .assets
            .retain(|asset| asset.id != definition.id);
        self.morph_catalog.assets.push(definition);
    }

    pub(crate) fn morph_catalog(&self) -> &MorphCatalog {
        &self.morph_catalog
    }

    pub(crate) fn take_morph_request(&mut self) -> Option<crate::wardrobe::Request> {
        self.morph_request.take()
    }

    pub(crate) fn set_active_morph_loadout(&mut self, loadout: &cubacadabra_morphs::MorphLoadout) {
        self.active_morphs = crate::wardrobe::selected_ids(loadout)
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        self.active_loadout = loadout.clone();
    }

    pub(crate) fn take_morph_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_import_requested)
    }

    pub(crate) fn take_morph_sidecar_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_sidecar_import_requested)
    }

    pub(crate) fn take_morph_project_add_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_project_add_requested)
    }

    pub(crate) fn take_morph_pack_import_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_pack_import_requested)
    }

    pub(crate) fn take_morph_sidecar_export_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_sidecar_export_requested)
    }

    pub(crate) fn take_morph_draft_export_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_draft_export_requested)
    }

    pub(crate) fn take_morph_publish_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_publish_requested)
    }

    pub(crate) fn take_morph_thumbnail_request(&mut self) -> bool {
        std::mem::take(&mut self.morph_thumbnail_requested)
    }

    pub(crate) fn morph_attachment(&self) -> MorphAttachment {
        MorphAttachment {
            mode: MorphAttachmentMode::Rigid,
            joint: self.morph_attachment_joint.trim().to_owned(),
            translation: self.morph_attachment_translation,
            rotation: self.morph_attachment_rotation,
            scale: self.morph_attachment_scale,
        }
    }

    pub(crate) fn morph_sidecar_payload(&self) -> Result<(String, String), String> {
        let path = self
            .morph_preview_path
            .as_deref()
            .ok_or_else(|| "Import a GLB before exporting its sidecar.".to_owned())?;
        let preview = self
            .morph_preview
            .as_ref()
            .ok_or_else(|| "The imported GLB has no preview mesh to save.".to_owned())?;
        let asset = self
            .morph_draft_asset
            .as_ref()
            .or_else(|| self.morph_catalog.asset(&self.selected_morph))
            .ok_or_else(|| "Select a catalog asset before exporting its sidecar.".to_owned())?;
        let geometry_file = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "The imported GLB needs a safe filename for its sidecar.".to_owned())?
            .to_owned();
        let lod_nodes = [
            self.morph_lod_nodes[0].trim(),
            self.morph_lod_nodes[1].trim(),
            self.morph_lod_nodes[2].trim(),
        ];
        let fallback_triangle_count = u32::try_from(preview.indices.len() / 3)
            .map_err(|_| "The preview mesh triangle count is too large.".to_owned())?;
        let summary = self
            .morph_source_summary
            .as_ref()
            .ok_or_else(|| "The imported GLB has no source summary to save.".to_owned())?;
        let triangle_counts = lod_nodes.map(|node| {
            summary
                .node_triangle_counts
                .get(node)
                .copied()
                .unwrap_or(fallback_triangle_count)
                .max(1)
        });
        let json = build_source_manifest_json(
            asset,
            geometry_file,
            self.morph_attachment(),
            lod_nodes,
            triangle_counts,
        )
        .map_err(|diagnostics| {
            diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        let suggested_name = format!(
            "{}.morph.json",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, json))
    }

    pub(crate) fn morph_pack_inputs(&self) -> Result<(String, String, String), String> {
        let (sidecar_name, manifest_json) = self.morph_sidecar_payload()?;
        let glb_path = self
            .morph_preview_path
            .as_ref()
            .ok_or_else(|| "Import a GLB before publishing its pack.".to_owned())?
            .clone();
        let suggested_name = format!(
            "{}.morphpack",
            std::path::Path::new(&sidecar_name)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((glb_path, suggested_name, manifest_json))
    }

    pub(crate) fn morph_project_payload(
        &self,
    ) -> Result<(String, String, MorphGlbPreviewMesh), String> {
        let path = self
            .morph_preview_path
            .clone()
            .ok_or_else(|| "Import a GLB before adding it to this game.".to_owned())?;
        let (_, manifest_json) = self.morph_sidecar_payload()?;
        let preview = self
            .morph_preview
            .clone()
            .ok_or_else(|| "The imported GLB has no preview mesh to add.".to_owned())?;
        Ok((path, manifest_json, preview))
    }

    pub(crate) fn morph_draft_payload(&self) -> Result<(String, String), String> {
        let path = self
            .morph_preview_path
            .as_deref()
            .ok_or_else(|| "Import a GLB before saving its draft.".to_owned())?;
        let asset = self
            .morph_draft_asset
            .as_ref()
            .or_else(|| self.morph_catalog.asset(&self.selected_morph))
            .ok_or_else(|| "Select a catalog asset before saving its draft.".to_owned())?;
        let geometry_file = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "The imported GLB needs a safe filename for its draft.".to_owned())?
            .to_owned();
        let summary = self
            .morph_source_summary
            .as_ref()
            .ok_or_else(|| "The imported GLB has no source summary to save.".to_owned())?;
        let fallback_triangle_count = self
            .morph_preview
            .as_ref()
            .map(|preview| preview.indices.len() / 3)
            .and_then(|count| u32::try_from(count).ok())
            .ok_or_else(|| "The imported GLB has no preview triangle count.".to_owned())?;
        let lod_nodes = [
            self.morph_lod_nodes[0].trim(),
            self.morph_lod_nodes[1].trim(),
            self.morph_lod_nodes[2].trim(),
        ];
        let triangle_counts = lod_nodes.map(|node| {
            summary
                .node_triangle_counts
                .get(node)
                .copied()
                .unwrap_or_else(|| {
                    if node.is_empty() {
                        0
                    } else {
                        fallback_triangle_count
                    }
                })
        });
        let json = build_morph_draft_json(
            asset,
            geometry_file,
            self.morph_attachment(),
            lod_nodes,
            triangle_counts,
        )?;
        let suggested_name = format!(
            "{}.morph.draft.json",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, json))
    }

    pub(crate) fn set_morph_sidecar_export_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    true,
                    "Sidecar saved. The GLB and .morph.json can now travel together.".to_owned(),
                ));
                self.notice = "Morph sidecar saved".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_draft_export_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    false,
                    "Draft saved. Complete the LOD mapping before exporting or publishing."
                        .to_owned(),
                ));
                self.notice = "Morph draft saved".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_publish_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status =
                    Some((true, format!("Published {asset_id} ({byte_len} bytes).")));
                self.notice = "Morph pack published".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_project_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status = Some((
                    true,
                    format!("Added {asset_id} to this game ({byte_len} bytes)."),
                ));
                self.notice = "Character asset added to game".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_runtime_result(&mut self, result: Result<(String, usize), String>) {
        match result {
            Ok((asset_id, byte_len)) => {
                self.morph_draft_status = Some((
                    true,
                    format!("Loaded {asset_id} for the live player ({byte_len} bytes)."),
                ));
                self.notice = "Morph pack loaded".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn morph_thumbnail_payload(&self) -> Result<(String, MorphGlbPreviewMesh), String> {
        let path = self
            .morph_preview_path
            .as_ref()
            .ok_or_else(|| "Import a GLB before generating a thumbnail.".to_owned())?;
        let preview = self
            .morph_preview
            .as_ref()
            .ok_or_else(|| "The imported GLB has no preview mesh.".to_owned())?
            .clone();
        let suggested_name = format!(
            "{}.png",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("morph")
        );
        Ok((suggested_name, preview))
    }

    pub(crate) fn set_morph_thumbnail_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.morph_draft_status = Some((
                    true,
                    "Thumbnail generated from the current shaded preview.".to_owned(),
                ));
                self.notice = "Morph thumbnail generated".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }

    pub(crate) fn set_morph_preview(
        &mut self,
        path: String,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        let triangle_count = u32::try_from(preview.indices.len() / 3)
            .unwrap_or(u32::MAX)
            .max(1);
        let draft_asset = Some(default_rigid_accessory_asset(&path, triangle_count));
        self.morph_preview_path = Some(path);
        self.morph_preview = Some(preview);
        self.morph_lod_previews = [None, None, None];
        self.morph_preview_lod = None;
        self.morph_attachment_joint = "head".to_owned();
        self.morph_attachment_translation = [0.0; 3];
        self.morph_attachment_rotation = [0.0, 0.0, 0.0, 1.0];
        self.morph_attachment_scale = [1.0; 3];
        self.morph_lod_nodes = ["near", "mid", "far"].map(|level| {
            summary
                .lod_candidates
                .get(level)
                .filter(|candidates| candidates.len() == 1)
                .and_then(|candidates| candidates.first())
                .cloned()
                .unwrap_or_default()
        });
        self.morph_source_summary = Some(summary);
        self.morph_draft_asset = draft_asset;
        self.morph_draft_status = None;
        self.morph_import_error = None;
        self.notice = "GLB preview imported".to_owned();
    }

    pub(crate) fn set_morph_lod_preview(&mut self, level: usize, preview: MorphGlbPreviewMesh) {
        if level >= self.morph_lod_previews.len() {
            return;
        }
        self.morph_lod_previews[level] = Some(preview.clone());
        if self.morph_preview_lod == Some(level) {
            self.morph_preview = Some(preview);
        }
    }

    pub(crate) fn select_morph_preview_lod(&mut self, level: Option<usize>) {
        let Some(level) = level else {
            self.morph_preview_lod = None;
            return;
        };
        let Some(preview) = self.morph_lod_previews.get(level).and_then(Option::as_ref) else {
            return;
        };
        self.morph_preview_lod = Some(level);
        self.morph_preview = Some(preview.clone());
    }

    pub(crate) fn set_morph_sidecar_preview(
        &mut self,
        glb_path: String,
        manifest: MorphSourceManifest,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        let attachment = manifest.attachment.clone();
        let lod_nodes = ["near", "mid", "far"].map(|level| {
            manifest
                .geometry
                .lod_nodes
                .get(level)
                .cloned()
                .unwrap_or_default()
        });
        self.set_morph_preview(glb_path, preview, summary);
        self.morph_draft_asset = Some(manifest.asset);
        self.morph_attachment_joint = attachment.joint;
        self.morph_attachment_translation = attachment.translation;
        self.morph_attachment_rotation = attachment.rotation;
        self.morph_attachment_scale = attachment.scale;
        self.morph_lod_nodes = lod_nodes;
        self.morph_draft_status = Some((
            true,
            "Sidecar reimported and GLB contract validated.".to_owned(),
        ));
        self.notice = "Morph sidecar reimported".to_owned();
    }

    pub(crate) fn set_morph_draft_preview(
        &mut self,
        glb_path: String,
        draft: MorphDraftDocument,
        preview: MorphGlbPreviewMesh,
        summary: MorphGlbSourceSummary,
    ) {
        self.set_morph_preview(glb_path, preview, summary);
        self.morph_draft_asset = Some(draft.asset);
        self.morph_attachment_joint = draft.attachment.joint;
        self.morph_attachment_translation = draft.attachment.translation;
        self.morph_attachment_rotation = draft.attachment.rotation;
        self.morph_attachment_scale = draft.attachment.scale;
        self.morph_lod_nodes = draft.lod_nodes;
        self.validate_morph_draft();
        self.notice = "Morph draft reimported".to_owned();
    }

    pub(crate) fn set_morph_import_error(&mut self, message: String) {
        self.morph_import_error = Some(message.clone());
        self.notice = message;
    }

    pub(crate) fn validate_morph_draft(&mut self) {
        let Some(summary) = &self.morph_source_summary else {
            self.morph_draft_status = Some((
                false,
                "Import a GLB before validating its draft.".to_owned(),
            ));
            return;
        };
        let mut issues = Vec::new();
        let joint = self.morph_attachment_joint.trim();
        if joint.is_empty() || joint.contains('/') || joint.contains('\\') {
            issues.push("Attachment joint must be a non-empty joint name.".to_owned());
        }
        if !self
            .morph_attachment_translation
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 10.0)
        {
            issues.push("Attachment offset must stay within +/-10 units.".to_owned());
        }
        if !self
            .morph_attachment_scale
            .iter()
            .all(|value| value.is_finite() && (0.01..=100.0).contains(value))
        {
            issues.push("Attachment scale must stay within 0.01–100.".to_owned());
        }
        let rotation_length = self
            .morph_attachment_rotation
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if !rotation_length.is_finite() || !(0.99..=1.01).contains(&rotation_length) {
            issues.push("Attachment rotation must be a normalized quaternion.".to_owned());
        }
        if self
            .morph_draft_asset
            .as_ref()
            .is_some_and(|asset| asset.kind == MorphAssetKind::Headwear)
            && let Some(mesh) = self
                .morph_lod_previews
                .first()
                .and_then(Option::as_ref)
                .or(self.morph_preview.as_ref())
            && let Some((minimum, maximum)) = morph_mesh_bounds(mesh)
        {
            let runtime_width = ((maximum[0] - minimum[0]) * self.morph_attachment_scale[0].abs())
                .max((maximum[2] - minimum[2]) * self.morph_attachment_scale[2].abs());
            if !(0.25..=2.20).contains(&runtime_width) {
                issues.push(format!(
                    "Headwear runtime width is {runtime_width:.2} units; use Fit to person head or adjust attachment scale."
                ));
            }
        }
        for (index, level) in ["Near", "Mid", "Far"].into_iter().enumerate() {
            let node = self.morph_lod_nodes[index].trim();
            if node.is_empty() {
                issues.push(format!("{level} LOD needs a node mapping."));
            } else if !summary.node_names.iter().any(|candidate| candidate == node) {
                issues.push(format!(
                    "{level} LOD node {node:?} is not present in the GLB."
                ));
            }
        }
        for first in 0..self.morph_lod_nodes.len() {
            for second in (first + 1)..self.morph_lod_nodes.len() {
                let first_node = self.morph_lod_nodes[first].trim();
                if !first_node.is_empty() && first_node == self.morph_lod_nodes[second].trim() {
                    issues.push("Near, Mid, and Far must use distinct nodes.".to_owned());
                }
            }
        }
        if issues.is_empty() {
            self.morph_draft_status = Some((
                true,
                "Draft mapping is ready for sidecar export.".to_owned(),
            ));
            self.notice = "Draft mapping validated".to_owned();
        } else {
            self.morph_draft_status = Some((false, issues.join(" ")));
            self.notice = "Draft mapping needs attention".to_owned();
        }
    }

    pub(crate) fn fit_current_morph_to_person(&mut self) {
        let result = self
            .morph_lod_previews
            .first()
            .and_then(Option::as_ref)
            .or(self.morph_preview.as_ref())
            .ok_or_else(|| "Import a headwear mesh before fitting it.".to_owned())
            .and_then(|mesh| {
                fit_rigid_headwear_to_person(mesh, self.morph_attachment_joint.trim())
            });
        match result {
            Ok(attachment) => {
                self.morph_attachment_translation = attachment.translation;
                self.morph_attachment_rotation = attachment.rotation;
                self.morph_attachment_scale = attachment.scale;
                self.morph_draft_status = Some((
                    true,
                    format!(
                        "Fit to person head at {:.3}× scale. Validate and republish the pack.",
                        attachment.scale[0]
                    ),
                ));
                self.notice = "Headwear attachment fitted".to_owned();
            }
            Err(message) => {
                self.morph_draft_status = Some((false, message.clone()));
                self.notice = message;
            }
        }
    }
}
