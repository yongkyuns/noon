from noon import *

class PointerSelectionGallery(MovingCameraScene):
    def construct(self):
        self.configure_pointer_interactions()

        circle = Circle(radius=1.15, color=BLUE)
        circle.set_fill(BLUE, opacity=0.72)
        circle.shift(LEFT * 2)

        rectangle = Rectangle(width=2.4, height=1.8, color=GREEN)
        rectangle.set_fill(GREEN, opacity=0.72)
        rectangle.shift(RIGHT * 2)

        self.add(circle, rectangle)
