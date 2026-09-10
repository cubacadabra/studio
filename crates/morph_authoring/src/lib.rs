//! Studio-only source manifests for authored morph assets.
//!
//! This authoring slice validates the sidecar contract around a Blender export
//! and inspects its GLB container/JSON LOD metadata. It intentionally does not
//! decode vertex buffers or compile a runtime pack yet.

use cubacadabra_morphs::{MorphAssetDefinition, MorphAssetKind, MorphDiagnostic};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MORPH_SOURCE_SCHEMA_VERSION: u16 = 1;
pub const MAX_SOURCE_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_SOURCE_GLB_BYTES: usize = 64 * 1024 * 1024;
const MAX_NODE_NAME_BYTES: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MorphAttachmentMode {
    Rigid,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MorphAttachment {
    pub mode: MorphAttachmentMode,
    pub joint: String,
    #[serde(default = "identity_translation")]
    pub translation: [f32; 3],
    #[serde(default = "identity_rotation")]
    pub rotation: [f32; 4],
    #[serde(default = "identity_scale")]
    pub scale: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MorphGeometrySource {
    pub file: String,
    pub lod_nodes: BTreeMap<String, String>,
    pub triangle_counts: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MorphSourceManifest {
    pub schema_version: u16,
    pub asset: MorphAssetDefinition,
    pub geometry: MorphGeometrySource,
    pub attachment: MorphAttachment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphSourceInspection {
    pub asset_id: String,
    pub asset_kind: MorphAssetKind,
    pub geometry_file: String,
    pub lod_count: usize,
    pub attachment_joint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphGlbInspection {
    pub version: u32,
    pub json_bytes: usize,
    pub bin_bytes: usize,
    pub lods: BTreeMap<String, MorphGlbLodInspection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphGlbLodInspection {
    pub node: String,
    pub mesh_index: usize,
    pub primitive_count: usize,
    pub triangle_count: u32,
}

impl MorphSourceManifest {
    pub fn validate(&self) -> Vec<MorphDiagnostic> {
        let mut diagnostics = Vec::new();
        if self.schema_version != MORPH_SOURCE_SCHEMA_VERSION {
            diagnostics.push(error(
                "MORPH_SOURCE_UNSUPPORTED_SCHEMA",
                "schemaVersion",
                format!("expected schema {MORPH_SOURCE_SCHEMA_VERSION}"),
            ));
        }
        diagnostics.extend(self.asset.validate());
        if self.asset.kind == MorphAssetKind::Base {
            diagnostics.push(error(
                "MORPH_SOURCE_BASE_NOT_ACCESSORY",
                "asset.kind",
                "the rigid accessory importer does not accept base assets",
            ));
        }
        validate_geometry(&self.geometry, &mut diagnostics);
        validate_attachment(&self.attachment, &mut diagnostics);
        if let Some(source) = &self.asset.source {
            if source.geometry != self.geometry.file {
                diagnostics.push(error(
                    "MORPH_SOURCE_GEOMETRY_PATH_MISMATCH",
                    "asset.source.geometry",
                    "asset source geometry must match geometry.file",
                ));
            }
        } else {
            diagnostics.push(error(
                "MORPH_SOURCE_MISSING_GEOMETRY_REFERENCE",
                "asset.source",
                "source manifests must declare asset.source.geometry",
            ));
        }
        diagnostics
    }

    pub fn inspect(&self) -> Result<MorphSourceInspection, Vec<MorphDiagnostic>> {
        let diagnostics = self.validate();
        if diagnostics.is_empty() {
            Ok(MorphSourceInspection {
                asset_id: self.asset.id.to_string(),
                asset_kind: self.asset.kind,
                geometry_file: self.geometry.file.clone(),
                lod_count: self.geometry.lod_nodes.len(),
                attachment_joint: self.attachment.joint.clone(),
            })
        } else {
            Err(diagnostics)
        }
    }
}

pub fn parse_source_manifest(source: &str) -> Result<MorphSourceManifest, Vec<MorphDiagnostic>> {
    if source.len() > MAX_SOURCE_MANIFEST_BYTES {
        return Err(vec![error(
            "MORPH_SOURCE_TOO_LARGE",
            "$",
            format!("source manifest exceeds {MAX_SOURCE_MANIFEST_BYTES} bytes"),
        )]);
    }
    let manifest: MorphSourceManifest = serde_json::from_str(source).map_err(|parse_error| {
        vec![error(
            "MORPH_SOURCE_INVALID_JSON",
            "$",
            parse_error.to_string(),
        )]
    })?;
    let diagnostics = manifest.validate();
    if diagnostics.is_empty() {
        Ok(manifest)
    } else {
        Err(diagnostics)
    }
}

/// Inspect the JSON and container structure of a GLB referenced by a source
/// manifest. This deliberately stops before decoding vertex buffers; the
/// compiler can add that step once the contract is stable without changing
/// the manifest API.
pub fn inspect_glb_bytes(
    manifest: &MorphSourceManifest,
    bytes: &[u8],
) -> Result<MorphGlbInspection, Vec<MorphDiagnostic>> {
    let mut diagnostics = manifest.validate();
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    if bytes.len() > MAX_SOURCE_GLB_BYTES {
        diagnostics.push(error(
            "MORPH_GLB_TOO_LARGE",
            "geometry.file",
            format!("GLB exceeds {MAX_SOURCE_GLB_BYTES} bytes"),
        ));
        return Err(diagnostics);
    }
    if !manifest.geometry.file.ends_with(".glb") {
        diagnostics.push(error(
            "MORPH_GLB_UNSUPPORTED_SOURCE",
            "geometry.file",
            "binary inspection currently accepts only .glb sources",
        ));
        return Err(diagnostics);
    }
    let (version, json, bin_bytes) = match read_glb_chunks(bytes) {
        Ok(value) => value,
        Err(diagnostic) => return Err(vec![diagnostic]),
    };
    let document: serde_json::Value = match serde_json::from_slice(&json) {
        Ok(value) => value,
        Err(parse_error) => {
            return Err(vec![error(
                "MORPH_GLB_INVALID_JSON",
                "geometry.file",
                parse_error.to_string(),
            )]);
        }
    };
    let nodes = document
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_NODES",
                "geometry.file",
                "GLB JSON must contain a nodes array",
            )]
        })?;
    let meshes = document
        .get("meshes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_MESHES",
                "geometry.file",
                "GLB JSON must contain a meshes array",
            )]
        })?;
    let accessors = document
        .get("accessors")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_ACCESSORS",
                "geometry.file",
                "GLB JSON must contain an accessors array",
            )]
        })?;

    let mut lods = BTreeMap::new();
    for level in ["near", "mid", "far"] {
        let node_name = manifest.geometry.lod_nodes[level].as_str();
        let matching_nodes = nodes
            .iter()
            .filter(|node| node.get("name").and_then(serde_json::Value::as_str) == Some(node_name))
            .collect::<Vec<_>>();
        if matching_nodes.len() != 1 {
            diagnostics.push(error(
                "MORPH_GLB_LOD_NODE_NOT_UNIQUE",
                &format!("geometry.lodNodes.{level}"),
                format!("expected exactly one node named {node_name:?}"),
            ));
            continue;
        }
        let node = matching_nodes[0];
        let Some(mesh_index) = node.get("mesh").and_then(serde_json::Value::as_u64) else {
            diagnostics.push(error(
                "MORPH_GLB_LOD_NODE_MISSING_MESH",
                &format!("geometry.lodNodes.{level}"),
                "LOD nodes must reference a mesh",
            ));
            continue;
        };
        let Ok(mesh_index) = usize::try_from(mesh_index) else {
            diagnostics.push(error(
                "MORPH_GLB_INVALID_MESH_INDEX",
                &format!("geometry.lodNodes.{level}"),
                "mesh index is too large",
            ));
            continue;
        };
        let Some(mesh) = meshes.get(mesh_index) else {
            diagnostics.push(error(
                "MORPH_GLB_INVALID_MESH_INDEX",
                &format!("geometry.lodNodes.{level}"),
                "LOD node references a missing mesh",
            ));
            continue;
        };
        let Some(primitives) = mesh.get("primitives").and_then(serde_json::Value::as_array) else {
            diagnostics.push(error(
                "MORPH_GLB_MISSING_PRIMITIVES",
                &format!("geometry.lodNodes.{level}"),
                "LOD mesh must contain primitives",
            ));
            continue;
        };
        let mut triangle_count = 0u32;
        let mut valid = true;
        for (primitive_index, primitive) in primitives.iter().enumerate() {
            let mode = primitive
                .get("mode")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(4);
            let count = primitive
                .get("indices")
                .and_then(serde_json::Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| accessors.get(index))
                .and_then(|accessor| accessor.get("count"))
                .and_then(serde_json::Value::as_u64)
                .or_else(|| {
                    primitive
                        .get("attributes")
                        .and_then(|attributes| attributes.get("POSITION"))
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|index| usize::try_from(index).ok())
                        .and_then(|index| accessors.get(index))
                        .and_then(|accessor| accessor.get("count"))
                        .and_then(serde_json::Value::as_u64)
                });
            let Some(count) = count else {
                diagnostics.push(error(
                    "MORPH_GLB_PRIMITIVE_MISSING_COUNT",
                    &format!("geometry.lodNodes.{level}.primitives[{primitive_index}]"),
                    "primitive needs an index or POSITION accessor with a count",
                ));
                valid = false;
                continue;
            };
            let Some(triangles) = triangle_count_for_mode(mode, count) else {
                diagnostics.push(error(
                    "MORPH_GLB_UNSUPPORTED_PRIMITIVE_MODE",
                    &format!("geometry.lodNodes.{level}.primitives[{primitive_index}].mode"),
                    "only triangles, triangle strips, and triangle fans are supported",
                ));
                valid = false;
                continue;
            };
            let Ok(triangles) = u32::try_from(triangles) else {
                diagnostics.push(error(
                    "MORPH_GLB_TRIANGLE_COUNT_OVERFLOW",
                    &format!("geometry.lodNodes.{level}"),
                    "triangle count exceeds the supported limit",
                ));
                valid = false;
                continue;
            };
            triangle_count = triangle_count.saturating_add(triangles);
        }
        if valid {
            let expected = manifest.geometry.triangle_counts[level];
            if triangle_count != expected {
                diagnostics.push(error(
                    "MORPH_GLB_TRIANGLE_COUNT_MISMATCH",
                    &format!("geometry.triangleCounts.{level}"),
                    format!("manifest declares {expected}, GLB reports {triangle_count}"),
                ));
            }
            lods.insert(
                level.to_owned(),
                MorphGlbLodInspection {
                    node: node_name.to_owned(),
                    mesh_index,
                    primitive_count: primitives.len(),
                    triangle_count,
                },
            );
        }
    }
    if diagnostics.is_empty() {
        Ok(MorphGlbInspection {
            version,
            json_bytes: json.len(),
            bin_bytes,
            lods,
        })
    } else {
        Err(diagnostics)
    }
}

fn triangle_count_for_mode(mode: u64, count: u64) -> Option<u64> {
    match mode {
        4 => Some(count / 3),
        5 | 6 if count >= 3 => Some(count - 2),
        5 | 6 => Some(0),
        _ => None,
    }
}

fn read_glb_chunks(bytes: &[u8]) -> Result<(u32, Vec<u8>, usize), MorphDiagnostic> {
    if bytes.len() < 12 {
        return Err(error(
            "MORPH_GLB_TRUNCATED",
            "geometry.file",
            "GLB header is truncated",
        ));
    }
    if &bytes[0..4] != b"glTF" {
        return Err(error(
            "MORPH_GLB_INVALID_MAGIC",
            "geometry.file",
            "GLB magic must be glTF",
        ));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("checked header length"));
    if version != 2 {
        return Err(error(
            "MORPH_GLB_UNSUPPORTED_VERSION",
            "geometry.file",
            "only GLB version 2 is supported",
        ));
    }
    let declared_length =
        u32::from_le_bytes(bytes[8..12].try_into().expect("checked header length"));
    let Ok(declared_length) = usize::try_from(declared_length) else {
        return Err(error(
            "MORPH_GLB_INVALID_LENGTH",
            "geometry.file",
            "GLB length is too large",
        ));
    };
    if declared_length != bytes.len() {
        return Err(error(
            "MORPH_GLB_INVALID_LENGTH",
            "geometry.file",
            "declared GLB length does not match the file",
        ));
    }
    let mut offset = 12;
    let mut json = None;
    let mut bin_bytes = 0usize;
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            return Err(error(
                "MORPH_GLB_TRUNCATED_CHUNK",
                "geometry.file",
                "GLB chunk header is truncated",
            ));
        }
        let chunk_length = u32::from_le_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("checked chunk header length"),
        );
        let Ok(chunk_length) = usize::try_from(chunk_length) else {
            return Err(error(
                "MORPH_GLB_INVALID_CHUNK_LENGTH",
                "geometry.file",
                "GLB chunk length is too large",
            ));
        };
        let chunk_type = &bytes[offset + 4..offset + 8];
        let chunk_start = offset + 8;
        let Some(chunk_end) = chunk_start.checked_add(chunk_length) else {
            return Err(error(
                "MORPH_GLB_INVALID_CHUNK_LENGTH",
                "geometry.file",
                "GLB chunk length overflows the file",
            ));
        };
        if chunk_end > bytes.len() {
            return Err(error(
                "MORPH_GLB_TRUNCATED_CHUNK",
                "geometry.file",
                "GLB chunk extends past the file",
            ));
        }
        match chunk_type {
            b"JSON" if json.is_none() => json = Some(bytes[chunk_start..chunk_end].to_vec()),
            b"JSON" => {
                return Err(error(
                    "MORPH_GLB_MULTIPLE_JSON_CHUNKS",
                    "geometry.file",
                    "GLB must contain exactly one JSON chunk",
                ));
            }
            b"BIN\0" => bin_bytes = bin_bytes.saturating_add(chunk_length),
            _ => {}
        }
        offset = chunk_end;
    }
    let Some(mut json) = json else {
        return Err(error(
            "MORPH_GLB_MISSING_JSON",
            "geometry.file",
            "GLB must contain a JSON chunk",
        ));
    };
    while matches!(json.last(), Some(byte) if *byte == b'\0' || byte.is_ascii_whitespace()) {
        json.pop();
    }
    Ok((version, json, bin_bytes))
}

fn validate_geometry(source: &MorphGeometrySource, diagnostics: &mut Vec<MorphDiagnostic>) {
    let valid_path = !source.file.is_empty()
        && source.file.len() <= 240
        && source.file.is_ascii()
        && !source.file.starts_with('/')
        && !source.file.contains('\\')
        && source
            .file
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
    if !valid_path || !(source.file.ends_with(".glb") || source.file.ends_with(".gltf")) {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_GEOMETRY_PATH",
            "geometry.file",
            "geometry must be a safe relative .glb or .gltf path",
        ));
    }

    for level in ["near", "mid", "far"] {
        let node_path = format!("geometry.lodNodes.{level}");
        match source.lod_nodes.get(level) {
            Some(node) if !node.is_empty() && node.len() <= MAX_NODE_NAME_BYTES => {}
            Some(_) => diagnostics.push(error(
                "MORPH_SOURCE_INVALID_LOD_NODE",
                &node_path,
                "LOD node names must be 1..=96 bytes",
            )),
            None => diagnostics.push(error(
                "MORPH_SOURCE_MISSING_LOD_NODE",
                &node_path,
                "near, mid, and far LOD nodes are required",
            )),
        }
        if !source.triangle_counts.contains_key(level) {
            diagnostics.push(error(
                "MORPH_SOURCE_MISSING_TRIANGLE_COUNT",
                &format!("geometry.triangleCounts.{level}"),
                "near, mid, and far triangle counts are required",
            ));
        }
    }
    if source.lod_nodes.len() != 3 || source.triangle_counts.len() != 3 {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_LOD_COUNT",
            "geometry",
            "only near, mid, and far LOD entries are allowed",
        ));
    }
    let triangle = |level: &str| source.triangle_counts.get(level).copied().unwrap_or(0);
    if triangle("near") < triangle("mid")
        || triangle("mid") < triangle("far")
        || triangle("far") == 0
    {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_TRIANGLE_BUDGET",
            "geometry.triangleCounts",
            "triangle counts must be near >= mid >= far > 0",
        ));
    }
}

fn validate_attachment(attachment: &MorphAttachment, diagnostics: &mut Vec<MorphDiagnostic>) {
    if attachment.joint.is_empty()
        || attachment.joint.len() > MAX_NODE_NAME_BYTES
        || !attachment.joint.is_ascii()
        || !attachment.joint.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_ATTACHMENT_JOINT",
            "attachment.joint",
            "attachment joints must be lower-case semantic names",
        ));
    }
    if !attachment
        .translation
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 10.0)
    {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_ATTACHMENT_TRANSLATION",
            "attachment.translation",
            "translation values must be finite and within +/-10 units",
        ));
    }
    if !attachment.rotation.iter().all(|value| value.is_finite()) {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_ATTACHMENT_ROTATION",
            "attachment.rotation",
            "rotation values must be finite",
        ));
    } else {
        let length = attachment
            .rotation
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if !(0.99..=1.01).contains(&length) {
            diagnostics.push(error(
                "MORPH_SOURCE_INVALID_ATTACHMENT_ROTATION",
                "attachment.rotation",
                "rotation quaternion must be normalized",
            ));
        }
    }
    if !attachment
        .scale
        .iter()
        .all(|value| value.is_finite() && (0.01..=100.0).contains(value))
    {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_ATTACHMENT_SCALE",
            "attachment.scale",
            "scale values must be finite and within 0.01..=100",
        ));
    }
}

fn error(code: &str, path: &str, message: impl Into<String>) -> MorphDiagnostic {
    MorphDiagnostic {
        code: code.to_owned(),
        path: path.to_owned(),
        message: message.into(),
    }
}

fn identity_translation() -> [f32; 3] {
    [0.0; 3]
}

fn identity_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn identity_scale() -> [f32; 3] {
    [1.0; 3]
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubacadabra_morphs::{MorphAssetId, MorphLodBudget, MorphProvenance, MorphSourceReference};
    use serde_json::json;

    fn fixture() -> MorphSourceManifest {
        let id = MorphAssetId::parse("cuba:headwear/star-cap.v1").unwrap();
        MorphSourceManifest {
            schema_version: 1,
            asset: MorphAssetDefinition {
                id,
                kind: MorphAssetKind::Headwear,
                display_name: "Star Cap".to_owned(),
                rig_profile: Some(MorphAssetId::parse("cuba:rig/biped15.v1").unwrap()),
                fit_profiles: vec![MorphAssetId::parse("cuba:fit/person-standard.v1").unwrap()],
                supported_bases: vec![MorphAssetId::parse("cuba:base/person.v1").unwrap()],
                occupied_slots: vec!["headwear".to_owned()],
                coverage: vec!["scalp".to_owned()],
                conflicts: vec!["full-headwear".to_owned()],
                materials: vec!["cloth".to_owned()],
                lod: MorphLodBudget {
                    near: 1200,
                    mid: 500,
                    far: 100,
                },
                required_capabilities: vec![
                    cubacadabra_morphs::CapabilityId::parse("mesh.rigid.v1").unwrap(),
                ],
                source: Some(MorphSourceReference {
                    geometry: "star-cap.glb".to_owned(),
                }),
                provenance: MorphProvenance {
                    source: "Blender fixture".to_owned(),
                    license: "Cubacadabra official".to_owned(),
                },
            },
            geometry: MorphGeometrySource {
                file: "star-cap.glb".to_owned(),
                lod_nodes: BTreeMap::from([
                    ("near".to_owned(), "StarCap_LOD0".to_owned()),
                    ("mid".to_owned(), "StarCap_LOD1".to_owned()),
                    ("far".to_owned(), "StarCap_LOD2".to_owned()),
                ]),
                triangle_counts: BTreeMap::from([
                    ("near".to_owned(), 1200),
                    ("mid".to_owned(), 500),
                    ("far".to_owned(), 100),
                ]),
            },
            attachment: MorphAttachment {
                mode: MorphAttachmentMode::Rigid,
                joint: "head".to_owned(),
                translation: [0.0, 0.45, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
        }
    }

    #[test]
    fn rigid_fixture_inspects_cleanly() {
        let inspection = fixture().inspect().unwrap();
        assert_eq!(inspection.asset_id, "cuba:headwear/star-cap.v1");
        assert_eq!(inspection.lod_count, 3);
        assert_eq!(inspection.attachment_joint, "head");
    }

    #[test]
    fn parser_round_trips_the_source_manifest() {
        let source = serde_json::to_string(&fixture()).unwrap();
        let parsed = parse_source_manifest(&source).unwrap();
        assert_eq!(parsed, fixture());
    }

    #[test]
    fn rejects_missing_lods_and_unsafe_attachment_data() {
        let mut manifest = fixture();
        manifest.geometry.lod_nodes.remove("far");
        manifest.attachment.joint = "../head".to_owned();
        manifest.attachment.rotation = [0.0, 0.0, 0.0, 0.0];
        let diagnostics = manifest.validate();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == "MORPH_SOURCE_MISSING_LOD_NODE")
        );
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == "MORPH_SOURCE_INVALID_ATTACHMENT_JOINT")
        );
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == "MORPH_SOURCE_INVALID_ATTACHMENT_ROTATION")
        );
    }

    fn glb_fixture(manifest: &MorphSourceManifest, counts: [u64; 3]) -> Vec<u8> {
        let levels = ["near", "mid", "far"];
        let nodes = levels
            .iter()
            .enumerate()
            .map(|(index, level)| {
                json!({ "name": manifest.geometry.lod_nodes[*level], "mesh": index })
            })
            .collect::<Vec<_>>();
        let meshes = counts
            .iter()
            .enumerate()
            .map(|(index, _count)| {
                json!({
                    "name": format!("mesh-{index}"),
                    "primitives": [{
                        "attributes": { "POSITION": index },
                        "mode": 4
                    }]
                })
            })
            .collect::<Vec<_>>();
        let accessors = counts
            .iter()
            .map(|count| json!({ "count": count * 3, "type": "SCALAR" }))
            .collect::<Vec<_>>();
        let document = json!({
            "asset": { "version": "2.0" },
            "nodes": nodes,
            "meshes": meshes,
            "accessors": accessors
        });
        let mut json_bytes = serde_json::to_vec(&document).unwrap();
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        let total_length = 12 + 8 + json_bytes.len();
        let mut bytes = Vec::with_capacity(total_length);
        bytes.extend_from_slice(b"glTF");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(total_length as u32).to_le_bytes());
        bytes.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"JSON");
        bytes.extend_from_slice(&json_bytes);
        bytes
    }

    #[test]
    fn glb_fixture_matches_declared_lods_and_triangle_budgets() {
        let manifest = fixture();
        let glb = glb_fixture(&manifest, [1200, 500, 100]);
        let inspection = inspect_glb_bytes(&manifest, &glb).unwrap();
        assert_eq!(inspection.version, 2);
        assert_eq!(inspection.bin_bytes, 0);
        assert_eq!(inspection.lods["near"].triangle_count, 1200);
        assert_eq!(inspection.lods["far"].node, "StarCap_LOD2");
    }

    #[test]
    fn glb_fixture_rejects_triangle_budget_mismatch() {
        let manifest = fixture();
        let glb = glb_fixture(&manifest, [1201, 500, 100]);
        let diagnostics = inspect_glb_bytes(&manifest, &glb).unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.code == "MORPH_GLB_TRIANGLE_COUNT_MISMATCH"
                && item.path == "geometry.triangleCounts.near"
        }));
    }
}
