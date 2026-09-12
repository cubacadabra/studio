#!/usr/bin/env python3
"""Generate a Blender-compatible skinned Person base GLB.

It contains the canonical 15-joint hierarchy, four-weight skin attributes, and
three separate LOD meshes. Studio compiles it into the shared schema 5 runtime
pack; a Blender-authored export can use the same sidecar contract.
"""

import json
import math
import struct
import sys
from pathlib import Path

STARTER_ROOT = Path(__file__).resolve().parents[2] / "tools" / "starter-set"
sys.path.insert(0, str(STARTER_ROOT))
from artwork.material_textures import ensure as ensure_material_textures


JOINTS = [
    ("root", None, (0.0, 0.0, 0.0)),
    ("torso", 0, (0.0, 1.68, 0.0)),
    ("head", 1, (0.0, 1.02, 0.0)),
    ("left-upper-arm", 1, (-0.59, -0.01, 0.0)),
    ("left-lower-arm", 3, (0.0, -0.58, 0.0)),
    ("left-hand", 4, (0.0, -0.48, -0.01)),
    ("right-upper-arm", 1, (0.59, -0.01, 0.0)),
    ("right-lower-arm", 6, (0.0, -0.58, 0.0)),
    ("right-hand", 7, (0.0, -0.48, -0.01)),
    ("left-upper-leg", 0, (-0.22, 0.87, 0.0)),
    ("left-lower-leg", 9, (0.0, -0.53, 0.0)),
    ("left-foot", 10, (0.0, -0.29, -0.13)),
    ("right-upper-leg", 0, (0.22, 0.87, 0.0)),
    ("right-lower-leg", 12, (0.0, -0.53, 0.0)),
    ("right-foot", 13, (0.0, -0.29, -0.13)),
]

HEAD_PROFILE = [
    (0.00, 0.18, 0.22), (0.06, 0.33, 0.34), (0.22, 0.46, 0.46),
    (0.45, 0.49, 0.49), (0.72, 0.48, 0.48), (0.91, 0.36, 0.36),
    (1.00, 0.025, 0.025),
]

PERSON_VARIANTS = {
    "person-01": {
        "id": "cuba:base/person.v1",
        "display_name": "Person 1",
        "skin_color": [0.91, 0.55, 0.39, 1.0],
        "head_profile": HEAD_PROFILE,
    },
    "person-02": {
        "id": "cuba:base/person-02.v1",
        "display_name": "Person 2",
        "skin_color": [0.38, 0.20, 0.12, 1.0],
        # A slightly broader cheek and jaw profile. These are individual
        # appearance dimensions, not a claim that facial shape defines race.
        "head_profile": [
            (0.00, 0.20, 0.23), (0.06, 0.35, 0.35), (0.22, 0.49, 0.47),
            (0.45, 0.52, 0.50), (0.72, 0.50, 0.50), (0.91, 0.39, 0.37),
            (1.00, 0.03, 0.025),
        ],
    },
}
TORSO_PROFILE = [
    (0.00, 0.37, 0.36), (0.09, 0.44, 0.44), (0.25, 0.48, 0.47),
    (0.55, 0.47, 0.45), (0.74, 0.48, 0.40), (0.91, 0.40, 0.31),
    (1.00, 0.19, 0.23),
]
SLEEVE_PROFILE = [
    (0.00, 0.31, 0.32), (0.12, 0.37, 0.38), (0.35, 0.40, 0.41),
    (0.57, 0.43, 0.43), (0.80, 0.42, 0.42), (0.93, 0.35, 0.35),
    (1.00, 0.12, 0.14),
]
POCKET_PROFILE = [
    (0.00, 0.34, 0.20), (0.13, 0.48, 0.43), (0.38, 0.48, 0.48),
    (0.72, 0.37, 0.45), (0.94, 0.29, 0.34), (1.00, 0.27, 0.20),
]
SHOE_PROFILE = [
    (0.00, 0.43, 0.46), (0.12, 0.49, 0.49), (0.38, 0.48, 0.48),
    (0.65, 0.41, 0.40), (0.85, 0.31, 0.29), (1.00, 0.22, 0.22),
]


def profile(points, t):
    upper = next(index for index in range(1, len(points)) if t <= points[index][0])
    lower = upper - 1
    a, b = points[lower], points[upper]
    span = b[0] - a[0]
    f = max(0.0, min(1.0, (t - a[0]) / span))

    def slope(index, axis):
        lo = max(0, index - 1)
        hi = min(len(points) - 1, index + 1)
        return (points[hi][axis] - points[lo][axis]) / (points[hi][0] - points[lo][0])

    def interpolate(axis):
        return (
            (2 * f**3 - 3 * f**2 + 1) * a[axis]
            + (f**3 - 2 * f**2 + f) * span * slope(lower, axis)
            + (-2 * f**3 + 3 * f**2) * b[axis]
            + (f**3 - f**2) * span * slope(upper, axis)
        )

    return interpolate(1), interpolate(2)


def signed_power(value, exponent):
    return math.copysign(abs(value) ** exponent, value)


def surface(shape, t, angle, head_profile):
    sin_angle, cos_angle = math.sin(angle), math.cos(angle)
    if shape == "head":
        radius_x, radius_z = profile(head_profile, t)
        exponent = 0.58
    elif shape == "torso":
        radius_x, radius_z = profile(TORSO_PROFILE, t)
        exponent = 0.66
    elif shape == "sleeve":
        radius_x, radius_z = profile(SLEEVE_PROFILE, t)
        exponent = 0.90
    elif shape == "pocket":
        radius_x, radius_z = profile(POCKET_PROFILE, t)
        exponent = 0.58
    elif shape == "shoe":
        radius_x, radius_z = profile(SHOE_PROFILE, t)
        exponent = 0.65
    elif shape == "shorts":
        radius_x, radius_z, exponent = 0.40 + 0.07 * math.sin(t * math.pi), 0.43, 0.65
    elif shape == "limb":
        radius_x, radius_z, exponent = 0.34 + 0.10 * math.sin(t * math.pi), 0.40, 0.86
    else:
        radius_x = radius_z = math.sqrt(max(0.001, 1.0 - (t * 2.0 - 1.0) ** 2)) * 0.49
        exponent = 0.95
    x = radius_x * signed_power(cos_angle, exponent)
    y = t - 0.5
    z = radius_z * signed_power(sin_angle, exponent)
    if shape == "shoe":
        z = (z + t * 0.14 - 0.03) * 0.95
        y -= max(-sin_angle, 0.0) * t * 0.15
    return x, y, z


def profile_piece(
    vertices,
    indices,
    joints,
    weights,
    center,
    size,
    joint,
    detail,
    shape,
    head_profile=HEAD_PROFILE,
):
    # Preserve round cheeks, hands, and garment fit in editor/portrait views.
    # Far remains intentionally compact for small on-screen characters.
    radial, rows = {3: (48, 24), 2: (24, 12), 1: (8, 4)}[detail]
    start = len(vertices)
    for row in range(rows + 1):
        t = row / rows
        for column in range(radial + 1):
            angle = column / radial * math.tau
            point = surface(shape, t, angle, head_profile)
            vertices.append(tuple(center[axis] + point[axis] * size[axis] for axis in range(3)))
            joints.append([joint, 0, 0, 0])
            weights.append([1.0, 0.0, 0.0, 0.0])
            if row < rows and column < radial:
                a = start + row * (radial + 1) + column
                b = a + radial + 1
                indices.extend([a, b, a + 1, a + 1, b, b + 1])
    for row, top in [(0, False), (rows, True)]:
        ring = start + row * (radial + 1)
        cap = len(vertices)
        points = vertices[ring:ring + radial]
        vertices.append(tuple(sum(point[axis] for point in points) / radial for axis in range(3)))
        joints.append([joint, 0, 0, 0])
        weights.append([1.0, 0.0, 0.0, 0.0])
        for column in range(radial):
            a = ring + column
            indices.extend([cap, a + 1, a] if top else [cap, a, a + 1])


def cube(vertices, indices, joints, weights, center, size, joint, detail):
    cx, cy, cz = center
    sx, sy, sz = (value * 0.5 for value in size)
    corners = [
        (-sx, -sy, -sz), (sx, -sy, -sz), (sx, sy, -sz), (-sx, sy, -sz),
        (-sx, -sy, sz), (sx, -sy, sz), (sx, sy, sz), (-sx, sy, sz),
    ]
    start = len(vertices)
    vertices.extend([(cx + x, cy + y, cz + z) for x, y, z in corners])
    joints.extend([[joint, 0, 0, 0]] * 8)
    weights.extend([[1.0, 0.0, 0.0, 0.0]] * 8)
    faces = [
        (0, 1, 2), (0, 2, 3), (4, 6, 5), (4, 7, 6),
        (0, 4, 5), (0, 5, 1), (3, 2, 6), (3, 6, 7),
        (0, 3, 7), (0, 7, 4), (1, 5, 6), (1, 6, 2),
    ]
    for a, b, c in faces:
        indices.extend([start + a, start + b, start + c])


def lod_geometry(detail, head_profile=HEAD_PROFILE):
    vertices, indices, joints, weights = [], [], [], []
    # Covered regions are a fitted underlayer; exposed regions match the
    # established Person profile so the analytic face remains correctly
    # seated on the authored head.
    for joint, center, size, shape in [
        (1, (0, 0, 0), (0.82, 0.98, 0.58), "torso"),
        (1, (0, 0.56, 0), (0.29, 0.34, 0.30), "limb"),
        (2, (0, 0, 0), (1.02, 0.94, 0.85), "head"),
        (3, (0, -0.27, 0), (0.28, 0.66, 0.32), "limb"),
        (4, (0, -0.24, 0), (0.25, 0.55, 0.30), "limb"),
        (5, (0, -0.055, -0.025), (0.26, 0.32, 0.23), "pebble"),
        (5, (0.125, 0.025, -0.04), (0.13, 0.20, 0.14), "pebble"),
        (6, (0, -0.27, 0), (0.28, 0.66, 0.32), "limb"),
        (7, (0, -0.24, 0), (0.25, 0.55, 0.30), "limb"),
        (8, (0, -0.055, -0.025), (0.26, 0.32, 0.23), "pebble"),
        (8, (-0.125, 0.025, -0.04), (0.13, 0.20, 0.14), "pebble"),
        (9, (0, -0.12, 0), (0.36, 0.56, 0.38), "limb"),
        (10, (0, -0.25, 0), (0.28, 0.76, 0.30), "limb"),
        (11, (0, 0.13, -0.08), (0.44, 0.24, 0.68), "shoe"),
        (12, (0, -0.12, 0), (0.36, 0.56, 0.38), "limb"),
        (13, (0, -0.25, 0), (0.28, 0.76, 0.30), "limb"),
        (14, (0, 0.13, -0.08), (0.44, 0.24, 0.68), "shoe"),
    ]:
        profile_piece(vertices, indices, joints, weights, center, size, joint, detail, shape, head_profile)
    return vertices, indices, joints, weights


def accessor(buffer_view, component_type, count, kind, minimum=None, maximum=None):
    value = {"bufferView": buffer_view, "componentType": component_type, "count": count, "type": kind}
    if minimum is not None:
        value["min"] = minimum
    if maximum is not None:
        value["max"] = maximum
    return value


def weld_surface(surface):
    """Share round seam vertices without merging different skin weights/UVs.

    Normals are authored by this generator and preserved through compilation.
    """
    vertices, indices, joints, weights = surface[:4]
    uvs = surface[4] if len(surface) == 5 else None
    result = ([], [], [], [], []) if uvs is not None else ([], [], [], [])
    lookup, remap = {}, []
    for index, vertex in enumerate(vertices):
        key = (tuple(round(v, 7) for v in vertex), tuple(joints[index]), tuple(weights[index]),
               tuple(uvs[index]) if uvs is not None else None)
        if key not in lookup:
            lookup[key] = len(result[0])
            result[0].append(vertex)
            result[2].append(joints[index])
            result[3].append(weights[index])
            if uvs is not None: result[4].append(uvs[index])
        remap.append(lookup[key])
    for offset in range(0, len(indices), 3):
        face = [remap[i] for i in indices[offset:offset+3]]
        if len(set(face)) == 3: result[1].extend(face)
    return result


def surface_normals(positions, indices, joints, weights):
    """Author smooth shading across UV copies of these rounded source surfaces.

    This is a source modelling choice, not a runtime rule. Keep distinct skin
    bindings separate; hand-authored GLB hard edges remain untouched by import.
    """
    keys=[(tuple(round(v,7) for v in p),tuple(j),tuple(w)) for p,j,w in zip(positions,joints,weights)]
    sums={key:[0.,0.,0.] for key in keys}
    for at in range(0,len(indices),3):
        a,b,c=indices[at:at+3]
        u=[positions[b][i]-positions[a][i] for i in range(3)]
        v=[positions[c][i]-positions[a][i] for i in range(3)]
        n=(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0])
        for index in (a,b,c):
            for axis in range(3): sums[keys[index]][axis]+=n[axis]
    def normalized(n):
        length=math.sqrt(sum(v*v for v in n))
        return tuple(v/length for v in n) if length>1e-12 else (0.,1.,0.)
    return [normalized(sums[key]) for key in keys]


def make_glb(output, variant):
    binary = bytearray()
    views, accessors, meshes = [], [], []

    def add_blob(blob, target=None):
        while len(binary) % 4:
            binary.append(0)
        offset = len(binary)
        binary.extend(blob)
        view = {"buffer": 0, "byteOffset": offset, "byteLength": len(blob)}
        if target is not None:
            view["target"] = target
        views.append(view)
        return len(views) - 1

    for level, detail in [("Near", 3), ("Mid", 2), ("Far", 1)]:
        positions, indices, joints, weights = weld_surface(lod_geometry(detail, variant["head_profile"]))
        pos_blob = b"".join(struct.pack("<3f", *v) for v in positions)
        idx_blob = b"".join(struct.pack("<I", v) for v in indices)
        joint_blob = b"".join(struct.pack("<4H", *v) for v in joints)
        weight_blob = b"".join(struct.pack("<4f", *v) for v in weights)
        pos_view = add_blob(pos_blob, 34962)
        idx_view = add_blob(idx_blob, 34963)
        joint_view = add_blob(joint_blob, 34962)
        weight_view = add_blob(weight_blob, 34962)
        pos_accessor = len(accessors)
        accessors.append(accessor(pos_view, 5126, len(positions), "VEC3"))
        idx_accessor = len(accessors)
        accessors.append(accessor(idx_view, 5125, len(indices), "SCALAR"))
        joint_accessor = len(accessors)
        accessors.append(accessor(joint_view, 5123, len(joints), "VEC4"))
        weight_accessor = len(accessors)
        accessors.append(accessor(weight_view, 5126, len(weights), "VEC4"))
        normals=surface_normals(positions,indices,joints,weights)
        normal_view=add_blob(b"".join(struct.pack("<3f",*n) for n in normals),34962)
        normal_accessor=len(accessors)
        accessors.append(accessor(normal_view,5126,len(normals),"VEC3"))
        uv_values=[((position[0]*1.25+position[2]*.55)%1.0,
                    (position[1]*.9+position[2]*.25)%1.0) for position in positions]
        uv_view=add_blob(b"".join(struct.pack("<2f",*uv) for uv in uv_values),34962)
        uv_accessor=len(accessors)
        accessors.append(accessor(uv_view,5126,len(uv_values),"VEC2"))
        meshes.append({
            "name": f"Person_{level}",
            "primitives": [{
                "attributes": {"POSITION": pos_accessor, "NORMAL": normal_accessor, "TEXCOORD_0": uv_accessor, "JOINTS_0": joint_accessor, "WEIGHTS_0": weight_accessor},
                "indices": idx_accessor,
                "material": 0,
                "mode": 4,
            }],
        })

    nodes = [{"name": name, **({"parent": parent} if parent is not None else {})} for name, parent, _ in JOINTS]
    # glTF expresses hierarchy on parents, not with a parent field.
    for index, (name, parent, translation) in enumerate(JOINTS):
        nodes[index] = {"name": name, "translation": list(translation)}
        if parent is not None:
            nodes[parent].setdefault("children", []).append(index)
    lod_node_indices = []
    for index, level in enumerate(["Near", "Mid", "Far"]):
        lod_node_indices.append(len(nodes))
        nodes.append({"name": f"Person_{level}", "mesh": index, "skin": 0})
    scene_nodes = [0]
    # Identity inverse bind matrices keep the fixture easy to inspect; runtime
    # importers still receive complete JOINTS_0/WEIGHTS_0 data.
    inverse_bind = b"".join(struct.pack("<16f", *([1.0 if i % 5 == 0 else 0.0 for i in range(16)])) for _ in JOINTS)
    inverse_view = add_blob(inverse_bind)
    inverse_accessor = len(accessors)
    accessors.append(accessor(inverse_view, 5126, len(JOINTS), "MAT4"))
    nodes[0].setdefault("children", []).extend(lod_node_indices)
    texture_names=ensure_material_textures()
    image_view=add_blob((STARTER_ROOT/"assets"/texture_names["skin"]).read_bytes())
    document = {
        "asset": {"version": "2.0", "generator": "Cubacadabra starter base artwork"},
        "scene": 0,
        "scenes": [{"nodes": scene_nodes}],
        "nodes": nodes,
        "meshes": meshes,
        "skins": [{"name": "Person_Skin", "joints": list(range(len(JOINTS))), "inverseBindMatrices": inverse_accessor, "skeleton": 0}],
        "materials": [{"name": "Person_Skin", "extras": {"cubaUseAvatarTint": variant.get("avatar_tint", False)}, "pbrMetallicRoughness": {"baseColorFactor": variant["skin_color"], "baseColorTexture": {"index": 0}, "roughnessFactor": 0.82, "metallicFactor": 0.0}}],
        "images": [{"name": texture_names["skin"], "bufferView": image_view, "mimeType": "image/png"}],
        "textures": [{"name": texture_names["skin"], "source": 0}],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(binary)}],
    }
    json_blob = json.dumps(document, separators=(",", ":")).encode("utf-8")
    json_blob += b" " * ((4 - len(json_blob) % 4) % 4)
    binary += b"\0" * ((4 - len(binary) % 4) % 4)
    total = 12 + 8 + len(json_blob) + 8 + len(binary)
    glb = struct.pack("<III", 0x46546C67, 2, total)
    glb += struct.pack("<II", len(json_blob), 0x4E4F534A) + json_blob
    glb += struct.pack("<II", len(binary), 0x004E4942) + binary
    output.write_bytes(glb)


def main():
    args = [argument for argument in sys.argv[1:] if not argument.startswith("--variant=")]
    variant_name = next(
        (argument.split("=", 1)[1] for argument in sys.argv[1:] if argument.startswith("--variant=")),
        "person-01",
    )
    if variant_name not in PERSON_VARIANTS:
        raise SystemExit(f"unknown person variant: {variant_name}")
    variant = PERSON_VARIANTS[variant_name]
    target = Path(args[0] if args else "/Users/aa/Downloads/person_skinned.glb")
    target.parent.mkdir(parents=True, exist_ok=True)
    make_glb(target, variant)
    lod_counts = {
        level: len(weld_surface(lod_geometry(detail, variant["head_profile"]))[1]) // 3
        for level, detail in (("near", 3), ("mid", 2), ("far", 1))
    }
    sidecar = target.with_suffix(".morph.json")
    sidecar.write_text(json.dumps({
        "schemaVersion": 2,
        "asset": {
            "id": variant["id"],
            "kind": "base",
            "displayName": variant["display_name"],
            "rigProfile": "cuba:rig/biped15.v1",
            "fitProfiles": ["cuba:fit/person-standard.v1"],
            "occupiedSlots": ["base"],
            "coverage": ["body"],
            "conflicts": [],
            "materials": ["skin", "face"],
            "lod": lod_counts,
            "requiredCapabilities": ["skin.biped15-linear.v1", "material.cuba-pbr.v1", "material.base-color-texture.v1"],
            "source": {"geometry": target.name},
            "provenance": {"source": f"Cubacadabra starter-set presentation skinned {variant_name} base", "license": "Cubacadabra official"},
        },
        "geometry": {"file": target.name, "lodNodes": {"near": "Person_Near", "mid": "Person_Mid", "far": "Person_Far"}},
        "attachment": {"mode": "skinned", "joint": "root", "translation": [0, 0, 0], "rotation": [0, 0, 0, 1], "scale": [1, 1, 1]},
        "skin": {"skeleton": "Person_Skin", "jointOrder": [name for name, _, _ in JOINTS], "maxInfluences": 4},
    }, indent=2) + "\n")
    print(target)
    print(sidecar)


if __name__ == "__main__":
    main()
