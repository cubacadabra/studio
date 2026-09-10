//! Studio-facing access to the shared morph catalog contract.
//!
//! Keeping this adapter independent from the engine makes it possible for the
//! future Morphs workspace to inspect drafts and diagnostics before anything
//! is uploaded to the renderer.

use cubacadabra_morph_authoring::{
    MorphAttachment, MorphAttachmentMode, MorphGeometrySource, MorphGlbInspection,
    MorphSourceInspection, decode_glb_preview, inspect_glb_bytes, inspect_glb_source,
    parse_source_manifest,
};
pub(crate) use cubacadabra_morph_authoring::{
    MorphGlbPreviewMesh, MorphGlbSourceSummary, MorphSourceManifest,
};
use cubacadabra_morphs::{
    CapabilitySet, MorphAssetDefinition, MorphAssetId, MorphCatalog, MorphDiagnostic,
    MorphLodBudget, MorphProvenance, MorphSourceReference, ResolvedMorphLoadout, parse_catalog,
    resolve_preset,
};
use std::collections::BTreeMap;

#[allow(dead_code)]
pub(crate) fn inspect_catalog(source: &str) -> Result<MorphCatalog, Vec<MorphDiagnostic>> {
    parse_catalog(source)
}

#[allow(dead_code)]
pub(crate) fn inspect_source_manifest(
    source: &str,
) -> Result<MorphSourceInspection, Vec<MorphDiagnostic>> {
    parse_source_manifest(source)?.inspect()
}

#[allow(dead_code)]
pub(crate) fn inspect_source_glb(
    manifest_source: &str,
    glb: &[u8],
) -> Result<MorphGlbInspection, Vec<MorphDiagnostic>> {
    let manifest = parse_source_manifest(manifest_source)?;
    inspect_glb_bytes(&manifest, glb)
}

/// Decode a bounded CPU preview for the Studio viewport. Runtime rendering
/// still consumes compiled morph packs; this adapter keeps authoring concerns
/// out of the shared client and engine paths.
pub(crate) fn decode_source_glb_preview(
    glb: &[u8],
) -> Result<MorphGlbPreviewMesh, Vec<MorphDiagnostic>> {
    decode_glb_preview(glb)
}

pub(crate) fn inspect_source_glb_structure(
    glb: &[u8],
) -> Result<MorphGlbSourceSummary, Vec<MorphDiagnostic>> {
    inspect_glb_source(glb)
}

pub(crate) fn source_manifest_geometry_file(
    source: &str,
) -> Result<String, Vec<MorphDiagnostic>> {
    Ok(parse_source_manifest(source)?.geometry.file)
}

pub(crate) fn inspect_source_sidecar(
    manifest_source: &str,
    glb: &[u8],
) -> Result<
    (
        MorphSourceManifest,
        MorphGlbPreviewMesh,
        MorphGlbSourceSummary,
    ),
    Vec<MorphDiagnostic>,
> {
    let manifest = parse_source_manifest(manifest_source)?;
    inspect_glb_bytes(&manifest, glb)?;
    let preview = decode_glb_preview(glb)?;
    let summary = inspect_glb_source(glb)?;
    Ok((manifest, preview, summary))
}

pub(crate) fn build_source_manifest_json(
    asset: &MorphAssetDefinition,
    geometry_file: String,
    attachment_joint: &str,
    lod_nodes: [&str; 3],
    triangle_counts: [u32; 3],
) -> Result<String, Vec<MorphDiagnostic>> {
    let mut asset = asset.clone();
    asset.source = Some(MorphSourceReference {
        geometry: geometry_file.clone(),
    });
    let geometry = MorphGeometrySource {
        file: geometry_file,
        lod_nodes: BTreeMap::from([
            ("near".to_owned(), lod_nodes[0].to_owned()),
            ("mid".to_owned(), lod_nodes[1].to_owned()),
            ("far".to_owned(), lod_nodes[2].to_owned()),
        ]),
        triangle_counts: BTreeMap::from([
            ("near".to_owned(), triangle_counts[0]),
            ("mid".to_owned(), triangle_counts[1]),
            ("far".to_owned(), triangle_counts[2]),
        ]),
    };
    let manifest = MorphSourceManifest {
        schema_version: cubacadabra_morph_authoring::MORPH_SOURCE_SCHEMA_VERSION,
        asset,
        geometry,
        attachment: MorphAttachment {
            mode: MorphAttachmentMode::Rigid,
            joint: attachment_joint.to_owned(),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        },
    };
    let diagnostics = manifest.validate();
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    serde_json::to_string_pretty(&manifest).map_err(|error| {
        vec![MorphDiagnostic {
            code: "MORPH_SOURCE_SERIALIZE_FAILED".to_owned(),
            path: "$".to_owned(),
            message: error.to_string(),
        }]
    })
}

pub(crate) fn default_rigid_accessory_asset(
    source_path: &str,
    triangle_count: u32,
) -> MorphAssetDefinition {
    let stem = std::path::Path::new(source_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("imported-accessory");
    let mut slug = stem
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                byte.to_ascii_lowercase() as char
            } else {
                '-'
            }
        })
        .collect::<String>();
    slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        slug = "imported-accessory".to_owned();
    }
    if slug.len() > 64 {
        slug.truncate(64);
        slug = slug.trim_matches('-').to_owned();
    }
    let display_name = slug
        .split('-')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    MorphAssetDefinition {
        id: MorphAssetId::parse(format!("cuba:headwear/{slug}.v1"))
            .expect("generated accessory ID must be valid"),
        kind: cubacadabra_morphs::MorphAssetKind::Headwear,
        display_name,
        rig_profile: Some(
            MorphAssetId::parse("cuba:rig/biped15.v1")
                .expect("built-in rig ID must be valid"),
        ),
        fit_profiles: vec![
            MorphAssetId::parse("cuba:fit/person-standard.v1")
                .expect("built-in fit ID must be valid"),
        ],
        supported_bases: vec![
            MorphAssetId::parse("cuba:base/person.v1")
                .expect("built-in base ID must be valid"),
        ],
        occupied_slots: vec!["headwear".to_owned()],
        coverage: vec!["head".to_owned()],
        conflicts: Vec::new(),
        materials: vec!["default".to_owned()],
        lod: MorphLodBudget {
            near: triangle_count,
            mid: triangle_count,
            far: triangle_count,
        },
        required_capabilities: vec![
            cubacadabra_morphs::CapabilityId::parse("mesh.rigid.v1")
                .expect("built-in capability ID must be valid"),
        ],
        source: None,
        provenance: MorphProvenance {
            source: "Studio GLB import".to_owned(),
            license: "Unreviewed".to_owned(),
        },
    }
}

#[allow(dead_code)]
pub(crate) fn resolve_catalog_preset(
    catalog: &MorphCatalog,
    preset_id: &MorphAssetId,
    capabilities: &CapabilitySet,
) -> Result<ResolvedMorphLoadout, Vec<MorphDiagnostic>> {
    resolve_preset(catalog, preset_id, capabilities)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPATIBILITY_CATALOG: &str =
        include_str!("../../rust/assets/characters/morph_catalog.json");

    #[test]
    fn studio_can_inspect_the_shared_catalog_without_engine_internals() {
        let catalog = inspect_catalog(COMPATIBILITY_CATALOG).expect("compatibility catalog");
        assert_eq!(catalog.presets.len(), 3);
        assert_eq!(catalog.assets.len(), 34);
    }

    #[test]
    fn studio_receives_structured_catalog_diagnostics() {
        let diagnostics =
            inspect_catalog(r#"{"schemaVersion":1,"contentVersion":"","assets":[],"presets":[]}"#)
                .unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "MORPH_CATALOG_INVALID_CONTENT_VERSION" })
        );
    }

    #[test]
    fn studio_uses_shared_catalog_resolution() {
        let catalog = inspect_catalog(COMPATIBILITY_CATALOG).expect("compatibility catalog");
        let preset_id = MorphAssetId::parse("cuba:preset/person-boy.v1").unwrap();
        let capabilities = CapabilitySet::new([
            cubacadabra_morphs::CapabilityId::parse("mesh.rigid.v1").unwrap(),
            cubacadabra_morphs::CapabilityId::parse("face.analytic.v1").unwrap(),
            cubacadabra_morphs::CapabilityId::parse("secondary.chain.v1").unwrap(),
        ]);
        let resolved = resolve_catalog_preset(&catalog, &preset_id, &capabilities).unwrap();
        assert_eq!(resolved.base.as_str(), "cuba:base/person.v1");
        assert_eq!(resolved.fit_profile.as_str(), "cuba:fit/person-standard.v1");
    }

    #[test]
    fn studio_can_build_a_valid_sidecar_for_a_catalog_part() {
        let catalog = inspect_catalog(COMPATIBILITY_CATALOG).expect("compatibility catalog");
        let asset_id = MorphAssetId::parse("cuba:hair/swept.v1").unwrap();
        let asset = catalog.asset(&asset_id).expect("catalog hair asset");
        let source = build_source_manifest_json(
            asset,
            "test_top_hat.glb".to_owned(),
            "head",
            ["Near", "Mid", "Far"],
            [248; 3],
        )
        .expect("valid sidecar");
        let manifest = parse_source_manifest(&source).expect("round-trip sidecar");
        assert_eq!(manifest.geometry.file, "test_top_hat.glb");
        assert_eq!(manifest.geometry.triangle_counts["near"], 248);
        assert_eq!(manifest.attachment.joint, "head");
    }

    #[test]
    fn imported_accessory_gets_a_stable_draft_identity() {
        let asset = default_rigid_accessory_asset("/tmp/Test Top Hat.glb", 248);
        assert_eq!(asset.id.as_str(), "cuba:headwear/test-top-hat.v1");
        assert_eq!(asset.display_name, "Test Top Hat");
        assert!(asset.validate().is_empty());
    }

    #[test]
    fn sidecar_geometry_path_is_read_from_the_validated_manifest() {
        let source = r#"{
            "schemaVersion": 1,
            "asset": {
                "id": "cuba:headwear/test-top-hat.v1",
                "kind": "headwear",
                "displayName": "Test Top Hat",
                "rigProfile": "cuba:rig/biped15.v1",
                "fitProfiles": ["cuba:fit/person-standard.v1"],
                "supportedBases": ["cuba:base/person.v1"],
                "occupiedSlots": ["headwear"],
                "coverage": ["head"],
                "materials": ["default"],
                "lod": {"near": 248, "mid": 248, "far": 248},
                "source": {"geometry": "models/test_top_hat.glb"},
                "provenance": {"source": "Studio GLB import", "license": "Unreviewed"}
            },
            "geometry": {
                "file": "models/test_top_hat.glb",
                "lodNodes": {"near": "Near", "mid": "Mid", "far": "Far"},
                "triangleCounts": {"near": 248, "mid": 248, "far": 248}
            },
            "attachment": {"mode": "rigid", "joint": "head"}
        }"#;
        assert_eq!(
            source_manifest_geometry_file(source).unwrap(),
            "models/test_top_hat.glb"
        );
    }
}
