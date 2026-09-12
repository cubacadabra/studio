#!/usr/bin/env python3
"""Generate Blender-compatible skinned clothing assets.

Each asset uses the canonical 15-joint hierarchy and four-weight attributes,
with presentation-detail Near meshes and intentionally economical Far meshes.
Studio compiles them through the same schema 5 path as a Blender export.
"""

import json
import math
import struct
import sys
from pathlib import Path

import generate_person_asset as person

STARTER_ROOT = Path(__file__).resolve().parents[2] / "tools" / "starter-set"
sys.path.insert(0, str(STARTER_ROOT))
from artwork.material_textures import ensure as ensure_material_textures


def material(material_id, name, color, avatar_tint=False, roughness=0.82, texture=None):
    textures=ensure_material_textures()
    lower=name.lower()
    if texture is None:
        if any(token in lower for token in ("denim", "jeans")):
            texture=textures["denim"]
        elif any(token in lower for token in ("hoodie", "fleece", "rib", "polo", "shirt", "collar", "trim", "shadow")):
            texture=textures["fleece"] if "rib" not in lower else textures["rib"]
        elif any(token in lower for token in ("leather", "upper", "lace", "sneaker")):
            texture=textures["leather"]
        elif any(token in lower for token in ("sole", "rubber")):
            texture=textures["rubber"]
        elif any(token in lower for token in ("satin", "sequin")):
            texture=textures["satin"]
        elif any(token in lower for token in ("slack", "pressed")):
            texture=textures["fleece"]
        else:
            texture=textures["cotton"]
    return {
        "id": material_id,
        "name": name,
        "color": color,
        "avatar_tint": avatar_tint,
        "roughness": roughness,
        "texture": texture,
    }


ASSETS = {
    "top": {
        "id": "cuba:top/person-top.v1",
        "kind": "top",
        "name": "Hoodie",
        "style": "hoodie",
        "slots": ["shirt"],
        "coverage": ["torso", "arms"],
        "materials": [
            material("top", "Hoodie", [0.10, 0.52, 0.46, 1.0], True),
            material("drawstring", "White Drawstring", [0.97, 0.97, 0.95, 1.0], roughness=0.68),
            material("pocket-opening", "Pocket Opening", [0.055, 0.25, 0.23, 1.0], roughness=0.76),
        ],
        "pieces": [
            (1, (0.0, 0.0, 0.0), (1.10, 1.04, 0.73), "torso"),
            (3, (0.0, -0.40, 0.0), (0.46, 1.13, 0.49), "sleeve"),
            (6, (0.0, -0.40, 0.0), (0.46, 1.13, 0.49), "sleeve"),
        ],
    },
    "bottom": {
        "id": "cuba:bottom/person-bottom.v1",
        "kind": "bottom",
        "name": "Jeans",
        "style": "jeans",
        "slots": ["pants"],
        "coverage": ["legs"],
        "materials": [
            material("bottom", "Jeans", [0.20, 0.28, 0.66, 1.0], True),
        ],
        "pieces": [
            (9, (0.0, -0.15, 0.0), (0.46, 0.66, 0.47), "shorts"),
            (12, (0.0, -0.15, 0.0), (0.46, 0.66, 0.47), "shorts"),
        ],
    },
    "shoes": {
        "id": "cuba:footwear/person-shoes.v1",
        "kind": "footwear",
        "name": "Sneakers",
        "style": "sneakers",
        "slots": ["shoes"],
        "coverage": ["feet"],
        "materials": [
            material("footwear", "Navy Upper", [0.08, 0.11, 0.16, 1.0], roughness=0.72),
            material("sole", "Ivory Sole", [0.96, 0.93, 0.84, 1.0], roughness=0.76),
            material("laces", "Ivory Laces", [0.98, 0.97, 0.94, 1.0], roughness=0.66),
        ],
        "pieces": [
            (11, (0.0, 0.155, -0.09), (0.50, 0.29, 0.76), "shoe"),
            (14, (0.0, 0.155, -0.09), (0.50, 0.29, 0.76), "shoe"),
        ],
    },
    "short_sleeve_collared": {
        "id": "cuba:top/short-sleeve-collared.v1",
        "kind": "top",
        "name": "Short Sleeve Collared",
        "style": "short-sleeve-collared",
        "slots": ["shirt"],
        "coverage": ["torso", "upper-arms"],
        "materials": [
            material("shirt", "Polo Cotton", [0.08, 0.29, 0.58, 1.0], True, roughness=0.78),
            material("collar-placket", "Folded Collar", [0.095, 0.31, 0.57, 1.0], True, roughness=0.72),
            material("sleeve-cuffs", "Navy Polo Trim", [0.025, 0.09, 0.23, 1.0], roughness=0.70),
            material("buttons", "Slate Buttons", [0.27, 0.35, 0.47, 1.0], roughness=0.42),
            material("logo", "Cubacadabra Logo", [1.0, 1.0, 1.0, 1.0], roughness=0.72, texture="logo.png"),
            material("collar-underside", "Collar Fold Shadow", [0.025, 0.085, 0.18, 1.0], roughness=0.88),
        ],
        "pieces": [
            (1, (0.0, 0.0, 0.0), (1.05, 0.98, 0.69), "torso"),
            (3, (0.0, -0.07, 0.0), (0.40, 0.40, 0.44), "sleeve"),
            (6, (0.0, -0.07, 0.0), (0.40, 0.40, 0.44), "sleeve"),
        ],
    },
    "slacks": {
        "id": "cuba:bottom/slacks.v1",
        "kind": "bottom",
        "name": "Slacks",
        "style": "slacks",
        "slots": ["pants"],
        "coverage": ["legs"],
        "materials": [
            material("slacks", "Charcoal Slacks", [0.13, 0.155, 0.20, 1.0], roughness=0.86),
            material("seams", "Pressed Seams", [0.16, 0.18, 0.23, 1.0], roughness=0.86),
        ],
        "pieces": [
            (9, (0.0, -0.15, 0.0), (0.48, 0.72, 0.49), "shorts"),
            (10, (0.0, -0.22, 0.0), (0.35, 0.78, 0.37), "limb"),
            (12, (0.0, -0.15, 0.0), (0.48, 0.72, 0.49), "shorts"),
            (13, (0.0, -0.22, 0.0), (0.35, 0.78, 0.37), "limb"),
        ],
    },
    "sparkles": {
        "id": "cuba:footwear/sparkles.v1",
        "kind": "footwear",
        "name": "Sparkles",
        "style": "sparkles",
        "slots": ["shoes"],
        "coverage": ["feet"],
        "materials": [
            material("sparkle-satin", "Amethyst Satin", [0.43, 0.29, 0.60, 1.0], roughness=0.38),
            material("gold-sole", "Pearl Sole", [0.91, 0.86, 0.74, 1.0], roughness=0.72),
            material("gold-laces", "Champagne Laces", [0.81, 0.69, 0.45, 1.0], roughness=0.56),
            material("sequins", "Iridescent Sequins", [0.85, 0.66, 0.89, 1.0], roughness=0.22),
        ],
        "pieces": [
            (11, (0.0, 0.155, -0.09), (0.50, 0.29, 0.76), "shoe"),
            (14, (0.0, 0.155, -0.09), (0.50, 0.29, 0.76), "shoe"),
        ],
    },
}


def hood_ring(vertices, indices, joints, weights, detail):
    """Build the soft oval collar that gives a lowered hood its opening."""
    radial, rows = {3: (48, 18), 2: (28, 11), 1: (20, 9)}[detail]
    start = len(vertices)
    center = (0.0, 0.45, 0.12)
    size = (0.73, 0.21, 0.72)
    for row in range(rows + 1):
        t = row / rows
        v, u = math.sin(t * math.tau), math.cos(t * math.tau)
        for column in range(radial + 1):
            angle = column / radial * math.tau
            point = (
                (0.33 + 0.15 * u) * math.cos(angle),
                0.22 * v + 0.26 * math.sin(angle),
                (0.32 + 0.15 * u) * math.sin(angle),
            )
            vertices.append(tuple(center[axis] + point[axis] * size[axis] for axis in range(3)))
            joints.append([1, 0, 0, 0])
            weights.append([1.0, 0.0, 0.0, 0.0])
            if row < rows and column < radial:
                a = start + row * (radial + 1) + column
                b = a + radial + 1
                indices.extend([a, b, a + 1, a + 1, b, b + 1])


def folded_hood(vertices, indices, joints, weights, detail):
    """Add the lowered hood profile used by the established Person art."""
    hood_ring(vertices, indices, joints, weights, detail)
    person.profile_piece(
        vertices,
        indices,
        joints,
        weights,
        (0.0, 0.30, 0.30),
        (0.69, 0.60, 0.38),
        1,
        detail,
        "pebble",
    )


def hoodie_pocket(vertices, indices, joints, weights, detail):
    """Add the established tapered kangaroo pocket profile."""
    person.profile_piece(
        vertices,
        indices,
        joints,
        weights,
        (0.0, -0.21, -0.33),
        (0.66, 0.32, 0.11),
        1,
        detail,
        "pocket",
    )


def normalize(vector):
    length = math.sqrt(sum(value * value for value in vector))
    return tuple(value / length for value in vector)


def cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def curved_cord(vertices, indices, joints, weights, side, detail):
    """Sweep a slim capped tube from the neckline to a short aglet."""
    radial = {3: 14, 2: 8, 1: 6}[detail]
    points = [
        (side * 0.105, 0.505, -0.190),
        (side * 0.125, 0.435, -0.235),
        (side * 0.145, 0.315, -0.285),
        (side * 0.145, 0.175, -0.325),
        (side * 0.135, 0.055, -0.350),
        (side * 0.135, 0.010, -0.350),
    ]
    start = len(vertices)
    for row, point in enumerate(points):
        previous = points[max(0, row - 1)]
        following = points[min(len(points) - 1, row + 1)]
        tangent = normalize(tuple(following[axis] - previous[axis] for axis in range(3)))
        basis_x = normalize(cross(tangent, (0.0, 0.0, 1.0)))
        basis_z = cross(tangent, basis_x)
        radius = 0.017 if row == 0 else (0.011 if row < len(points) - 2 else 0.014)
        for column in range(radial + 1):
            angle = column / radial * math.tau
            vertices.append(tuple(
                point[axis]
                + basis_x[axis] * math.cos(angle) * radius
                + basis_z[axis] * math.sin(angle) * radius
                for axis in range(3)
            ))
            joints.append([1, 0, 0, 0])
            weights.append([1.0, 0.0, 0.0, 0.0])
            if row < len(points) - 1 and column < radial:
                a = start + row * (radial + 1) + column
                b = a + radial + 1
                indices.extend([a, b, a + 1, a + 1, b, b + 1])
    for row, top in [(0, False), (len(points) - 1, True)]:
        ring = start + row * (radial + 1)
        cap = len(vertices)
        vertices.append(points[row])
        joints.append([1, 0, 0, 0])
        weights.append([1.0, 0.0, 0.0, 0.0])
        for column in range(radial):
            a = ring + column
            indices.extend([cap, a + 1, a] if top else [cap, a, a + 1])


def detail_tube(vertices, indices, joints, weights, points, radius, detail, joint=1):
    """Sweep a small capped tube for modeled pocket openings and stitches."""
    radial = {3: 14, 2: 8, 1: 5}[detail]
    start = len(vertices)
    for row, point in enumerate(points):
        previous = points[max(0, row - 1)]
        following = points[min(len(points) - 1, row + 1)]
        tangent = normalize(tuple(following[axis] - previous[axis] for axis in range(3)))
        across = normalize(cross(tangent, (0.0, 0.0, 1.0)))
        forward = cross(tangent, across)
        for column in range(radial + 1):
            angle = column / radial * math.tau
            vertices.append(tuple(
                point[axis]
                + across[axis] * math.cos(angle) * radius
                + forward[axis] * math.sin(angle) * radius
                for axis in range(3)
            ))
            joints.append([joint, 0, 0, 0])
            weights.append([1.0, 0.0, 0.0, 0.0])
            if row < len(points) - 1 and column < radial:
                a = start + row * (radial + 1) + column
                b = a + radial + 1
                indices.extend([a, b, a + 1, a + 1, b, b + 1])
    for row, top in [(0, False), (len(points) - 1, True)]:
        ring = start + row * (radial + 1)
        cap = len(vertices)
        vertices.append(points[row])
        joints.append([joint, 0, 0, 0])
        weights.append([1.0, 0.0, 0.0, 0.0])
        for column in range(radial):
            a = ring + column
            indices.extend([cap, a + 1, a] if top else [cap, a, a + 1])


def pocket_details(white, openings, detail):
    front = -0.389
    for side in (-1.0, 1.0):
        detail_tube(
            *openings,
            [
                (side * 0.17, -0.105, front),
                (side * 0.235, -0.175, front - 0.002),
                (side * 0.295, -0.265, front),
            ],
            0.012,
            detail,
        )


def garment_band(surface, profile, detail, joint):
    """Add a softly rolled cloth band while sharing the garment material."""
    radial = {3: 48, 2: 24, 1: 12}[detail]
    append_grid(surface, oval_rings(profile, radial), joint)


def hoodie_finish(cloth, openings, detail):
    """Model cuffs, waistband, hood seam, and pocket edge for close views."""
    for joint in (3, 6):
        garment_band(
            cloth,
            [(-0.985, 0.145, 0.155), (-0.970, 0.160, 0.170),
             (-0.915, 0.170, 0.180), (-0.895, 0.155, 0.164)],
            detail,
            joint,
        )
    garment_band(
        cloth,
        [(-0.550, 0.390, 0.260), (-0.535, 0.420, 0.290),
         (-0.485, 0.430, 0.300), (-0.470, 0.400, 0.270)],
        detail,
        1,
    )
    # Subtle modeled seams survive the untextured shared renderer and provide
    # the small contact shadows that make the pocket and hood read as fabric.
    for side in (-1.0, 1.0):
        detail_tube(
            *openings,
            [(side * 0.300, -0.275, -0.385),
             (side * 0.245, -0.345, -0.386),
             (side * 0.165, -0.405, -0.384)],
            0.006,
            detail,
        )
    detail_tube(
        *openings,
        [(-0.285, 0.285, -0.205), (0.0, 0.245, -0.230), (0.285, 0.285, -0.205)],
        0.005,
        detail,
    )


def jeans_finish(cloth, detail):
    """Give the shorts a waistband, hems, fly, and front-pocket construction."""
    for joint, side in ((9, -1.0), (12, 1.0)):
        garment_band(
            cloth,
            [(0.115, 0.218, 0.224), (0.135, 0.235, 0.240),
             (0.175, 0.238, 0.243), (0.192, 0.218, 0.225)],
            detail,
            joint,
        )
        garment_band(
            cloth,
            [(-0.495, 0.205, 0.212), (-0.478, 0.226, 0.233),
             (-0.435, 0.228, 0.235), (-0.418, 0.207, 0.214)],
            detail,
            joint,
        )
        detail_tube(
            *cloth,
            [(side * 0.035, 0.105, -0.225),
             (side * 0.105, 0.045, -0.232),
             (side * 0.165, -0.045, -0.226)],
            0.005,
            detail,
            joint,
        )
        detail_tube(
            *cloth,
            [(side * -0.205, 0.095, -0.100), (side * -0.205, -0.425, -0.105)],
            0.0035,
            detail,
            joint,
        )
    detail_tube(
        *cloth,
        [(0.0, 1.000, -0.242), (0.0, 0.925, -0.247), (0.0, 0.850, -0.235)],
        0.004,
        detail,
        0,
    )


def shoe_detail_surfaces(detail):
    """Recreate the established Person shoe sole and three raised lace bands."""
    soles = ([], [], [], [])
    laces = ([], [], [], [])
    # The foot joint rests 0.05 units above the floor. The sole profile is
    # centered so its lowest normalized point (-0.5) lands exactly on it.
    sole_center_y = -0.05 + 0.5 * 0.095
    for joint in (11, 14):
        person.profile_piece(
            *soles,
            (0.0, sole_center_y, -0.09),
            (0.51, 0.095, 0.77),
            joint,
            detail,
            "shoe",
        )
        for offset in (-0.32, 0.0, 0.32):
            person.profile_piece(
                *laces,
                (0.0, 0.26 + offset * 0.7 * 0.065, -0.24 + offset * 0.24),
                (0.31, 0.065 * 0.28, 0.24 * 0.15),
                joint,
                max(1, detail - 1),
                "pebble",
            )
    return soles, laces


def add_front_disc(surface, center, radius, detail, joint=1):
    vertices, indices, joints, weights = surface
    segments = {3: 16, 2: 10, 1: 6}[detail]
    start = len(vertices)
    vertices.append(center)
    joints.append([joint, 0, 0, 0])
    weights.append([1.0, 0.0, 0.0, 0.0])
    for index in range(segments + 1):
        angle = index / segments * math.tau
        vertices.append((
            center[0] + math.cos(angle) * radius,
            center[1] + math.sin(angle) * radius,
            center[2],
        ))
        joints.append([joint, 0, 0, 0])
        weights.append([1.0, 0.0, 0.0, 0.0])
        if index < segments:
            indices.extend([start, start + index + 2, start + index + 1])


def collared_surfaces(asset, detail):
    shirt = ([], [], [], [])
    radial = {3: 48, 2: 28, 1: 16}[detail]
    # An open neckline, not a capped torso with collar triangles pasted on it.
    torso_profile = [
        (-0.49, 0.42, 0.27), (-0.46, 0.46, 0.30),
        (-0.30, 0.48, 0.315), (0.0, 0.47, 0.31),
        (0.24, 0.48, 0.275), (0.36, 0.40, 0.225),
        (0.43, 0.27, 0.17), (0.49, 0.145, 0.135),
    ]
    rings = []
    for row, (y, rx, rz) in enumerate(torso_profile):
        ring = []
        for column in range(radial + 1):
            angle = column / radial * math.tau
            # Open V below the throat; taper it into the last two shoulder rows.
            dip = max(0.0, -math.sin(angle)) ** 14 * max(0, row - 5) * 0.075
            ring.append((rx * person.signed_power(math.cos(angle), 0.72),
                         y - dip, rz * person.signed_power(math.sin(angle), 0.72)))
        rings.append(ring)
    append_grid(shirt, rings, 1)
    torso = (list(shirt[0]), list(shirt[1]))

    # Sample the actual triangulated torso at each LOD. Analytic overlays can
    # otherwise float above (or disappear inside) a simplified mesh.
    def chest(x, y, lift=0.006):
        hits = []
        vertices, indices = torso
        for offset in range(0, len(indices), 3):
            a, b, c = [vertices[i] for i in indices[offset:offset + 3]]
            det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
            if abs(det) < 1e-10:
                continue
            u = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / det
            v = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / det
            if min(u, v, 1 - u - v) >= -1e-6:
                hits.append(u * a[2] + v * b[2] + (1 - u - v) * c[2])
        if not hits:
            raise ValueError(f"Polo detail outside torso: {x}, {y}")
        return (x, y, min(hits) - lift)

    def torso_radius(angle, y):
        """Intersect the torso along a horizontal ray for collar clearance."""
        direction = (math.cos(angle), math.sin(angle))
        hits = []
        vertices, indices = torso
        for offset in range(0, len(indices), 3):
            points = [vertices[i] for i in indices[offset:offset + 3]]
            a, b, c = [(p[0] * -direction[1] + p[2] * direction[0], p[1]) for p in points]
            det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
            if abs(det) < 1e-10:
                continue
            u = ((b[1] - c[1]) * -c[0] + (c[0] - b[0]) * (y - c[1])) / det
            v = ((c[1] - a[1]) * -c[0] + (a[0] - c[0]) * (y - c[1])) / det
            if min(u, v, 1 - u - v) >= -1e-6:
                hits.append(sum(weight * (p[0] * direction[0] + p[2] * direction[1])
                                for weight, p in zip((u, v, 1 - u - v), points)))
        return max(hits, default=0.0)

    trim = ([], [], [], [])
    for joint in (3, 6):
        # Sleeve and hem share their rim; neither is a capped floating pebble.
        sleeve_profile = [(-0.36, 0.172, 0.188), (-0.30, 0.178, 0.193),
                          (-0.13, 0.192, 0.211), (0.04, 0.192, 0.208),
                          (0.10, 0.181, 0.198), (0.15, 0.153, 0.170),
                          (0.19, 0.108, 0.122), (0.21, 0.05, 0.058),
                          (0.215, 0.001, 0.001)]
        append_grid(shirt, oval_rings(sleeve_profile, radial), joint)
        cuff_profile = [(-0.30, 0.180, 0.195), (-0.36, 0.175, 0.191),
                        (-0.365, 0.158, 0.174), (-0.30, 0.163, 0.178)]
        cuff_rings = oval_rings(cuff_profile, radial)
        # This profile travels down the outside, around the hem and up inside.
        append_grid(trim, cuff_rings, joint, reverse=True)

    collar = ([], [], [], [])
    # Folded collar with a stand around the neck, a rounded fold and an outer
    # fall. Front tips sweep down onto the chest, leaving an open throat.
    sections = [(0.15, 0.14, 0.475), (0.155, 0.145, 0.515),
                (0.18, 0.166, 0.522), (0.24, 0.21, 0.455),
                (0.29, 0.24, 0.40)]
    collar_rings = []
    for row, (rx, rz, y) in enumerate(sections):
        ring = []
        for column in range(radial + 1):
            angle = -math.pi / 2 + 0.52 + column / radial * (math.tau - 1.04)
            front = max(0., -math.sin(angle)) ** 8
            fall = row / (len(sections) - 1)
            x, height, z = (rx * math.cos(angle), y - front * fall * 0.15,
                            rz * math.sin(angle) - front * fall * 0.085)
            if row >= 2:
                # Both sides of the thick fall must clear the torso. Its
                # underside sits close enough to read as a contact shadow.
                ray = math.atan2(z, x)
                radius = max(math.hypot(x, z), torso_radius(ray, height) + 0.012,
                             torso_radius(ray, height - 0.028) + 0.012)
                x, z = radius * math.cos(ray), radius * math.sin(ray)
            ring.append((x, height, z))
        collar_rings.append(ring)
    # A separate shaded underside makes the folded fabric thickness readable
    # without introducing a shadow pass in the shared/mobile renderer. Keep
    # edge vertices separate so smoothing cannot erase the cloth's rim.
    underside = ([], [], [], [])
    append_grid(collar, collar_rings, 1, reverse=True)
    inner = [[(x, y - 0.028, z + 0.003) for x, y, z in ring] for ring in collar_rings]
    append_grid(underside, inner, 1)
    for edge in (0, -1):
        append_grid(underside, [collar_rings[edge], inner[edge]], 1, reverse=edge == -1)
        append_grid(underside, [[ring[edge] for ring in collar_rings],
                            [ring[edge] for ring in inner]], 1, reverse=edge == 0)

    # Two restrained fastened buttons beneath the open throat. The placket ends
    # at the opening instead of visually buttoning the shirt to the neck.
    placket = [[chest(x, y) for x in (-width, width)]
               for y, width in ((0.18, 0.034), (0.23, 0.034), (0.28, 0.034),
                                (0.32, 0.04), (0.337, 0.046))]
    append_grid(trim, placket, 1)
    buttons = ([], [], [], [])
    add_front_disc(buttons, chest(0.0, 0.23, 0.010), 0.017, detail)
    add_front_disc(buttons, chest(0.0, 0.305, 0.010), 0.015, detail)

    logo = ([], [], [], [], [])
    logo_grid = [[chest(-0.32 + column * 0.03, 0.08 + row * 0.03, 0.008)
                  for column in range(5)] for row in range(5)]
    append_grid(logo[:4], logo_grid, 1)
    logo[4].extend([(column / 4, 1 - row / 4) for row in range(5) for column in range(5)])
    return [shirt, collar, trim, buttons, logo, underside]


def oval_rings(profile, radial):
    return [[(rx * math.cos(column / radial * math.tau), y,
              rz * math.sin(column / radial * math.tau))
             for column in range(radial + 1)] for y, rx, rz in profile]


def append_grid(surface, rings, joint, reverse=False):
    vertices, indices, joints, weights = surface
    start, width = len(vertices), len(rings[0])
    for ring in rings:
        vertices.extend(ring)
        joints.extend([[joint, 0, 0, 0]] * width)
        weights.extend([[1.0, 0.0, 0.0, 0.0]] * width)
    for row in range(len(rings) - 1):
        for column in range(width - 1):
            a = start + row * width + column
            b = a + width
            faces = [[a, b, a + 1], [a + 1, b, b + 1]]
            for face in faces:
                indices.extend(reversed(face) if reverse else face)


def slacks_surfaces(asset, detail):
    fabric = ([], [], [], [])
    for joint, center, size, shape in asset["pieces"]:
        person.profile_piece(*fabric, center, size, joint, detail, shape)
    seams = ([], [], [], [])
    for joint in (9, 12):
        detail_tube(*seams, [(0.0, 0.10, -0.225), (0.0, -0.48, -0.225)], 0.004, detail, joint)
    for joint in (10, 13):
        detail_tube(*seams, [(0.0, 0.13, -0.180), (0.0, -0.54, -0.180)], 0.003, detail, joint)
    return [fabric, seams]


def sparkle_surfaces(asset, detail):
    uppers = ([], [], [], [])
    for joint, center, size, shape in asset["pieces"]:
        person.profile_piece(*uppers, center, size, joint, detail, shape)
    soles, laces = shoe_detail_surfaces(detail)
    sequins = ([], [], [], [])
    positions = [
        (-0.15, 0.245, -0.31), (0.0, 0.275, -0.34), (0.15, 0.245, -0.31),
        (-0.21, 0.205, -0.23), (0.0, 0.245, -0.25), (0.21, 0.205, -0.23),
        (-0.14, 0.165, -0.14), (0.14, 0.165, -0.14), (0.0, 0.205, -0.10),
    ]
    keep = {3: 9, 2: 5, 1: 2}[detail]
    for joint in (11, 14):
        for center in positions[:keep]:
            person.profile_piece(*sequins, center, (0.052, 0.028, 0.032), joint, 1, "pebble")
    return [uppers, soles, laces, sequins]


def clothing_lod_surfaces(asset, detail):
    pieces = asset["pieces"]
    cloth = ([], [], [], [])
    vertices, indices, joints, weights = cloth
    for joint, center, size, shape in pieces:
        person.profile_piece(vertices, indices, joints, weights, center, size, joint, detail, shape)
    if asset["style"] == "hoodie":
        folded_hood(vertices, indices, joints, weights, detail)
        hoodie_pocket(vertices, indices, joints, weights, detail)
        cords = ([], [], [], [])
        for side in (-1.0, 1.0):
            curved_cord(*cords, side, detail)
        openings = ([], [], [], [])
        pocket_details(cords, openings, detail)
        hoodie_finish(cloth, openings, detail)
        return [cloth, cords, openings]
    if asset["style"] == "short-sleeve-collared":
        return collared_surfaces(asset, detail)
    if asset["style"] == "slacks":
        return slacks_surfaces(asset, detail)
    if asset["style"] == "sparkles":
        return sparkle_surfaces(asset, detail)
    if asset["style"] == "sneakers":
        return [cloth, *shoe_detail_surfaces(detail)]
    if asset["style"] == "jeans":
        jeans_finish(cloth, detail)
    return [cloth]


def make_glb(output, asset):
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

    triangle_counts = {}
    for level, detail in [("Near", 3), ("Mid", 2), ("Far", 1)]:
        surfaces = [person.weld_surface(surface) for surface in clothing_lod_surfaces(asset, detail)]
        if len(surfaces) != len(asset["materials"]):
            raise ValueError(f"surface/material mismatch for {asset['id']}")
        triangle_counts[level.lower()] = sum(len(surface[1]) // 3 for surface in surfaces)
        primitives = []
        for material_index, surface in enumerate(surfaces):
            positions, indices, joints, weights = surface[:4]
            uvs = surface[4] if len(surface) == 5 else None
            blobs = [
                b"".join(struct.pack("<3f", *value) for value in positions),
                b"".join(struct.pack("<I", value) for value in indices),
                b"".join(struct.pack("<4H", *value) for value in joints),
                b"".join(struct.pack("<4f", *value) for value in weights),
            ]
            pos_view, idx_view, joint_view, weight_view = [
                add_blob(blob, 34962 if index != 1 else 34963)
                for index, blob in enumerate(blobs)
            ]
            pos_accessor = len(accessors)
            accessors.append({"bufferView": pos_view, "componentType": 5126, "count": len(positions), "type": "VEC3"})
            idx_accessor = len(accessors)
            accessors.append({"bufferView": idx_view, "componentType": 5125, "count": len(indices), "type": "SCALAR"})
            joint_accessor = len(accessors)
            accessors.append({"bufferView": joint_view, "componentType": 5123, "count": len(joints), "type": "VEC4"})
            weight_accessor = len(accessors)
            accessors.append({"bufferView": weight_view, "componentType": 5126, "count": len(weights), "type": "VEC4"})
            attributes = {"POSITION": pos_accessor, "JOINTS_0": joint_accessor, "WEIGHTS_0": weight_accessor}
            normals=person.surface_normals(positions,indices,joints,weights)
            normal_view=add_blob(b"".join(struct.pack("<3f",*n) for n in normals),34962)
            attributes["NORMAL"]=len(accessors)
            accessors.append({"bufferView":normal_view,"componentType":5126,"count":len(normals),"type":"VEC3"})
            if uvs is None:
                uvs=[((position[0]*1.55+position[2]*.6)%1.0,
                      (position[1]*1.25+position[2]*.3)%1.0) for position in positions]
            if uvs is not None:
                uv_blob = b"".join(struct.pack("<2f", *value) for value in uvs)
                uv_view = add_blob(uv_blob, 34962)
                uv_accessor = len(accessors)
                accessors.append({"bufferView": uv_view, "componentType": 5126, "count": len(uvs), "type": "VEC2"})
                attributes["TEXCOORD_0"] = uv_accessor
            primitives.append({
                "attributes": attributes,
                "indices": idx_accessor,
                "material": material_index,
                "mode": 4,
            })
        meshes.append({
            "name": f"Person{asset['kind'].title()}_{level}",
            "primitives": primitives,
        })

    nodes = []
    for name, parent_index, translation in person.JOINTS:
        node = {"name": name, "translation": list(translation)}
        if parent_index is not None:
            nodes[parent_index].setdefault("children", [])
        nodes.append(node)
    for index, (_, parent_index, _) in enumerate(person.JOINTS):
        if parent_index is not None:
            nodes[parent_index].setdefault("children", []).append(index)
    lod_node_indices = []
    for index, level in enumerate(["Near", "Mid", "Far"]):
        lod_node_indices.append(len(nodes))
        nodes.append({"name": f"Person{asset['kind'].title()}_{level}", "mesh": index, "skin": 0})
    nodes[0].setdefault("children", []).extend(lod_node_indices)
    inverse_bind = b"".join(
        struct.pack("<16f", *([1.0 if i % 5 == 0 else 0.0 for i in range(16)]))
        for _ in person.JOINTS
    )
    inverse_view = add_blob(inverse_bind)
    inverse_accessor = len(accessors)
    accessors.append({"bufferView": inverse_view, "componentType": 5126, "count": len(person.JOINTS), "type": "MAT4"})
    materials, images, textures = [], [], []
    texture_indices = {}
    asset_root = STARTER_ROOT / "assets"
    for source in asset["materials"]:
        pbr = {
            "baseColorFactor": source["color"],
            "roughnessFactor": source["roughness"],
            "metallicFactor": 0.0,
        }
        if source["texture"] is not None:
            texture_name = source["texture"]
            if texture_name not in texture_indices:
                texture_path = asset_root / texture_name
                if not texture_path.exists():
                    texture_path = Path(__file__).resolve().parent.parent / "assets" / texture_name
                image_view = add_blob(texture_path.read_bytes())
                image_index = len(images)
                images.append({"name": texture_name, "bufferView": image_view, "mimeType": "image/png"})
                texture_indices[texture_name] = len(textures)
                textures.append({"name": texture_name, "source": image_index})
            pbr["baseColorTexture"] = {"index": texture_indices[texture_name], "texCoord": 0}
        materials.append({
            "name": source["name"],
            "extras": {"cubaUseAvatarTint": source["avatar_tint"]},
            "pbrMetallicRoughness": {
                **pbr,
            },
        })
    document = {
        "asset": {"version": "2.0", "generator": "Cubacadabra starter wardrobe artwork"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": nodes,
        "meshes": meshes,
        "skins": [{"name": "Person_Skin", "joints": list(range(len(person.JOINTS))), "inverseBindMatrices": inverse_accessor, "skeleton": 0}],
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(binary)}],
    }
    if images:
        document["images"] = images
        document["textures"] = textures
    json_blob = json.dumps(document, separators=(",", ":")).encode("utf-8")
    json_blob += b" " * ((4 - len(json_blob) % 4) % 4)
    binary += b"\0" * ((4 - len(binary) % 4) % 4)
    total = 12 + 8 + len(json_blob) + 8 + len(binary)
    output.write_bytes(struct.pack("<III", 0x46546C67, 2, total))
    with output.open("ab") as stream:
        stream.write(struct.pack("<II", len(json_blob), 0x4E4F534A))
        stream.write(json_blob)
        stream.write(struct.pack("<II", len(binary), 0x004E4942))
        stream.write(binary)
    return triangle_counts


def write_sidecar(glb_path, asset):
    glb_path.with_suffix(".morph.json").write_text(json.dumps({
        "schemaVersion": 2,
        "asset": {
            "id": asset["id"],
            "kind": asset["kind"],
            "displayName": asset["name"],
            "rigProfile": "cuba:rig/biped15.v1",
            "fitProfiles": ["cuba:fit/person-standard.v1"],
            "supportedBases": ["cuba:base/person.v1"],
            "occupiedSlots": asset["slots"],
            "coverage": asset["coverage"],
            "conflicts": [],
            "materials": [material["id"] for material in asset["materials"]],
            "lod": asset["lod"],
            "requiredCapabilities": ["skin.biped15-linear.v1", "material.cuba-pbr.v1"]
            + (["material.base-color-texture.v1"] if any(material["texture"] for material in asset["materials"]) else []),
            "source": {"geometry": glb_path.name},
            "provenance": {"source": "Cubacadabra starter-set presentation wardrobe", "license": "Cubacadabra official"},
        },
        "geometry": {"file": glb_path.name, "lodNodes": {
            "near": f"Person{asset['kind'].title()}_Near",
            "mid": f"Person{asset['kind'].title()}_Mid",
            "far": f"Person{asset['kind'].title()}_Far",
        }},
        "attachment": {"mode": "skinned", "joint": "root", "translation": [0, 0, 0], "rotation": [0, 0, 0, 1], "scale": [1, 1, 1]},
        "skin": {"skeleton": "Person_Skin", "jointOrder": [name for name, _, _ in person.JOINTS], "maxInfluences": 4},
    }, indent=2) + "\n")


def main():
    output_dir = Path(sys.argv[1] if len(sys.argv) > 1 else "/Users/aa/Downloads")
    output_dir.mkdir(parents=True, exist_ok=True)
    for key, asset in ASSETS.items():
        glb_path = output_dir / f"person_{key}.glb"
        asset["lod"] = make_glb(glb_path, asset)
        write_sidecar(glb_path, asset)
        print(glb_path)


if __name__ == "__main__":
    main()
