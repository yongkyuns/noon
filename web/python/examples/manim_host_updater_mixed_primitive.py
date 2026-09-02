from noon import *


class MixedPrimitiveRotationUpdater(Scene):
    def construct(self):
        # Keep a different primitive ahead of the diagnostic target. The target
        # is frame object 2 but line instance 1, exercising the index domains
        # independently.
        marker = Circle(radius=0.35).set_color(BLUE)
        line_reference = Line(ORIGIN, LEFT).set_color(WHITE)
        line_moving = Line(ORIGIN, LEFT).set_color(YELLOW)

        def updater_forth(mobj, dt):
            mobj.rotate_about_origin(dt)

        def updater_back(mobj, dt):
            mobj.rotate_about_origin(-dt)

        line_moving.add_updater(updater_forth)
        self.add(marker, line_reference, line_moving)
        self.wait(2)
        line_moving.remove_updater(updater_forth)
        line_moving.add_updater(updater_back)
        self.wait(2)
        line_moving.remove_updater(updater_back)
        self.wait(0.5)


result = MixedPrimitiveRotationUpdater()
result.setup()
try:
    result.construct()
finally:
    result.tear_down()
