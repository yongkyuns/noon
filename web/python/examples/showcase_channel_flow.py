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


def advect(x):
    # Follow a normalized lateral coordinate in a depth-averaged sketch of the flow.
    # At a given x, longitudinal speed is discharge divided by cross-sectional area.
    for _ in range(24):
        x += FLOW / (24 * DEPTH * channel_width(x))
    return x


def text(value, size, x, y, color=INK):
    return Text(value, font_size=size, color=color).move_to((x, y, 0))


def channel_surface(function, color):
    return Mesh3D.parametric(function, u_range=(-4.55, 4.55), v_range=(0, 1),
                            resolution=(48, 2), color=color, point_lit=True)


def section_frame(x, width, color):
    a, b = -width / 2, width / 2
    return [
        Mesh3D.line3d((x, a, DEPTH), (x, b, DEPTH), thickness=0.035, color=color),
        Mesh3D.line3d((x, a, 0), (x, a, DEPTH), thickness=0.035, color=color),
        Mesh3D.line3d((x, b, 0), (x, b, DEPTH), thickness=0.035, color=color),
    ]


class ChannelFlow(ThreeDScene):
    def construct(self):
        if not 1.35 <= THROAT_WIDTH < INLET_WIDTH:
            raise ValueError("Choose a throat width from 1.35 up to, but below, 4 meters.")
        self.set_camera_orientation(phi=0.85, theta=-1.34, focal_distance=10, zoom=0.91)
        bed = channel_surface(lambda x, v: (x, (v - 0.5) * channel_width(x), 0), BED)
        self.add_world_mobjects(bed)
        for side in (-1, 1):
            wall = channel_surface(lambda x, z: (x, side * channel_width(x) / 2, z), BANK)
            coping = channel_surface(
                lambda x, y: (x, side * (channel_width(x) / 2 + 0.28 * y), DEPTH), RIM
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
        for _ in range(5):
            stations = [advect(x) for x in stations]
            self.play(*(
                WorldTransformTo(dot, translation=(stations[i], u * channel_width(stations[i]), DEPTH + 0.08))
                for dot, i, u in markers
            ), run_time=0.7, rate_func=linear)
        self.wait(0.9)

        self.next_section("Find the cross-sections")
        self.remove(heading, caption)
        self.remove(*(dot for dot, _, _ in markers))
        heading = text("Where can the water go?", 42, 0, 3.11)
        caption = text("Both sections are 1 m deep", 29, 0, -3.23, MUTED)
        self.add_fixed_in_frame_mobjects(heading, caption)
        inlet_x, throat_x = -3.23, 1.58
        inlet_section = section_frame(inlet_x, INLET_WIDTH, UPSTREAM)
        throat_section = section_frame(throat_x, THROAT_WIDTH, THROAT)
        self.add_world_mobjects(*inlet_section, *throat_section)
        self.move_camera(phi=0.30, theta=-1.57, focal_distance=10, zoom=0.97, run_time=1.6)
        wide = text(f"Inlet · {INLET_WIDTH:g} m", 30, -3.34, 2.54, UPSTREAM)
        narrow = text(f"Throat · {THROAT_WIDTH:g} m", 30, 2.64, 2.54, THROAT)
        self.add_fixed_in_frame_mobjects(wide, narrow)
        self.wait(2)

        self.next_section("Measure one second")
        self.remove(heading, caption, wide, narrow)
        self.add_fixed_in_frame_mobjects(
            text("How much water passes in one second?", 40, 0, 3.11),
        )
        self.move_camera(phi=1.07, theta=-1.27, focal_distance=10, zoom=0.91, run_time=1.4)
        caption = text("In steady flow, water does not build up here", 30, 0, -3.23, MUTED)
        self.add_fixed_in_frame_mobjects(caption)
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
        self.remove(caption)
        inlet_distance = FLOW / (INLET_WIDTH * DEPTH)
        throat_distance = FLOW / (THROAT_WIDTH * DEPTH)
        wide = text(f"{INLET_WIDTH:g} × 1 × {inlet_distance:.2f} = {FLOW:g} m³", 31, -3.53, -2.62, UPSTREAM)
        relation = "=" if round(throat_distance, 2) == throat_distance else "≈"
        narrow = text(f"{THROAT_WIDTH:g} × 1 × {throat_distance:.2f} {relation} {FLOW:g} m³",
                      31, 3.53, -2.62, THROAT)
        factors = text("Width × depth × distance", 29, 0, -3.27, MUTED)
        self.add_fixed_in_frame_mobjects(wide, narrow, factors)
        self.wait(2.4)

        self.next_section("Explain the stretch")
        self.remove(factors)
        self.add_fixed_in_frame_mobjects(
            text("Same volume per second · higher speed", 30, 0, -3.27, INK),
        )
        # Mark the two travel distances from the same camera angle.
        for x, distance in ((inlet_x, inlet_distance), (throat_x, throat_distance)):
            z = DEPTH + 0.12
            self.add_world_mobjects(
                Mesh3D.line3d((x, 0, z), (x + distance, 0, z), thickness=0.035, color=INK),
            )
        self.wait(4.4)
