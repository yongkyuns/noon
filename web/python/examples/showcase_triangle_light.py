"""Triangular-mirror laboratory, based on the three supplied video screenshots.

Edit the controls below, then Run. The props and character are original vector
artwork over a generated wood texture, not screenshots or a baked video. No YouTube controls are reproduced.
Pink flight -> direction-colored rays -> fast-forward ray-exposure accumulation.
The dense finale is computed from the very same reflected trajectory, NOT noise
or a white fill. Its seven exposures are an explicitly accelerated montage,
not a claim to animate ten thousand individual collisions in real time.

All animation is ordinary retained Noon animation. Python computes geometry and
immutable raster resources once; it never runs a per-frame physics callback.
Reference: https://www.youtube.com/watch?v=z_sXLlLVUls (screenshots 1:11/1:16/1:26).
"""
import colorsys
import math
import struct
import zlib
from array import array

from noon import *


# Illustration coordinates use a 1600 x 900 drawing board, mapped to Noon's
# standard 16:9, eight-scene-unit-high frame. They are not screen-pixel motion.
ART_WIDTH, ART_HEIGHT = 1600.0, 900.0
ART_SCALE = 8.0 / ART_HEIGHT


def xy(point):
    return ((point[0] - ART_WIDTH / 2) * ART_SCALE,
            (ART_HEIGHT / 2 - point[1]) * ART_SCALE, 0.0)


# Unlike the first draft, the reference mirror is large, right-aligned and
# scalene. These are the inside mirror faces, underneath the metallic rim.
TRIANGLE = tuple(xy(p)[:2] for p in ((633, 695), (1362, 695), (941, 222)))
LAUNCH_POINT = xy((826, 536))[:2]
LAUNCH_ANGLE_DEGREES = 17.375
BOUNCES = 36
FLIGHT_SECONDS = 8.0
COLOR_BOUNCES = 48
COLOR_FLIGHT_SECONDS = 5.0
DENSE_BOUNCES = 10000
EXPOSURE_COUNTS = tuple(round(DENSE_BOUNCES * fraction) for fraction in (0.025, 0.05, 0.1, 0.2, 0.4, 0.7, 1.0))
EXPOSURE_WIDTH = 480
EXPOSURE_GAIN = 0.08
RING_BINS = 72


def cross(a, b):
    return a[0] * b[1] - a[1] * b[0]


def difference(a, b):
    return (a[0] - b[0], a[1] - b[1])


def trace_ray(vertices, origin, angle_degrees, bounces):
    """Exact ray/edge intersections and d' = d - 2(d dot n)n, either winding."""
    if isinstance(bounces, bool) or not isinstance(bounces, int) or bounces < 1:
        raise ValueError("BOUNCES must be a positive integer")
    if len(vertices) != 3 or any(len(v) != 2 for v in vertices) or len(origin) != 2:
        raise ValueError("Use exactly three 2D vertices and one 2D launch point")
    if not all(math.isfinite(x) for v in (*vertices, origin) for x in v):
        raise ValueError("Triangle and launch point coordinates must be finite")
    if not math.isfinite(angle_degrees):
        raise ValueError("LAUNCH_ANGLE_DEGREES must be finite")
    edges = [difference(vertices[(i + 1) % 3], vertices[i]) for i in range(3)]
    scale = max(math.hypot(*edge) for edge in edges)
    area = cross(edges[0], difference(vertices[2], vertices[0]))
    if not math.isfinite(scale) or not math.isfinite(area):
        raise ValueError("TRIANGLE coordinates are too large; scale them down")
    if scale == 0 or abs(area) <= 1e-12 * scale * scale:
        raise ValueError("TRIANGLE must have non-zero area")
    winding = 1.0 if area > 0 else -1.0
    if any(winding * cross(edge, difference(origin, vertices[i])) <= 1e-10 * scale * scale
           for i, edge in enumerate(edges)):
        raise ValueError("LAUNCH_POINT must be strictly inside TRIANGLE")
    angle = math.radians(angle_degrees % 360.0)
    direction = (math.cos(angle), math.sin(angle))
    position = tuple(origin)
    points, walls = [position], []
    for bounce in range(bounces):
        candidates = []
        for wall, edge in enumerate(edges):
            denominator = cross(direction, edge)
            if abs(denominator) <= 1e-12 * math.hypot(*edge):
                continue
            offset = difference(vertices[wall], position)
            distance = cross(offset, edge) / denominator
            fraction = cross(offset, direction) / denominator
            if distance > 1e-10 * scale and -1e-10 <= fraction <= 1.0 + 1e-10:
                candidates.append((distance, wall, fraction))
        if not candidates:
            raise ValueError("No forward wall intersection; move the launch away from a corner")
        _, wall, fraction = min(candidates)
        if fraction <= 1e-9 or fraction >= 1.0 - 1e-9:
            raise ValueError(f"Ray hits a triangle corner at bounce {bounce + 1}; change LAUNCH_ANGLE_DEGREES slightly")
        edge = edges[wall]
        # Reconstruct on the mirror; do not accumulate epsilon position nudges.
        position = (vertices[wall][0] + fraction * edge[0],
                    vertices[wall][1] + fraction * edge[1])
        points.append(position)
        walls.append(wall)
        length = math.hypot(*edge)
        normal = (-edge[1] / length, edge[0] / length)
        projection = direction[0] * normal[0] + direction[1] * normal[1]
        reflected = (direction[0] - 2 * projection * normal[0],
                     direction[1] - 2 * projection * normal[1])
        norm = math.hypot(*reflected)
        direction = (reflected[0] / norm, reflected[1] / norm)
    return points, walls


def flight_durations(points, seconds):
    if not math.isfinite(seconds) or seconds <= 0:
        raise ValueError("FLIGHT_SECONDS must be finite and positive")
    lengths = [math.dist(a, b) for a, b in zip(points, points[1:])]
    if not lengths or any(length <= 0 or not math.isfinite(length) for length in lengths):
        raise ValueError("The ray path must contain finite, non-zero segments")
    total = math.fsum(lengths)
    return [seconds * length / total for length in lengths]


def direction_hue(a, b):
    # Screen-clockwise hue: red right, green down, cyan left, purple up.
    return (math.atan2(a[1] - b[1], b[0] - a[0]) / math.tau) % 1.0


def direction_histogram(points, bins=RING_BINS):
    counts = [0] * bins
    for a, b in zip(points, points[1:]):
        counts[min(bins - 1, int(direction_hue(a, b) * bins))] += 1
    return counts


def rgba_png(width, height, rgba):
    """Encode an immutable RGBA resource with the standard library, no Pillow."""
    if len(rgba) != width * height * 4:
        raise ValueError("RGBA buffer length does not match its dimensions")
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    scanlines = b"".join(b"\0" + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">2I5B", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(scanlines, 4)) + chunk(b"IEND", b""))


def accumulate_ray_exposures(vertices, points, checkpoints, width=EXPOSURE_WIDTH):
    """Add light from actual segments, then tone-map it into bounded PNG assets.

    Bresenham traversal is only image rasterization; collision geometry above
    is analytic. The segment energy is weighted by its Euclidean pixel length.
    Outside-triangle pixels stay transparent. No random samples are generated.
    """
    if (isinstance(width, bool) or not isinstance(width, int) or not 16 <= width <= 1024):
        raise ValueError("Exposure width must be an integer from 16 to 1024")
    if (not checkpoints or tuple(sorted(set(checkpoints))) != tuple(checkpoints)
            or any(isinstance(n, bool) or not isinstance(n, int) or not 1 <= n < len(points) for n in checkpoints)):
        raise ValueError("Exposure checkpoints must be increasing valid collision counts")
    xmin, xmax = min(p[0] for p in vertices), max(p[0] for p in vertices)
    ymin, ymax = min(p[1] for p in vertices), max(p[1] for p in vertices)
    height = max(1, round(width * (ymax - ymin) / (xmax - xmin)))
    energy = array("f", [0.0]) * (width * height * 3)
    edge_sign = 1 if cross(difference(vertices[1], vertices[0]), difference(vertices[2], vertices[0])) > 0 else -1
    interior = []
    for row in range(height):
        for col in range(width):
            point = (xmin + (col + 0.5) * (xmax - xmin) / width,
                     ymax - (row + 0.5) * (ymax - ymin) / height)
            if all(edge_sign * cross(difference(vertices[(i + 1) % 3], vertices[i]), difference(point, vertices[i])) >= 0
                   for i in range(3)):
                interior.append(row * width + col)
    def pixel(p):
        return (min(width - 1, max(0, round((p[0] - xmin) * width / (xmax - xmin) - 0.5))),
                min(height - 1, max(0, round((ymax - p[1]) * height / (ymax - ymin) - 0.5))))
    wanted = set(checkpoints)
    snapshots = []
    for count, (a, b) in enumerate(zip(points, points[1:]), 1):
        x, y = pixel(a)
        end_x, end_y = pixel(b)
        dx, dy = abs(end_x - x), -abs(end_y - y)
        sx, sy = (1 if x < end_x else -1), (1 if y < end_y else -1)
        error = dx + dy
        weight = math.hypot(dx, dy) / max(1, dx, -dy)
        light = tuple(channel * weight for channel in colorsys.hsv_to_rgb(direction_hue(a, b), 0.72, 1.0))
        while True:
            at = (y * width + x) * 3
            energy[at] += light[0]
            energy[at + 1] += light[1]
            energy[at + 2] += light[2]
            if x == end_x and y == end_y:
                break
            twice = 2 * error
            if twice >= dy:
                error += dy
                x += sx
            if twice <= dx:
                error += dx
                y += sy
        if count in wanted:
            rgba = bytearray(width * height * 4)
            for index in interior:
                for channel, base in enumerate((27, 28, 48)):
                    value = math.sqrt(1.0 - math.exp(-EXPOSURE_GAIN * energy[index * 3 + channel]))
                    rgba[index * 4 + channel] = min(255, round(base + (255 - base) * value))
                rgba[index * 4 + 3] = 255
            snapshots.append(rgba_png(width, height, rgba))
        if count == checkpoints[-1]:
            break
    return snapshots, (xmin, ymin, xmax, ymax)



def wood_texture(width=640, height=360):
    """A soft procedural wood backdrop, separate from the ray-exposure image."""
    pixels = bytearray(width * height * 4)
    for row in range(height):
        v = row / height
        for col in range(width):
            u = col / width
            light = math.exp(-((u - 0.49) ** 2 / 0.19 + (v - 0.38) ** 2 / 0.75))
            grain = 1.4 * math.sin(col * 0.87 + math.sin(row * 0.017)) + 1.0 * math.sin(col * 1.93 + row * 0.015)
            board = 1.8 * math.cos(math.floor(u * 24) * 1.71)
            at = (row * width + col) * 4
            for channel, (base, lift) in enumerate(((115, 31), (73, 23), (52, 18))):
                pixels[at + channel] = round(base + lift * light + grain + board)
            pixels[at + 3] = 255
    return rgba_png(width, height, pixels)


def ink(value):
    return Color(*(int(value[i:i + 2], 16) / 255.0 for i in (1, 3, 5)))


def artwork_path(points, color, width=1.0, fill=None, opacity=1.0, z=0):
    obj = VMobject(stroke_color=ink(color), stroke_width=width * ART_SCALE / 0.01,
                   fill_color=ink(fill or color), fill_opacity=opacity if fill else 0,
                   stroke_opacity=opacity, z_index=z)
    coords = [xy(point) for point in points]
    if fill:
        coords.append(coords[0])
    return obj.set_points_as_corners(coords)


def panel(x, y, w, h, color, radius=0, stroke=None, width=1, opacity=1, z=0):
    if radius:
        points = []
        for cx, cy, start in ((x + w - radius, y + radius, -math.pi / 2),
                              (x + w - radius, y + h - radius, 0),
                              (x + radius, y + h - radius, math.pi / 2),
                              (x + radius, y + radius, math.pi)):
            points.extend((cx + radius * math.cos(start + j * math.pi / 10),
                           cy + radius * math.sin(start + j * math.pi / 10)) for j in range(6))
    else:
        points = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
    return artwork_path(points, stroke or color, width if stroke else 0, color, opacity, z)


def oval(cx, cy, rx, ry, color, opacity=1, stroke=None, width=1, z=0):
    points = [(cx + rx * math.cos(i * math.tau / 48), cy + ry * math.sin(i * math.tau / 48)) for i in range(48)]
    return artwork_path(points, stroke or color, width if stroke else 0, color, opacity, z)


def arm_parts(raised):
    if raised:
        left = [(279, 622), (263, 609), (219, 558), (205, 575), (253, 648), (274, 651)]
        right = [(367, 618), (382, 603), (413, 558), (432, 574), (387, 646), (367, 649)]
        hands = ((219, 564), (422, 565))
        pen = [(425, 551), (444, 525)]
    else:
        left = [(280, 622), (260, 625), (237, 698), (253, 705), (277, 662), (282, 645)]
        right = [(368, 622), (387, 624), (400, 712), (382, 716), (366, 662), (363, 644)]
        hands = ((245, 710), (391, 723))
        pen = [(393, 730), (401, 763)]
    return [artwork_path(left, "#c5ced8", 2, "#edf1f7"),
            artwork_path(right, "#c5ced8", 2, "#edf1f7"),
            oval(*hands[0], 14, 15, "#f1c997"), oval(*hands[1], 14, 15, "#f1c997"),
            artwork_path(pen, "#354268", 7)]


def laboratory_artwork():
    """Original vector reconstruction of the reference's warm illustrated set."""
    objects = [ImageMobject(wood_texture(), height=8.0, resampling_algorithm="bilinear")]
    # The panelled dado and floor remain ordinary retained vector shapes.
    objects += [panel(0, 635, 1600, 184, "#69422e"), panel(0, 630, 1600, 9, "#9b6543"),
                panel(0, 806, 1600, 12, "#482c20"), panel(0, 818, 1600, 82, "#755034")]
    for i in range(18):
        objects.append(panel(i * 92 + 9, 651, 85, 151, "#6e4730", 5, "#895a3c", 2))
    for y in (831, 847, 870, 896):
        objects.append(artwork_path([(0, y), (1600, y)], "#4c3528", 3))
    for i, y in enumerate((825, 846, 871)):
        for x in range(100 + i * 149, 1600, 420):
            objects.append(artwork_path([(x, y), (x + 18, y + 21)], "#4c3528", 2))
    # Chalkboard with the sparse marks visible in the reference.
    objects += [panel(61, 108, 414, 251, "#705733", 5), panel(71, 116, 393, 229, "#2b4b45", 3),
                panel(84, 354, 365, 9, "#624728", 2)]
    objects += [artwork_path([(85, 143), (122, 143)], "#5a7d6d", 2),
                artwork_path([(85, 155), (110, 155)], "#5a7d6d", 2),
                oval(431, 143, 13, 13, "#2b4b45", stroke="#789380", width=2),
                artwork_path([(98 + i * 3, 325 - 13 * math.sin(i * 0.25)) for i in range(25)], "#6c8c7a", 2),
                panel(135, 350, 25, 6, "#e9e6cd", 1), panel(173, 350, 16, 6, "#e4bf4c", 1),
                panel(368, 347, 36, 8, "#c95647", 1)]
    # Hanging lights, with baked vector halos instead of dozens of GPU effects.
    for left, right, sag in ((-20, 731, 30), (1175, 1620, 18)):
        cord = [(left + (right - left) * i / 60, 24 + sag * math.sin(math.pi * i / 60)) for i in range(61)]
        objects.append(artwork_path(cord, "#493b2a", 2))
        for i in range(1, 23 if left < 0 else 20):
            t = i / (23 if left < 0 else 20)
            x, y = left + (right - left) * t, 24 + sag * math.sin(math.pi * t)
            objects += [oval(x, y + 9, 13, 17, "#f4d69c", 0.06), oval(x, y + 9, 8, 12, "#f5d08e", 0.11),
                        artwork_path([(x, y), (x, y + 6)], "#52442b", 2), oval(x, y + 10, 3.6, 5.2, "#f2d497")]
    for cx, cy in ((470, 65), (1338, 47)):
        objects += [artwork_path([(cx, 0), (cx, cy - 27)], "#594d39", 2), panel(cx - 5, cy - 28, 10, 10, "#b29351", 2),
                    artwork_path([(cx - 39, cy + 7), (cx - 32, cy - 11), (cx - 17, cy - 22), (cx + 17, cy - 22),
                                  (cx + 32, cy - 11), (cx + 39, cy + 7)], "#285a53", 0, "#285a53"),
                    oval(cx, cy + 7, 39, 6, "#f4e7bb")]
    for i in range(20):
        objects.append(oval((97 * i + 31) % 1600, (149 * i + 30) % 615,
                            1.1, 1.4, "#e2bd94", opacity=0.3))
    # Bookcase, books, globe and a small abstract illustrated cover.
    objects += [panel(1417, 262, 185, 376, "#69432c"), panel(1417, 400, 185, 10, "#9a6a43"),
                panel(1417, 566, 185, 11, "#9a6a43")]
    book_colors = ("#e0af45", "#da8051", "#bf5450", "#987244", "#4c97b3")
    for i, color in enumerate(book_colors):
        x, y = 1431 + 22 * i, 330 + (0 if i == 0 else 12)
        objects += [panel(x, y, 19, 70 - (y - 330), color, 2),
                    artwork_path([(x + 4, y + 8), (x + 16, y + 8)], "#f1d9a2", 2),
                    artwork_path([(x + 4, y + 12), (x + 4, 399)], "#f1d9a2", 1, opacity=0.3)]
    for i, color in enumerate(("#5098ae", "#d4a93f", "#4b91b5")):
        x, y = 1427 + 20 * i, 519 - i * 12
        objects += [panel(x, y, 18, 47 + i * 12, color, 2), artwork_path([(x + 3, y + 8), (x + 14, y + 8)], "#b1d5c8", 2)]
    objects += [oval(1568, 365, 22, 22, "#438bc2"),
                artwork_path([(1549, 358), (1555, 348), (1565, 352), (1571, 362), (1586, 363), (1581, 372),
                              (1567, 369), (1560, 359)], "#78ae62", 0, "#78ae62"),
                panel(1496, 517, 97, 49, "#28343a", 1), panel(1500, 521, 89, 41, "#5d7183"),
                artwork_path([(1500, 550), (1523, 531), (1549, 555), (1570, 525), (1589, 552)], "#b4c99b", 3)]
    # Cream wall clock.
    objects.append(oval(1502, 187, 43, 43, "#eee7c8"))
    for i in range(12):
        angle = i * math.tau / 12
        objects.append(artwork_path([(1502 + r * math.sin(angle), 187 - r * math.cos(angle)) for r in (34, 39)], "#494566", 3.5))
    clock_hands = [artwork_path([(1502, 187), end], "#343c54", 3)
                   for end in ((1480, 154), (1529, 174))]
    objects += [*clock_hands, oval(1502, 187, 4, 4, "#c25146")]
    # Mirror stand, side table, mug and steam.
    for x in (727, 1268):
        objects += [panel(x - 5, 699, 9, 112, "#758395"), panel(x, 699, 2, 112, "#b7bfc9"),
                    panel(x - 31, 810, 60, 10, "#535b70", 5)]
    objects += [panel(488, 716, 132, 11, "#9a6c40", 5), panel(504, 726, 8, 89, "#4e3828"),
                panel(603, 726, 8, 89, "#4e3828"),
                artwork_path([(509, 714), (568, 713), (566, 705), (510, 707)], "#efe4c9", 0, "#efe4c9"),
                artwork_path([(514, 704), (570, 710), (571, 702), (515, 696)], "#fff0d4", 0, "#fff0d4"),
                oval(608, 697, 8, 10, "#87583f", stroke="#c67142", width=5),
                panel(576, 683, 32, 33, "#c97646", 5), oval(592, 684, 16, 4, "#633e2e"),
                artwork_path([(590 + 4 * math.sin(i / 3), 679 - i * 2) for i in range(23)], "#c7aa82", 2, opacity=0.45)]
    # Plant and warm terracotta pot, plus the rug below the character.
    for angle, length in ((-0.5, 133), (-0.28, 157), (-0.05, 168), (0.15, 153), (0.40, 119), (0.65, 105)):
        base = (76, 744)
        tip = (76 + math.sin(angle) * length, 744 - math.cos(angle) * length)
        objects.append(artwork_path([base, (tip[0] - 5, tip[1] + 7), tip, (tip[0] + 6, tip[1] + 4)], "#70a664", 0, "#70a664"))
    objects += [artwork_path([(39, 741), (102, 741), (115, 816), (27, 816)], "#ad6a43", 0, "#ad6a43"),
                panel(30, 734, 87, 13, "#b57248", 4), oval(320, 833, 184, 33, "#a1473d"),
                oval(320, 833, 164, 25, "#a1473d", stroke="#c79754", width=2),
                oval(320, 833, 153, 18, "#a1473d", stroke="#be6c4e", width=1)]
    # Original bespectacled scientist: coat, trousers, warm hair, large eyes.
    objects += [panel(304, 753, 23, 53, "#3c526f", 2), panel(336, 753, 23, 53, "#3c526f", 2),
                panel(301, 802, 29, 16, "#3c342c", 6), panel(334, 802, 31, 16, "#3c342c", 6)]
    arms = arm_parts(True)
    objects.extend(arms)
    objects += [artwork_path([(272, 627), (291, 610), (350, 610), (371, 629), (376, 772), (267, 772)], "#cbd2da", 2, "#eef1f5"),
                artwork_path([(300, 616), (285, 640), (303, 771), (315, 771)], "#d6dee8", 0, "#d6dee8"),
                artwork_path([(344, 616), (359, 640), (345, 771), (334, 771)], "#d6dee8", 0, "#d6dee8"),
                panel(308, 593, 25, 26, "#e2b787", 5),
                artwork_path([(318, 617), (308, 635), (313, 668), (327, 676), (337, 665), (330, 633), (325, 617)], "#a0c9df", 0, "#a0c9df"),
                panel(347, 655, 18, 22, "#d6dde5", 2), panel(352, 648, 5, 12, "#72a4cb", 1),
                artwork_path([(324, 677), (324, 769)], "#c5cdd6", 1.5)]
    objects += [oval(321, 529, 63, 66, "#b49155"), oval(271, 550, 11, 15, "#e8bf8e"),
                oval(375, 550, 11, 15, "#e8bf8e"), oval(323, 545, 51, 55, "#efc996"),
                artwork_path([(269, 526), (280, 487), (300, 467), (319, 467), (336, 472), (355, 482),
                              (373, 506), (376, 527), (355, 518), (329, 501), (318, 518), (299, 517), (280, 537)], "#b39053", 0, "#b39053"),
                artwork_path([(324, 470), (319, 490), (316, 506)], "#9d7b46", 4),
                artwork_path([(337, 477), (331, 492), (327, 505)], "#9d7b46", 3),
                oval(301, 542, 14, 21, "#f6e6cc"), oval(347, 542, 14, 21, "#f6e6cc"),
                oval(307, 545, 8.5, 15, "#243448"), oval(352, 545, 8.5, 15, "#243448"),
                oval(309, 537, 3.5, 4, "#ffffff"), oval(354, 537, 3.5, 4, "#ffffff"),
                artwork_path([(282, 529), (300, 525), (314, 529)], "#80623c", 3),
                artwork_path([(334, 529), (349, 525), (364, 529)], "#80623c", 3),
                panel(277, 533, 41, 32, "#edcca2", 7, "#394458", 3.5, opacity=0.13),
                panel(330, 533, 41, 32, "#edcca2", 7, "#394458", 3.5, opacity=0.13)]
    # Opaque outlines separate from the gently tinted lenses.
    for x in (277, 330):
        lens = panel(x, 533, 41, 32, "#edcca2", 7, "#394458", 3.5)
        lens.set_fill(opacity=0)
        objects.append(lens)
    objects += [artwork_path([(318, 545), (323, 542), (330, 545)], "#394458", 3.5),
                artwork_path([(276, 541), (265, 537)], "#394458", 3.5),
                oval(325, 560, 5, 7, "#ddb184")]
    mouth = oval(324, 581, 4.5, 5.5, "#98644b")
    objects.append(mouth)
    return objects, arms, mouth, clock_hands


def mirror_artwork():
    boundary = [(619, 704), (1381, 704), (941, 205), (619, 704)]
    glass = artwork_path(boundary[:-1], "#1b1c30", 0, "#1b1c30", z=3)
    rim = [artwork_path(boundary, color, width, z=30) for color, width in
           (("#5c6682", 18), ("#98accb", 12), ("#d9e3f0", 7), ("#f8f8f3", 2))]
    return glass, rim


def world_ray(a, b, color, width, z=10):
    return VMobject(stroke_color=color, stroke_width=width, fill_opacity=0,
                    z_index=z).set_points_as_corners([(*a, 0), (*b, 0)])


def direction_ring(counts):
    # Bar lengths are the actual finite direction histogram, not decorative noise.
    maximum = max(1, max(counts))
    bars = []
    for i, count in enumerate(counts):
        angle = (i + 0.5) * math.tau / len(counts)
        outer = 60 + 37 * math.sqrt(count / maximum)
        points = [(1320 + r * math.cos(a), 314 + r * math.sin(a))
                  for r, a in ((36, angle - 0.026), (outer, angle - 0.026),
                               (outer, angle + 0.026), (36, angle + 0.026))]
        color = "#%02x%02x%02x" % tuple(round(v * 255) for v in colorsys.hsv_to_rgb(i / len(counts), 0.63, 1))
        bars.append(artwork_path(points, color, 0, color, z=5))
    return Group(*bars)


class TriangleLightShowcase(Scene):
    def construct(self):
        visible_count = BOUNCES + COLOR_BOUNCES
        points, _ = trace_ray(TRIANGLE, LAUNCH_POINT, LAUNCH_ANGLE_DEGREES, visible_count + DENSE_BOUNCES)
        checkpoints = tuple(visible_count + count for count in EXPOSURE_COUNTS)
        pngs, bounds = accumulate_ray_exposures(TRIANGLE, points, checkpoints)
        room, arms, mouth, clock_hands = laboratory_artwork()
        glass, rim = mirror_artwork()
        photon = Dot((*points[0], 0), radius=0.029, color=WHITE, z_index=25)
        photon.set_glow(color=ink("#ff749c"), radius=Pixels(8), intensity=2.6)
        self.play(FadeIn(Group(*room)), FadeIn(glass), FadeIn(Group(*rim)), FadeIn(photon), run_time=0.8)

        beam = None
        for section, begin, end, seconds in ((0, 0, BOUNCES, FLIGHT_SECONDS),
                                             (1, BOUNCES, visible_count, COLOR_FLIGHT_SECONDS)):
            section_points = points[begin:end + 1]
            for index, duration in enumerate(flight_durations(section_points, seconds), begin):
                a, b = points[index:index + 2]
                hue = direction_hue(a, b)
                color = ink("#ff7097") if section == 0 else Color(*colorsys.hsv_to_rgb(hue, 0.63, 0.93))
                trail = world_ray(a, b, color, 0.95)
                trail.set_opacity(0.22 if section == 0 else 0.65)
                halo = world_ray(a, b, color, 3.2, z=21).set_opacity(0.7)
                core = world_ray(a, b, ink("#fff2f5"), 0.95, z=22)
                next_beam = (halo, core)
                animations = [Create(trail), Create(halo), Create(core), photon.animate.move_to((*b, 0))]
                if beam is not None:
                    animations.extend(FadeOut(part) for part in beam)
                self.play(*animations, run_time=duration, rate_func=linear)
                beam = next_beam
            if section == 0:
                # Match the change from the raised-arm shot to the resting pose.
                resting = arm_parts(False)
                smile = artwork_path([(317, 579), (322, 583), (329, 582), (333, 577)], "#98644b", 2)
                clock_targets = [artwork_path([(1502, 187), end], "#343c54", 3)
                                 for end in ((1529, 158), (1515, 166))]
                self.play(*[Transform(old, new) for old, new in zip(arms, resting)],
                          *[Transform(old, new) for old, new in zip(clock_hands, clock_targets)],
                          FadeOut(mouth), FadeIn(smile), run_time=0.6)

        left, _, top = TRIANGLE
        angle = math.atan2(top[1] - left[1], top[0] - left[0])
        arc = VMobject(stroke_color=ink("#ffd75c"), stroke_width=3.2, fill_opacity=0, z_index=24)
        radius = 64 * ART_SCALE
        arc.set_points_as_corners([(left[0] + radius * math.cos(i * angle / 36),
                                    left[1] + radius * math.sin(i * angle / 36), 0) for i in range(37)])
        arc.set_glow(color=ink("#ffd75c"), radius=Pixels(4), intensity=1.6)
        self.play(Create(arc), *[FadeOut(part) for part in beam], FadeOut(photon), run_time=0.6)
        self.wait(1.0)

        ring = direction_ring(direction_histogram(points))
        xmin, ymin, xmax, ymax = bounds
        previous = None
        for index, png in enumerate(pngs):
            exposure = ImageMobject(png, height=ymax - ymin, resampling_algorithm="bilinear", z_index=18)
            exposure.move_to(((xmin + xmax) / 2, (ymin + ymax) / 2, 0))
            animations = [FadeIn(exposure)]
            if previous is not None:
                animations.append(FadeOut(previous))
            else:
                animations.append(FadeOut(arc))
            if index == len(pngs) - 1:
                # The full direction histogram appears with the full exposure,
                # not prematurely as if future collisions had already happened.
                animations.append(FadeIn(ring))
                clock_targets = [artwork_path([(1502, 187), end], "#343c54", 3)
                                 for end in ((1490, 220), (1527, 188))]
                animations += [Transform(old, new) for old, new in zip(clock_hands, clock_targets)]
            self.play(*animations, run_time=1.0, rate_func=linear)
            previous = exposure
        self.wait(2.0)
