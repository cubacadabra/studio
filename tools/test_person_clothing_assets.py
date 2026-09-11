"""Geometry regression checks; run with python3 -m unittest discover -s tools."""
import math
import unittest
from collections import Counter

import generate_person_asset as person
import generate_person_clothing_assets as clothing


class PoloGeometryTests(unittest.TestCase):
    def test_lods_have_valid_surfaces_and_outward_logo(self):
        triangle_counts = []
        for detail in (3, 2, 1):
            surfaces = clothing.collared_surfaces(clothing.ASSETS["short_sleeve_collared"], detail)
            self.assertEqual(len(surfaces), 6)
            triangle_counts.append(sum(len(surface[1]) // 3 for surface in surfaces))
            for surface in surfaces:
                vertices, indices, joints, weights = surface[:4]
                self.assertTrue(vertices)
                self.assertEqual(len(indices) % 3, 0)
                self.assertEqual(len(vertices), len(joints))
                self.assertEqual(len(vertices), len(weights))
                self.assertTrue(all(math.isfinite(value) for v in vertices for value in v))
                self.assertTrue(all(0 <= i < len(vertices) for i in indices))
                self.assertTrue(all(abs(sum(w) - 1) < 1e-6 for w in weights))
            vertices, indices, _, _, uvs = surfaces[4]
            self.assertEqual(len(vertices), len(uvs))
            self.assertTrue(all(0 <= value <= 1 for uv in uvs for value in uv))
            for offset in range(0, len(indices), 3):
                a, b, c = [vertices[i] for i in indices[offset:offset + 3]]
                normal_z = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
                self.assertLess(normal_z, 0, "logo must face out from chest")
            # Hem rings must leave space for skin, not cover the arm opening.
            vertices, _, joints, _ = surfaces[2]
            cuff = [v for v, j in zip(vertices, joints) if j[0] in (3, 6)]
            self.assertTrue(all(math.hypot(v[0], v[2]) > 0.15 for v in cuff))
            # Exactly one button fan, not a second fastening above the opening.
            button_segments = {3: 12, 2: 8, 1: 6}[detail]
            self.assertEqual(len(surfaces[3][1]) // 3, button_segments)
            collar, underside = surfaces[1][0], surfaces[5][0]
            for outer, inner in zip(collar, underside):
                self.assertAlmostEqual(outer[1] - inner[1], 0.028)
            # Weld the separately shaded faces and verify consistent winding
            # around the closed collar shell. Inward rims vanish under culling.
            edges = Counter()
            for surface in (surfaces[1], surfaces[5]):
                vertices, indices = surface[:2]
                for offset in range(0, len(indices), 3):
                    triangle = [tuple(round(v, 7) for v in vertices[i])
                                for i in indices[offset:offset + 3]]
                    for a, b in zip(triangle, triangle[1:] + triangle[:1]):
                        edges[tuple(sorted((a, b)))] += 1 if a < b else -1
            self.assertTrue(all(count == 0 for count in edges.values()),
                            "collar shell must be closed with outward-facing rims")
        self.assertGreater(triangle_counts[0], triangle_counts[1])
        self.assertGreater(triangle_counts[1], triangle_counts[2])

    def test_person_arm_skin_reaches_elbows_and_wrists_at_every_lod(self):
        for detail in (3, 2, 1):
            vertices, _, joints, _ = person.lod_geometry(detail)
            for upper, lower, hand in ((3, 4, 5), (6, 7, 8)):
                ys = lambda joint: [v[1] for v, j in zip(vertices, joints) if j[0] == joint]
                self.assertLessEqual(min(ys(upper)), person.JOINTS[lower][2][1] + max(ys(lower)))
                self.assertLessEqual(min(ys(lower)), person.JOINTS[hand][2][1] + max(ys(hand)))


if __name__ == "__main__":
    unittest.main()
