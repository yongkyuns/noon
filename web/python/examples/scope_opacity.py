"""Final composed opacity paired with noon::example_scenes::scope_opacity."""
from noon import Blur, MovingCameraScene, Pixels, Rectangle, VGroup


class FamilyOpacity(MovingCameraScene):
    def construct(self):
        self.camera.frame.scale(0.75)
        witness = Rectangle(width=0.6, height=0.6, fill="#ffffff", fill_opacity=1, stroke=None).shift((-4, -2, 0))
        red = Rectangle(width=2, height=2, fill="#ff0000", fill_opacity=1, stroke=None).shift((-0.5, 0, 0))
        blue = Rectangle(width=2, height=2, fill="#0000ff", fill_opacity=0.5, stroke=None).shift((0.5, 0, 0))
        group = VGroup(red, blue).set_object_opacity(0.5)
        parent = VGroup(group).set_object_opacity(0.6)
        self.add(witness, parent)
        self.wait(0.5)
        group.set_object_opacity(0.25)
        self.wait(0.5)
        group.set_object_opacity(0)
        self.wait(0.5)
        group.set_object_opacity(1)
        self.wait(0.5)
        group.add_effect(Blur(radius=Pixels(0)), name="soften", scope="composed")
        self.wait(0.5)
        group.set_object_opacity(0.5)
        self.wait(0.5)
        group.set_effect("soften", radius=Pixels(1))
        self.wait(0.5)
        group.remove_effect("soften")
        self.wait(0.5)
        parent.set_object_opacity(1)
        self.wait(0.5)
        group.set_object_opacity(1)
        self.wait(0.5)
