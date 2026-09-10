//! Studio-facing access to the shared morph catalog contract.
//!
//! Keeping this adapter independent from the engine makes it possible for the
//! future Morphs workspace to inspect drafts and diagnostics before anything
//! is uploaded to the renderer.

use cubacadabra_morph_authoring::{
    MorphGlbInspection, MorphSourceInspection, inspect_glb_bytes, parse_source_manifest,
};
use cubacadabra_morphs::{
    CapabilitySet, MorphAssetId, MorphCatalog, MorphDiagnostic, ResolvedMorphLoadout,
    parse_catalog, resolve_preset,
};

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
}
