//! Studio's editable appearance and catalog UI. Categories organize browsing;
//! occupied slots and explicit conflicts alone determine what can coexist.
use cubacadabra_morphs::{
    MorphAssetDefinition, MorphAssetId, MorphAssetKind, MorphCatalog, MorphLoadout,
    MorphParameterValue, MorphPreset,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub const SKIN_TONES: [&str; 12] = [
    "#f6dcc6", "#efc5ad", "#e4b798", "#d6a57e", "#c59270", "#b88260", "#a86f50", "#966044",
    "#805039", "#69412f", "#533326", "#3d251f",
];

#[derive(Clone, Debug)]
pub enum Request {
    Preset(MorphAssetId),
    Equip(MorphAssetId),
    Remove(MorphAssetId),
    Clear(Vec<MorphAssetId>),
    Color(String, String),
    RetryCatalog,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Deserialize)]
struct RemoteCatalog {
    release: String,
    assets: Vec<RemoteAsset>,
    #[serde(default)]
    presets: Vec<MorphPreset>,
}

#[derive(Deserialize)]
struct RemoteAsset {
    definition: MorphAssetDefinition,
    delivery: String,
    artifact: Option<Artifact>,
}

pub struct PublishedCatalog {
    pub catalog: MorphCatalog,
    pub artifacts: BTreeMap<MorphAssetId, Artifact>,
}

impl PublishedCatalog {
    pub fn parse(source: &str) -> Result<Self, String> {
        let remote: RemoteCatalog = serde_json::from_str(source)
            .map_err(|error| format!("Invalid morph catalog: {error}"))?;
        let mut artifacts = BTreeMap::new();
        let mut assets = Vec::new();
        for row in remote.assets {
            match (row.delivery.as_str(), row.artifact) {
                ("morphpack", Some(artifact)) if valid_artifact(&artifact) => {
                    artifacts.insert(row.definition.id.clone(), artifact);
                }
                ("builtin", None) => {}
                _ => {
                    return Err(format!(
                        "Missing or invalid delivery for {}",
                        row.definition.id
                    ));
                }
            }
            assets.push(row.definition);
        }
        let catalog = MorphCatalog {
            schema_version: cubacadabra_morphs::MORPH_CATALOG_SCHEMA_VERSION,
            content_version: remote.release,
            assets,
            presets: remote.presets,
        };
        let diagnostics = catalog.validate();
        if !diagnostics.is_empty() {
            return Err(diagnostics
                .iter()
                .take(3)
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>()
                .join("; "));
        }
        // Validate every recipe, even when it is not the initially selected one.
        let capabilities = cubacadabra_morphs::CapabilitySet::new(
            catalog
                .assets
                .iter()
                .flat_map(|asset| asset.required_capabilities.iter().cloned()),
        );
        for preset in &catalog.presets {
            if preset
                .thumbnail
                .as_deref()
                .is_some_and(|url| !valid_thumbnail(url))
            {
                return Err(format!("Invalid thumbnail for {}", preset.display_name));
            }
            cubacadabra_morphs::resolve_preset(&catalog, &preset.id, &capabilities)
                .map_err(|d| format!("Invalid {}: {}", preset.display_name, d[0].message))?;
        }
        Ok(Self { catalog, artifacts })
    }
}

fn valid_thumbnail(url: &str) -> bool {
    let Some(hash) = url
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".png"))
    else {
        return false;
    };
    hash.len() == 64
        && hash
            .bytes()
            .all(|v| v.is_ascii_hexdigit() && !v.is_ascii_uppercase())
        && url == format!("/morphs/thumbnails/sha256/{}/{}.png", &hash[..2], hash)
}

fn valid_artifact(artifact: &Artifact) -> bool {
    artifact.bytes > 0
        && artifact.bytes <= cubacadabra_morphs::MAX_MORPH_PACK_BYTES
        && artifact.sha256.len() == 64
        && artifact
            .sha256
            .bytes()
            .all(|v| v.is_ascii_hexdigit() && !v.is_ascii_uppercase())
        && artifact.url
            == format!(
                "/morphs/packs/sha256/{}/{}.morphpack",
                &artifact.sha256[..2],
                artifact.sha256
            )
}

pub fn optional(asset: &MorphAssetDefinition) -> bool {
    !matches!(
        asset.kind,
        MorphAssetKind::Base
            | MorphAssetKind::Top
            | MorphAssetKind::Bottom
            | MorphAssetKind::Footwear
            | MorphAssetKind::OnePiece
            | MorphAssetKind::Outfit
            | MorphAssetKind::Face
    )
}

pub fn conflicts(left: &MorphAssetDefinition, right: &MorphAssetDefinition) -> bool {
    left.occupied_slots
        .iter()
        .any(|slot| right.occupied_slots.contains(slot) || right.conflicts.contains(slot))
        || right
            .occupied_slots
            .iter()
            .any(|slot| left.conflicts.contains(slot))
}

pub fn edit(
    catalog: &MorphCatalog,
    current: &MorphLoadout,
    request: &Request,
) -> Result<MorphLoadout, String> {
    let mut next = current.clone();
    match request {
        Request::Preset(id) => {
            next = catalog
                .presets
                .iter()
                .find(|preset| preset.id == *id)
                .ok_or("That starter is no longer in the catalog.")?
                .loadout();
        }
        Request::Equip(id) => {
            let asset = catalog
                .asset(id)
                .ok_or("That part is no longer in the catalog.")?;
            match asset.kind {
                MorphAssetKind::Base => {
                    // Keep the previous appearance if this would silently strip it.
                    if next.parts.iter().chain(next.face.iter()).any(|id| {
                        catalog
                            .asset(id)
                            .is_some_and(|part| !part.supported_bases.contains(&asset.id))
                    }) {
                        return Err("This body does not fit the current parts. Choose a compatible starter first.".into());
                    }
                    next.base = id.clone();
                }
                MorphAssetKind::Face => next.face = Some(id.clone()),
                _ => {
                    if !asset.supported_bases.contains(&next.base) {
                        return Err("This part does not fit the current body.".into());
                    }
                    next.parts.retain(|part| {
                        catalog
                            .asset(part)
                            .is_none_or(|other| !conflicts(asset, other))
                    });
                    next.parts.push(id.clone());
                }
            }
        }
        Request::Remove(id) => {
            let asset = catalog
                .asset(id)
                .ok_or("That part is no longer in the catalog.")?;
            if !optional(asset) {
                return Err("Choose a replacement for this part.".into());
            }
            next.parts.retain(|part| part != id);
        }
        Request::Clear(ids) => {
            for id in ids {
                next = edit(catalog, &next, &Request::Remove(id.clone()))?;
            }
        }
        Request::Color(channel, value) => {
            next.parameters
                .insert(channel.clone(), MorphParameterValue::Text(value.clone()));
        }
        Request::RetryCatalog => {}
    }
    next.canonicalize();
    Ok(next)
}

pub fn selected_ids(loadout: &MorphLoadout) -> BTreeSet<MorphAssetId> {
    std::iter::once(&loadout.base)
        .chain(loadout.parts.iter())
        .chain(loadout.face.iter())
        .cloned()
        .collect()
}

pub fn matches_preset(loadout: &MorphLoadout, preset: &MorphPreset) -> bool {
    loadout.base == preset.base
        && loadout.face == preset.face
        && loadout.parameters == preset.parameters
        && loadout.parts.iter().collect::<BTreeSet<_>>()
            == preset.parts.iter().collect::<BTreeSet<_>>()
}
