"""Measure equal volumes in a steady, constant-depth channel."""
from noon import *

THROAT_WIDTH = 2.0  # Try 1.5. The same volume passes through a smaller area each second.
INLET_WIDTH = 4.0
DEPTH = 1.0
FLOW = 2.0  # Cubic meters per second.
BANK = color_from_hex("#2c4351")
BED = color_from_hex("#14364a")
RIM = color_from_hex("#517383")
DYE = color_from_hex("#a5f9ff")
UPSTREAM = color_from_hex("#ffc375")
THROAT = color_from_hex("#78e3b9")
INK = color_from_hex("#eff9f7")
MUTED = color_from_hex("#adcad1")


def channel_width(x):
    t = max(0.0, min(1.0, (x + 1.65) / 2.1))
    t = t * t * (3 - 2 * t)
    return INLET_WIDTH + (THROAT_WIDTH - INLET_WIDTH) * t


def advect(x, seconds):
    # Follow a normalized lateral coordinate in a depth-averaged sketch of the flow.
    # At a given x, longitudinal speed is discharge divided by cross-sectional area.
    for _ in range(12):
        x += seconds * FLOW / (12 * DEPTH * channel_width(x))
    return x


def text(value, size, x, y, color=INK):
    return Text(value, font_size=size, color=color).move_to((x, y, 0))


class ChannelFlow(ThreeDScene):
    def reveal_labels(self, labels):
        # Attach to the screen before the fade so the text stays fixed as the camera turns.
        for label in labels:
            label.set_opacity(0)
        self.add_fixed_in_frame_mobjects(*labels)
        return [label.animate.set_opacity(1) for label in labels]

    def draw_guide(self, start, end, axis, color):
        dimensions = [0.055, 0.055, 0.055]
        dimensions[axis] = 1
        guide = Mesh3D.prism(dimensions=dimensions, color=color)
        length = end[axis] - start[axis]
        initial_scale = [1, 1, 1]
        initial_scale[axis] = 0.02
        initial_center = tuple(value + 0.01 if index == axis else value
                               for index, value in enumerate(start))
        self.add_world_mobjects(guide)
        self.set_world_transform(guide, translation=initial_center, scale=initial_scale)
        final_scale = [1, 1, 1]
        final_scale[axis] = length
        center = tuple((a + b) / 2 for a, b in zip(start, end))
        return WorldTransformTo(guide, translation=center, scale=final_scale)

    def section_frame(self, x, width, color):
        # Keep this guide just upstream so it stays clear of the swept volume.
        x -= 0.04
        a, b = -width / 2, width / 2
        return [
            self.draw_guide((x, a, DEPTH), (x, b, DEPTH), 1, color),
            self.draw_guide((x, a, 0), (x, a, DEPTH), 2, color),
            self.draw_guide((x, b, 0), (x, b, DEPTH), 2, color),
        ]

    def construct(self):
        if not 1.35 <= THROAT_WIDTH < INLET_WIDTH:
            raise ValueError("Choose a throat width from 1.35 up to, but below, 4 meters.")
        self.set_camera_orientation(phi=0.85, theta=-1.34, focal_distance=10, zoom=0.91)
        bed = Mesh3D.parametric(
            lambda x, v: (x, (v - 0.5) * channel_width(x), 0),
            u_range=(-4.55, 4.55), v_range=(0, 1), resolution=(48, 2), color=BED, point_lit=True,
        )
        self.add_world_mobjects(bed)
        for side in (-1, 1):
            wall = Mesh3D.parametric(
                lambda x, z: (x, side * channel_width(x) / 2, z),
                u_range=(-4.55, 4.55), v_range=(0, 1), resolution=(48, 2), color=BANK, point_lit=True,
            )
            coping = Mesh3D.parametric(
                lambda x, y: (x, side * (channel_width(x) / 2 + 0.28 * y), DEPTH),
                u_range=(-4.55, 4.55), v_range=(0, 1), resolution=(48, 2), color=RIM, point_lit=True,
            )
            self.add_world_mobjects(wall, coping)
        self.add_world_mobjects(
            self.point_light(position=(-2, -3, 6), color=WHITE, intensity=1)
        )

        self.next_section("Follow the dye")
        heading = text("Why does the dye stretch?", 43, 0, 3.11)
        caption = text("Follow one dye pulse through the throat", 29, 0, -3.23, MUTED)
        self.add_fixed_in_frame_mobjects(heading, caption)
        # Three rows of dye markers travel together. Their spacing, both along
        # and across the channel, reveals the change in the same parcel of water.
        markers = []
        stations = [-2.72, -2.38, -2.04]
        for i, x in enumerate(stations):
            for u in (-0.28, 0, 0.28):
                dot = Mesh3D.sphere(radius=0.095, resolution=(10, 6), color=DYE)
                dot.shift((x, u * channel_width(x), DEPTH + 0.08))
                markers.append((dot, i, u))
                self.add_world_mobjects(dot)
        for _ in range(10):
            stations = [advect(x, 0.5) for x in stations]
            self.play(*(
                WorldTransformTo(dot, translation=(stations[i], u * channel_width(stations[i]), DEPTH + 0.08))
                for dot, i, u in markers
            ), run_time=0.35, rate_func=linear)
        self.wait(0.9)

        self.next_section("Find the cross-sections")
        self.play(
            FadeOut(heading), FadeOut(caption),
            *(WorldTransformTo(dot, translation=(stations[i] + 0.2, u * channel_width(stations[i]), DEPTH + 0.08),
                               scale=(0.025, 0.025, 0.025)) for dot, i, u in markers),
            run_time=0.6,
        )
        self.remove(*(dot for dot, _, _ in markers))
        heading = text("Where can the water go?", 42, 0, 3.11)
        caption = text("Both sections are 1 m deep", 29, 0, -3.23, MUTED)
        inlet_x, throat_x = -3.23, 1.58
        self.move_camera(phi=0.30, theta=-1.57, focal_distance=10, zoom=0.97, run_time=1.6,
                         added_anims=tuple(self.reveal_labels((heading, caption))))
        wide = text(f"Inlet · {INLET_WIDTH:g} m", 30, -3.34, 2.54, UPSTREAM)
        narrow = text(f"Throat · {THROAT_WIDTH:g} m", 30, 2.64, 2.54, THROAT)
        self.play(*self.section_frame(inlet_x, INLET_WIDTH, UPSTREAM),
                  *self.reveal_labels((wide,)), run_time=0.6)
        self.play(*self.section_frame(throat_x, THROAT_WIDTH, THROAT),
                  *self.reveal_labels((narrow,)), run_time=0.6)
        self.wait(1.8)

        self.next_section("Measure one second")
        self.play(*(FadeOut(label) for label in (heading, caption, wide, narrow)), run_time=0.4)
        heading = text("How much water passes in one second?", 40, 0, 3.11)
        caption = text("In steady flow, water does not build up here", 30, 0, -3.23, MUTED)
        self.move_camera(phi=1.07, theta=-1.27, focal_distance=10, zoom=0.91, run_time=1.4,
                         added_anims=tuple(self.reveal_labels((heading, caption))))
        sweeps = []
        for x, width, color in ((inlet_x, INLET_WIDTH, UPSTREAM), (throat_x, THROAT_WIDTH, THROAT)):
            distance = FLOW / (width * DEPTH)
            # The thin leading slice grows downstream into the one-second volume.
            block = Mesh3D.prism(dimensions=(1, width, DEPTH), color=color, point_lit=True)
            self.add_world_mobjects(block)
            self.set_world_transform(block, translation=(x + 0.04, 0, DEPTH / 2 + 0.02),
                                     scale=(0.08, 0.99, 0.99))
            sweeps.append(WorldTransformTo(block, translation=(x + distance / 2, 0, DEPTH / 2 + 0.02),
                                           scale=(distance, 0.99, 0.99)))
        self.wait(2)
        self.play(*sweeps, run_time=2, rate_func=linear)
        self.play(FadeOut(caption), run_time=0.3)
        inlet_distance = FLOW / (INLET_WIDTH * DEPTH)
        throat_distance = FLOW / (THROAT_WIDTH * DEPTH)
        wide = text(f"{INLET_WIDTH:g} × 1 × {inlet_distance:.2f} = {FLOW:g} m³", 31, -3.53, -2.62, UPSTREAM)
        relation = "=" if round(throat_distance, 2) == throat_distance else "≈"
        narrow = text(f"{THROAT_WIDTH:g} × 1 × {throat_distance:.2f} {relation} {FLOW:g} m³",
                      31, 3.53, -2.62, THROAT)
        factors = text("Width × depth × distance", 29, 0, -3.27, MUTED)
        self.play(*self.reveal_labels((wide, narrow, factors)), run_time=0.7)
        self.wait(2.4)

        self.next_section("Explain the stretch")
        self.play(FadeOut(factors), run_time=0.3)
        takeaway = text("Same volume per second · higher speed", 30, 0, -3.27, INK)
        # Draw the distances along the flow while the viewing angle holds.
        z = DEPTH + 0.09
        self.play(
            self.draw_guide((inlet_x, 0, z), (inlet_x + inlet_distance, 0, z), 0, INK),
            self.draw_guide((throat_x, 0, z), (throat_x + throat_distance, 0, z), 0, INK),
            *self.reveal_labels((takeaway,)), run_time=0.8,
        )
        self.wait(4.4)
