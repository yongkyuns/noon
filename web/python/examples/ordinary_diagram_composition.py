"""M0 diagram composition using ordinary shared layout and transforms.

Paired with noon::example_scenes::diagram_composition. Flowing strokes are
tracked separately in #2188; this scene establishes the existing foundation.
"""

from noon import *


class OrdinaryDiagramComposition(Scene):
    def construct(self):
        imu_box = Rectangle(width=1.6, height=0.8)
        imu_box.set_fill(BLUE, opacity=0.2).set_stroke(BLUE, width=3)
        imu_label = Text("IMU", font_size=24, color=WHITE).move_to(imu_box)
        imu = VGroup(imu_box, imu_label)

        ekf_box = Rectangle(width=1.6, height=0.8)
        ekf_box.set_fill(ORANGE, opacity=0.2).set_stroke(ORANGE, width=3)
        ekf_label = Text("EKF", font_size=24, color=WHITE).move_to(ekf_box)
        ekf = VGroup(ekf_box, ekf_label)

        # Arrange nodes before adding their connections to the assembly.
        nodes = VGroup(imu, ekf).arrange(RIGHT, buff=1.2)
        edge = Arrow(
            imu_box.get_right(), ekf_box.get_left(),
            buff=0, tip_length=0.2, stroke_width=3, color=WHITE,
        )
        assembly = VGroup(nodes, edge)
        self.add(assembly)
        self.wait(0.25)

        # A common group transform carries both nodes and the internal arrow.
        self.play(assembly.animate.shift(UP * 0.75), run_time=1, rate_func=linear)
        assert abs(imu_box.get_center().x + 1.4) < 1e-5
        assert abs(ekf_box.get_center().x - 1.4) < 1e-5
        assert abs(imu_box.get_center().y - 0.75) < 1e-5
        assert abs(ekf_box.get_center().y - 0.75) < 1e-5
        self.wait(0.25)

        # Prepare one coherent ordinary target. The node and its connection
        # change together; the other node keeps its position. Copying labels
        # preserves their existing retained text resources.
        target_nodes = nodes.copy()
        target_nodes[1].shift(RIGHT)
        target_edge = Arrow(
            target_nodes[0][0].get_right(), target_nodes[1][0].get_left(),
            buff=0, tip_length=0.2, stroke_width=3, color=WHITE,
        )
        target = VGroup(target_nodes, target_edge)
        self.play(Transform(assembly, target), run_time=1, rate_func=linear)

        assert abs(imu_box.get_center().x + 1.4) < 1e-5
        assert abs(ekf_box.get_center().x - 2.4) < 1e-5
        assert abs(imu_label.get_center().x - imu_box.get_center().x) < 1e-5
        assert abs(ekf_label.get_center().x - ekf_box.get_center().x) < 1e-5
        assert abs(edge.get_start().x - imu_box.get_right().x) < 1e-5
        assert abs(edge.get_end().x - ekf_box.get_left().x) < 1e-5
        self.wait(0.5)
