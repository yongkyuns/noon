"""Complex coordinates over the shared Rust NumberPlane frame."""

from _manim_number_plane import NumberPlane


class ComplexPlane(NumberPlane):
    """A NumberPlane with Manim-compatible complex coordinate queries."""

    def number_to_point(self, number):
        value = complex(number)
        return self.coords_to_point(value.real, value.imag)

    n2p = number_to_point

    def point_to_number(self, point):
        value = self.point_to_coords(point)
        return complex(value.x, value.y)

    p2n = point_to_number


__all__ = ["ComplexPlane"]
