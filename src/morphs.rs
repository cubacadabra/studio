//! Studio-facing access to the shared morph catalog contract.
//!
//! Keeping this adapter independent from the engine makes it possible for the
//! future Morphs workspace to inspect drafts and diagnostics before anything
//! is uploaded to the renderer.

use cubacadabra_morph_authoring::{
    MorphAttachment, MorphAttachmentMode, MorphGeometrySource, MorphGlbInspection,
    MorphSourceInspection, compile_morph_pack, decode_glb_preview, decode_glb_preview_node,
    inspect_glb_bytes, inspect_glb_source, parse_source_manifest,
};
pub(crate) use cubacadabra_morph_authoring::{
    MorphGlbPreviewMesh, MorphGlbSourceSummary, MorphPackSummary, MorphSourceManifest,
};
use cubacadabra_morphs::{
    CapabilitySet, MorphAssetDefinition, MorphAssetId, MorphCatalog, MorphDiagnostic,
    MorphLodBudget, MorphProvenance, MorphSourceReference, ResolvedMorphLoadout, parse_catalog,
    resolve_preset,
};
use std::collections::BTreeMap;
use std::io::Cursor;

pub(crate) const MORPH_DRAFT_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MorphDraftDocument {
    pub(crate) asset: MorphAssetDefinition,
    pub(crate) geometry_file: String,
    pub(crate) attachment_joint: String,
    pub(crate) lod_nodes: [String; 3],
}

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

pub(crate) fn decode_source_glb_preview_node(
    glb: &[u8],
    node_name: &str,
) -> Result<MorphGlbPreviewMesh, Vec<MorphDiagnostic>> {
    decode_glb_preview_node(glb, Some(node_name))
}

pub(crate) fn inspect_source_glb_structure(
    glb: &[u8],
) -> Result<MorphGlbSourceSummary, Vec<MorphDiagnostic>> {
    inspect_glb_source(glb)
}

pub(crate) fn source_manifest_geometry_file(source: &str) -> Result<String, Vec<MorphDiagnostic>> {
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

pub(crate) fn compile_source_morph_pack(
    manifest_source: &str,
    glb: &[u8],
) -> Result<(Vec<u8>, MorphPackSummary), Vec<MorphDiagnostic>> {
    let manifest = parse_source_manifest(manifest_source)?;
    compile_morph_pack(&manifest, glb)
}

pub(crate) fn encode_morph_thumbnail_png(mesh: &MorphGlbPreviewMesh) -> Result<Vec<u8>, String> {
    const SIZE: u32 = 256;
    if mesh.vertices.is_empty() || mesh.indices.len() < 3 {
        return Err("The preview mesh has no triangles for a thumbnail.".to_owned());
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for vertex in &mesh.vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let span = (max[0] - min[0]).max(max[1] - min[1]).max(0.0001);
    let scale = 220.0 / span;
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
    let project = |vertex: [f32; 3]| {
        [
            128.0 + (vertex[0] - center[0]) * scale,
            128.0 - (vertex[1] - center[1]) * scale,
        ]
    };
    let base = mesh.base_color.unwrap_or([0.35, 0.55, 0.78, 1.0]);
    let mut image = image::RgbaImage::from_pixel(SIZE, SIZE, image::Rgba([24, 24, 28, 255]));
    let mut triangles = Vec::new();
    for triangle in mesh
        .indices
        .chunks(3)
        .filter(|triangle| triangle.len() == 3)
    {
        let Some(a) = mesh.vertices.get(triangle[0] as usize).copied() else {
            continue;
        };
        let Some(b) = mesh.vertices.get(triangle[1] as usize).copied() else {
            continue;
        };
        let Some(c) = mesh.vertices.get(triangle[2] as usize).copied() else {
            continue;
        };
        let normal = normalize3(cross3(sub3(b, a), sub3(c, a)));
        let brightness =
            (dot3(normal, normalize3([0.35, 0.75, 0.65])).abs() * 0.55 + 0.45).clamp(0.0, 1.0);
        triangles.push((
            (a[2] + b[2] + c[2]) / 3.0,
            [project(a), project(b), project(c)],
            brightness,
        ));
    }
    triangles.sort_by(|first, second| first.0.total_cmp(&second.0));
    for (_, points, brightness) in triangles {
        let area = edge(points[0], points[1], points[2]);
        // Avoid rasterizing faces that collapse to a sub-pixel sliver in the
        // fixed thumbnail view; they otherwise become visible one-pixel bars.
        if area.abs() < 1.0 {
            continue;
        }
        let min_x = points
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min)
            .floor() as i32;
        let max_x = points
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil() as i32;
        let min_y = points
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min)
            .floor() as i32;
        let max_y = points
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil() as i32;
        let fill = [
            (base[0] * brightness * 255.0) as u8,
            (base[1] * brightness * 255.0) as u8,
            (base[2] * brightness * 255.0) as u8,
            (base[3] * 255.0) as u8,
        ];
        for y in min_y.max(0)..=max_y.min(SIZE as i32 - 1) {
            for x in min_x.max(0)..=max_x.min(SIZE as i32 - 1) {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                let w0 = edge(points[1], points[2], point) / area;
                let w1 = edge(points[2], points[0], point) / area;
                let w2 = edge(points[0], points[1], point) / area;
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    image.put_pixel(x as u32, y as u32, image::Rgba(fill));
                }
            }
        }
    }
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| format!("could not encode thumbnail: {error}"))?;
    Ok(output.into_inner())
}

fn edge(a: [f32; 2], b: [f32; 2], point: [f32; 2]) -> f32 {
    (point[0] - a[0]) * (b[1] - a[1]) - (point[1] - a[1]) * (b[0] - a[0])
}

fn sub3(first: [f32; 3], second: [f32; 3]) -> [f32; 3] {
    [
        first[0] - second[0],
        first[1] - second[1],
        first[2] - second[2],
    ]
}

fn cross3(first: [f32; 3], second: [f32; 3]) -> [f32; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn dot3(first: [f32; 3], second: [f32; 3]) -> f32 {
    first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let length = dot3(value, value).sqrt();
    if length > f32::EPSILON {
        [value[0] / length, value[1] / length, value[2] / length]
    } else {
        [0.0, 1.0, 0.0]
    }
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
    asset.lod = MorphLodBudget {
        near: triangle_counts[0],
        mid: triangle_counts[1],
        far: triangle_counts[2],
    };
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

/// Serialize the in-progress Studio mapping without applying the publish-only
/// source-manifest validation. Drafts are intentionally not consumable by the
/// runtime compiler; they let an artist save work while a GLB still needs
/// LODs or other repairs.
pub(crate) fn build_morph_draft_json(
    asset: &MorphAssetDefinition,
    geometry_file: String,
    attachment_joint: &str,
    lod_nodes: [&str; 3],
    triangle_counts: [u32; 3],
) -> Result<String, String> {
    let mut asset = asset.clone();
    asset.source = Some(MorphSourceReference {
        geometry: geometry_file.clone(),
    });
    let asset = serde_json::to_value(asset)
        .map_err(|error| format!("could not serialize draft asset: {error}"))?;
    serde_json::to_string_pretty(&serde_json::json!({
        "draftSchemaVersion": MORPH_DRAFT_SCHEMA_VERSION,
        "asset": asset,
        "geometry": {
            "file": geometry_file,
            "lodNodes": {
                "near": lod_nodes[0],
                "mid": lod_nodes[1],
                "far": lod_nodes[2]
            },
            "triangleCounts": {
                "near": triangle_counts[0],
                "mid": triangle_counts[1],
                "far": triangle_counts[2]
            }
        },
        "attachment": {
            "mode": "rigid",
            "joint": attachment_joint,
            "translation": [0.0, 0.0, 0.0],
            "rotation": [0.0, 0.0, 0.0, 1.0],
            "scale": [1.0, 1.0, 1.0]
        }
    }))
    .map_err(|error| format!("could not serialize morph draft: {error}"))
}

pub(crate) fn is_morph_draft_json(source: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(source)
        .ok()
        .is_some_and(|value| value.get("draftSchemaVersion").is_some())
}

pub(crate) fn parse_morph_draft_json(source: &str) -> Result<MorphDraftDocument, String> {
    let root: serde_json::Value = serde_json::from_str(source)
        .map_err(|error| format!("draft is not valid JSON: {error}"))?;
    let version = root
        .get("draftSchemaVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "draft is missing draftSchemaVersion.".to_owned())?;
    if version != u64::from(MORPH_DRAFT_SCHEMA_VERSION) {
        return Err(format!(
            "unsupported morph draft schema {version}; expected {MORPH_DRAFT_SCHEMA_VERSION}."
        ));
    }
    let asset: MorphAssetDefinition = root
        .get("asset")
        .cloned()
        .ok_or_else(|| "draft is missing asset metadata.".to_owned())
        .and_then(|value| {
            serde_json::from_value(value)
                .map_err(|error| format!("draft asset metadata is invalid: {error}"))
        })?;
    let asset_diagnostics = asset.validate();
    if !asset_diagnostics.is_empty() {
        return Err(asset_diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("; "));
    }
    let geometry = root
        .get("geometry")
        .ok_or_else(|| "draft is missing geometry metadata.".to_owned())?;
    let geometry_file = geometry
        .get("file")
        .and_then(serde_json::Value::as_str)
        .filter(|file| is_safe_draft_geometry_file(file))
        .ok_or_else(|| "draft geometry.file must be a safe relative .glb path.".to_owned())?
        .to_owned();
    let lod_nodes = ["near", "mid", "far"].map(|level| {
        geometry
            .get("lodNodes")
            .and_then(|nodes| nodes.get(level))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    });
    let attachment_joint = root
        .get("attachment")
        .and_then(|attachment| attachment.get("joint"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Ok(MorphDraftDocument {
        asset,
        geometry_file,
        attachment_joint,
        lod_nodes,
    })
}

fn is_safe_draft_geometry_file(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 240
        && file.is_ascii()
        && !file.starts_with('/')
        && !file.contains('\\')
        && file
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        && file.ends_with(".glb")
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
            MorphAssetId::parse("cuba:rig/biped15.v1").expect("built-in rig ID must be valid"),
        ),
        fit_profiles: vec![
            MorphAssetId::parse("cuba:fit/person-standard.v1")
                .expect("built-in fit ID must be valid"),
        ],
        supported_bases: vec![
            MorphAssetId::parse("cuba:base/person.v1").expect("built-in base ID must be valid"),
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
            [248, 124, 48],
        )
        .expect("valid sidecar");
        let manifest = parse_source_manifest(&source).expect("round-trip sidecar");
        assert_eq!(manifest.geometry.file, "test_top_hat.glb");
        assert_eq!(manifest.geometry.triangle_counts["near"], 248);
        assert_eq!(manifest.asset.lod.near, 248);
        assert_eq!(manifest.asset.lod.mid, 124);
        assert_eq!(manifest.asset.lod.far, 48);
        assert_eq!(manifest.attachment.joint, "head");
    }

    #[test]
    fn studio_can_save_an_incomplete_morph_draft() {
        let asset = default_rigid_accessory_asset("test_top_hat.glb", 248);
        let source = build_morph_draft_json(
            &asset,
            "test_top_hat.glb".to_owned(),
            "head",
            ["", "", ""],
            [0, 0, 0],
        )
        .expect("draft should serialize before LOD mapping");
        let json: serde_json::Value = serde_json::from_str(&source).expect("draft JSON");
        assert_eq!(json["draftSchemaVersion"], MORPH_DRAFT_SCHEMA_VERSION);
        assert_eq!(json["geometry"]["lodNodes"]["near"], "");
        assert_eq!(json["attachment"]["joint"], "head");
    }

    #[test]
    fn studio_can_reopen_an_incomplete_morph_draft() {
        let asset = default_rigid_accessory_asset("test_top_hat.glb", 248);
        let source = build_morph_draft_json(
            &asset,
            "test_top_hat.glb".to_owned(),
            "head",
            ["", "", ""],
            [0, 0, 0],
        )
        .expect("draft should serialize");
        let draft = parse_morph_draft_json(&source).expect("draft should parse");
        assert_eq!(draft.asset.id, asset.id);
        assert_eq!(draft.geometry_file, "test_top_hat.glb");
        assert_eq!(draft.attachment_joint, "head");
        assert_eq!(
            draft.lod_nodes,
            [String::new(), String::new(), String::new()]
        );
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

    #[test]
    fn thumbnail_encoder_returns_png_bytes() {
        let preview = MorphGlbPreviewMesh {
            name: "triangle".to_owned(),
            vertices: vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2],
            base_color: Some([0.2, 0.4, 0.8, 1.0]),
        };
        let png = encode_morph_thumbnail_png(&preview).expect("thumbnail PNG");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
