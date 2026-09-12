//! Strict source NORMAL import; no reconstruction or smoothing in the compiler.
use super::{MorphDiagnostic, accessor_layout, bounded_count, error, read_vec3};
use serde_json::Value;

pub(super) fn decode(
    primitive: &Value,
    accessors: &[Value],
    views: &[Value],
    bin: &[u8],
    vertex_count: usize,
) -> Result<Vec<[f32; 3]>, Vec<MorphDiagnostic>> {
    let path = "attributes.NORMAL";
    let index = primitive
        .get("attributes")
        .and_then(|a| a.get("NORMAL"))
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_NORMAL",
                path,
                "every LOD primitive must export authored NORMAL data",
            )]
        })?;
    let accessor = index
        .as_u64()
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| accessors.get(i))
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_INVALID_ACCESSOR",
                path,
                "invalid NORMAL accessor",
            )]
        })?;
    let count = bounded_count(accessor, "NORMAL")?;
    if accessor["type"] != "VEC3"
        || accessor["componentType"] != 5126
        || accessor.get("sparse").is_some()
        || accessor["normalized"] == true
        || count != vertex_count
    {
        return Err(vec![error(
            "MORPH_GLB_INVALID_NORMAL",
            path,
            "NORMAL must be a non-sparse float VEC3 for every POSITION",
        )]);
    }
    let layout = accessor_layout(accessor, views, bin, 12, "NORMAL")?;
    let view = &views[accessor["bufferView"].as_u64().unwrap() as usize];
    let view_end = view["byteOffset"]
        .as_u64()
        .unwrap_or(0)
        .checked_add(view["byteLength"].as_u64().unwrap_or(0))
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let mut normals = Vec::with_capacity(count);
    for i in 0..count {
        let start = i
            .checked_mul(layout.stride)
            .and_then(|n| layout.start.checked_add(n));
        let normal = start
            .filter(|s| s.checked_add(12).is_some_and(|e| e <= view_end))
            .and_then(|start| read_vec3(bin, start))
            .ok_or_else(|| {
                vec![error(
                    "MORPH_GLB_TRUNCATED_NORMAL",
                    path,
                    "NORMAL exceeds its buffer view",
                )]
            })?;
        let length_squared = normal.iter().map(|v| v * v).sum::<f32>();
        if !normal.iter().all(|v| v.is_finite()) || !(0.98..=1.02).contains(&length_squared) {
            return Err(vec![error(
                "MORPH_GLB_INVALID_NORMAL",
                path,
                "NORMAL must contain finite unit vectors",
            )]);
        }
        normals.push(normal);
    }
    Ok(normals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn imports_strided_authored_normals_without_welding_hard_edges() {
        let primitive = json!({"attributes":{"NORMAL":0}});
        let accessors =
            [json!({"bufferView":0,"componentType":5126,"count":2,"type":"VEC3","byteOffset":4})];
        let views = [json!({"byteOffset":4,"byteLength":36,"byteStride":16})];
        let mut bin = vec![0; 8];
        for normal in [[1.0f32, 0., 0.], [0., 0., -1.]] {
            for value in normal {
                bin.extend_from_slice(&value.to_le_bytes());
            }
            bin.extend_from_slice(&[0; 4]);
        }
        assert_eq!(
            decode(&primitive, &accessors, &views, &bin, 2).unwrap(),
            [[1., 0., 0.], [0., 0., -1.]]
        );
        let shortened = [json!({"byteOffset":4,"byteLength":20,"byteStride":16})];
        assert_eq!(
            decode(&primitive, &accessors, &shortened, &bin, 2).unwrap_err()[0].code,
            "MORPH_GLB_TRUNCATED_NORMAL"
        );
        assert!(decode(&primitive, &accessors, &views, &bin[..20], 2).is_err());
    }

    #[test]
    fn rejects_absent_mismatched_sparse_and_nonunit_source_normals() {
        assert_eq!(
            decode(&json!({"attributes":{}}), &[], &[], &[], 3).unwrap_err()[0].code,
            "MORPH_GLB_MISSING_NORMAL"
        );
        let primitive = json!({"attributes":{"NORMAL":0}});
        let accessor = json!({"bufferView":0,"componentType":5126,"count":1,"type":"VEC3"});
        let views = [json!({"byteLength":12})];
        for invalid in [
            [0.0f32; 3],
            [2., 0., 0.],
            [f32::NAN, 0., 1.],
            [f32::INFINITY, 0., 1.],
        ] {
            let bin: Vec<u8> = invalid.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(
                decode(&primitive, std::slice::from_ref(&accessor), &views, &bin, 1).unwrap_err()
                    [0]
                .code,
                "MORPH_GLB_INVALID_NORMAL"
            );
        }
        let mut sparse = accessor.clone();
        sparse["sparse"] = json!({});
        assert!(decode(&primitive, &[sparse], &views, &[0; 12], 1).is_err());
        assert!(decode(&primitive, &[accessor], &views, &[0; 12], 2).is_err());
    }
}
