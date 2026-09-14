use super::*;
impl StudioApp {
    pub(crate) fn import_morph_pack(&mut self) {
        let result = rfd::FileDialog::new()
            .add_filter("Morph pack", &["morphpack"])
            .set_title("Load morph pack")
            .pick_file()
            .ok_or_else(|| "Morph pack load cancelled.".to_owned())
            .and_then(|path| {
                let pack = fs::read(&path)
                    .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
                self.activate_morph_pack(&pack)
            });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_runtime_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn activate_morph_pack(&mut self, pack: &[u8]) -> Result<(String, usize), String> {
        debug!("decoding morph pack: bytes={}", pack.len());
        let decoded = decode_morph_pack(pack)
            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
        let asset_id = decoded.asset.id.to_string();
        debug!(
            "morph pack decoded: asset_id={} kind={:?} attachment_mode={:?}",
            asset_id, decoded.asset.kind, decoded.attachment.mode
        );
        self.renderer
            .as_mut()
            .ok_or_else(|| "The renderer is not ready for morph registration.".to_owned())?
            .register_morph_pack(pack)
            .map_err(|diagnostics| Self::format_morph_diagnostics(&diagnostics))?;
        debug!("morph pack registered with renderer: asset_id={}", asset_id);
        if let Some(shell) = &mut self.shell {
            shell.upsert_morph_asset(decoded.asset.clone());
        }
        self.registered_morphs.insert(asset_id.clone());
        self.request_morph_change(wardrobe::Request::Equip(decoded.asset.id))?;
        Ok((asset_id, pack.len()))
    }

    pub(crate) fn apply_morph_loadout(
        &mut self,
        mut loadout: cubacadabra_morphs::MorphLoadout,
    ) -> Result<(), String> {
        debug!(
            "resolving morph loadout: base={} parts={:?}",
            loadout.base, loadout.parts
        );
        let catalog = self
            .shell
            .as_ref()
            .ok_or_else(|| "Morph shell is not ready.".to_owned())?
            .morph_catalog()
            .clone();
        loadout.revision = self.client.engine().appearance_revision().saturating_add(1);
        let capabilities = morph_application::capabilities();
        cubacadabra_morphs::resolve_loadout(&catalog, &loadout, &capabilities).map_err(
            |diagnostics| {
                let message = Self::format_morph_diagnostics(&diagnostics);
                warn!("morph loadout resolution failed: {}", message);
                message
            },
        )?;
        let appearance = serde_json::to_string(&loadout)
            .map_err(|error| format!("Could not encode morph loadout: {error}"))?;
        debug!(
            "applying native morph loadout: base={} parts={:?}",
            loadout.base, loadout.parts
        );
        if self
            .client
            .engine_mut()
            .set_local_morph_loadout_json(&appearance)
            == 0
        {
            error!("engine rejected native morph loadout");
            return Err("The player appearance rejected that morph loadout.".to_owned());
        }
        self.morph_loadout = loadout;
        debug!(
            "morph loadout applied to engine: base={} parts={:?}",
            self.morph_loadout.base, self.morph_loadout.parts
        );
        Ok(())
    }

    pub(crate) fn generate_morph_thumbnail(&mut self) {
        let payload = self
            .shell
            .as_ref()
            .map(StudioShell::morph_thumbnail_payload)
            .unwrap_or_else(|| Err("Studio shell is not ready.".to_owned()));
        let result = payload.and_then(|(suggested_name, preview)| {
            let png = encode_morph_thumbnail_png(&preview)?;
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name(&suggested_name)
                .set_title("Generate morph thumbnail")
                .save_file()
            else {
                return Err("Thumbnail generation cancelled.".to_owned());
            };
            fs::write(&path, png)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))
        });
        if let Some(shell) = &mut self.shell {
            shell.set_morph_thumbnail_result(result);
        }
        self.request_redraw();
    }

    pub(crate) fn format_morph_diagnostics(
        diagnostics: &[cubacadabra_morphs::MorphDiagnostic],
    ) -> String {
        let summary = diagnostics
            .iter()
            .take(3)
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("; ");
        if diagnostics.len() > 3 {
            format!("{summary}; and {} more", diagnostics.len() - 3)
        } else {
            summary
        }
    }
}
