import sys
import types
import unittest
from unittest.mock import patch
import _manim_angle as angle

class FakeLine(angle._compat.Line):
    def __init__(self, start, end): self.endpoints = [angle._base.Vec2(*start), angle._base.Vec2(*end)]
    def get_start(self): return self.endpoints[0]
    def get_end(self): return self.endpoints[1]

class AngleAdapters(unittest.TestCase):
    def test_from_three_points_preserves_pinned_line_order_and_angle_type(self):
        points = (object(), object(), object())
        line_calls = []
        angle_calls = []

        def make_line(start, end):
            line = object()
            line_calls.append((start, end, line))
            return line

        def initialize(self, line1, line2, **kwargs):
            angle_calls.append((type(self), line1, line2, kwargs))

        with patch.object(angle._compat, "Line", side_effect=make_line), \
             patch.object(angle.Angle, "__init__", new=initialize):
            result = angle.Angle.from_three_points(*points, radius=0.5, other_angle=True)
            inherited = angle.RightAngle.from_three_points(*points, length=0.4)

        self.assertIs(type(result), angle.Angle)
        self.assertIs(type(inherited), angle.Angle)
        self.assertEqual([(start, end) for start, end, _ in line_calls],
                         [(points[1], points[0]), (points[1], points[2])] * 2)
        self.assertEqual(angle_calls[0],
                         (angle.Angle, line_calls[0][2], line_calls[1][2],
                          {"radius": 0.5, "other_angle": True}))
        self.assertEqual(angle_calls[1],
                         (angle.Angle, line_calls[2][2], line_calls[3][2],
                          {"length": 0.4}))

    def test_defaults_and_right_angle_only_coerce_and_delegate(self):
        calls=[]
        ffi=types.ModuleType("pyodide.ffi");ffi.to_js=lambda values:values
        options=types.SimpleNamespace(angle=lambda *args:calls.append(args) or object())
        first=FakeLine((-1,0),(1,0));second=FakeLine((0,-1),(0,1))
        with patch.dict(sys.modules,{"pyodide.ffi":ffi}), \
             patch.object(angle,"_require_geometry_host"), \
             patch.object(angle,"_finish_candidate"), \
             patch.object(angle._shared,"_geometry_options",options):
            angle.Angle(first,second)
            angle.RightAngle(first,second,length=0.5,quadrant=(-1,1))
        self.assertEqual(calls[0],([-1.,0.,1.,0.,0.,-1.,0.,1.],None,1.,1.,False,False,9))
        self.assertEqual(calls[1][1:],(0.5,-1.,1.,False,True,9))

    def test_value_is_observed_from_rust_metadata(self):
        calls=[]
        handle=types.SimpleNamespace(angleValue=lambda degrees:calls.append(degrees) or (90 if degrees else 1.5707963267948966))
        obj=object.__new__(angle.Angle)
        with patch.object(angle._shared,"_handle_for",return_value=handle):
            self.assertEqual(obj.get_value(True),90.)
            self.assertEqual(obj.angle_value,1.5707963267948966)
        self.assertEqual(calls,[True,False])

    def test_dot_profile_is_rejected_before_geometry_or_line_observation(self):
        first=FakeLine((-1,0),(1,0));second=FakeLine((0,-1),(0,1))
        with patch.object(angle,"_require_geometry_host"), patch.object(first,"get_start",side_effect=AssertionError("observed before guard")):
            with self.assertRaises(NotImplementedError):angle.Angle(first,second,dot=True)
            with self.assertRaises(TypeError):angle.Angle(object(),second)

if __name__=="__main__":unittest.main()
