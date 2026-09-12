use super::*;
use cubacadabra_morphs::{CapabilityId, CapabilitySet, MorphLoadout};

pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet::new(
        [
            "mesh.rigid.v1",
            "skin.biped15-linear.v1",
            "material.cuba-pbr.v1",
            "material.base-color-texture.v1",
            "face.analytic.v1",
            "face.authored-static.v1",
            "rig.canonical-rest.v1",
            "secondary.chain.v1",
            "material.emissive.v1",
            "hair.authored.v1",
            "accessory.ear-device.v1",
        ]
        .map(|id| CapabilityId::parse(id).unwrap()),
    )
}

impl StudioApp {
    pub(super) fn install_published_morphs(&mut self, source: &str) {
        let Some(shell) = &mut self.shell else {
            return;
        };
        match shell.set_remote_morph_catalog(source) {
            Ok(_) => {
                self.pending_morph = None;
                self.registered_morphs.clear();
                shell.set_morph_loading(false);
                let presets = shell.morph_catalog().presets.clone();
                // Apply the default only at startup; catalog retries must not
                // erase an appearance the user has already edited.
                if self.standalone_preview && self.morph_request_serial == 0 {
                    if let Some(preset) = presets.first() {
                        if let Err(message) =
                            self.request_morph_change(wardrobe::Request::Preset(preset.id.clone()))
                        {
                            self.shell.as_mut().unwrap().set_notice(message);
                        }
                    }
                }
                for preset in presets {
                    if let Some(url) = preset.thumbnail {
                        self.network.request_morph_thumbnail(url);
                    }
                }
            }
            Err(message) => {
                shell.set_catalog_error(message.clone());
                shell.set_notice(message);
            }
        }
    }

    pub(super) fn request_morph_change(
        &mut self,
        request: wardrobe::Request,
    ) -> Result<(), String> {
        let shell = self.shell.as_ref().ok_or("Morph shell is not ready.")?;
        let current = self
            .pending_morph
            .as_ref()
            .map(|(_, loadout)| loadout)
            .unwrap_or(&self.morph_loadout);
        let next = wardrobe::edit(shell.morph_catalog(), current, &request)?;
        validate(shell.morph_catalog(), &next)?;
        let assets: Vec<_> = wardrobe::selected_ids(&next)
            .into_iter()
            .filter(|id| !self.registered_morphs.contains(id.as_str()))
            .filter_map(|id| {
                shell
                    .morph_artifact(&id)
                    .cloned()
                    .map(|artifact| (id.to_string(), artifact))
            })
            .collect();
        self.morph_request_serial = self.morph_request_serial.wrapping_add(1);
        if let wardrobe::Request::Preset(id) = request {
            self.shell.as_mut().unwrap().select_morph(id);
        }
        let serial = self.morph_request_serial;
        self.pending_morph = None;
        self.shell
            .as_mut()
            .unwrap()
            .set_morph_loading(!assets.is_empty());
        if assets.is_empty() {
            self.apply_morph_loadout(next)?;
            self.shell
                .as_mut()
                .unwrap()
                .set_notice("Appearance updated".into());
        } else {
            self.pending_morph = Some((serial, next));
            self.network.request_morph_packs(serial, assets);
        }
        Ok(())
    }

    pub(super) fn finish_morph_change(
        &mut self,
        serial: u64,
        packs: Vec<(String, Vec<u8>)>,
    ) -> Result<(), String> {
        if !self
            .pending_morph
            .as_ref()
            .is_some_and(|(pending, _)| *pending == serial)
        {
            return Ok(()); // A slower request must never replace the latest choice.
        }
        let (_, next) = self.pending_morph.take().unwrap();
        self.shell.as_mut().unwrap().set_morph_loading(false);
        let decoded = packs
            .iter()
            .map(|(expected, bytes)| {
                let pack =
                    decode_morph_pack(bytes).map_err(|d| Self::format_morph_diagnostics(&d))?;
                if pack.asset.id.as_str() != expected {
                    return Err("Downloaded morph has the wrong asset ID.".into());
                }
                Ok(pack.asset.id.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let renderer = self.renderer.as_mut().ok_or("The renderer is not ready.")?;
        // Registration changes only the resource cache. The visible loadout is
        // committed once every resource has been successfully registered.
        for ((_, bytes), id) in packs.iter().zip(decoded) {
            renderer
                .register_morph_pack(bytes)
                .map_err(|d| Self::format_morph_diagnostics(&d))?;
            self.registered_morphs.insert(id);
        }
        self.apply_morph_loadout(next)?;
        self.shell
            .as_mut()
            .unwrap()
            .set_notice("Appearance updated".into());
        Ok(())
    }
}

fn validate(
    catalog: &cubacadabra_morphs::MorphCatalog,
    loadout: &MorphLoadout,
) -> Result<(), String> {
    cubacadabra_morphs::resolve_loadout(catalog, loadout, &capabilities())
        .map_err(|d| StudioApp::format_morph_diagnostics(&d))?;
    cubacadabra_morphs::project_v2_to_v1(catalog, loadout)
        .map_err(|d| StudioApp::format_morph_diagnostics(&d))?;
    Ok(())
}
