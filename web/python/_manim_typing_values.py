"""ManimCE 0.21.0 Python type aliases; no scene or renderer state.

Pinned to ManimCommunity/manim@861cd4849b17db1db3515b531ffe80b297848f93.
Only host type expressions are represented here. Array shapes documented by
upstream are not additional runtime validators.
"""
from collections.abc import Callable, Sequence
from os import PathLike
import numpy as np
import numpy.typing as npt

ManimFloat = np.float64
ManimInt = np.int64
ManimColorDType = ManimFloat
FloatRGB = npt.NDArray[ManimColorDType]
FloatRGBLike = FloatRGB | tuple[float, float, float]
FloatRGB_Array = npt.NDArray[ManimColorDType]
FloatRGBLike_Array = FloatRGB_Array | Sequence[FloatRGBLike]
IntRGB = npt.NDArray[ManimInt]
IntRGBLike = IntRGB | tuple[int, int, int]
FloatRGBA = npt.NDArray[ManimColorDType]
FloatRGBALike = FloatRGBA | tuple[float, float, float, float]
FloatRGBA_Array = npt.NDArray[ManimColorDType]
FloatRGBALike_Array = FloatRGBA_Array | Sequence[FloatRGBALike]
IntRGBA = npt.NDArray[ManimInt]
IntRGBALike = IntRGBA | tuple[int, int, int, int]
FloatHSV = FloatRGB
FloatHSVLike = FloatRGBLike
FloatHSVA = FloatRGBA
FloatHSVALike = FloatRGBALike
FloatHSL = FloatRGB
FloatHSLLike = FloatRGBLike
ManimColorInternal = FloatRGBA
PointDType = ManimFloat
Point2D = npt.NDArray[PointDType]
Point2DLike = Point2D | tuple[float, float]
Point2D_Array = npt.NDArray[PointDType]
Point2DLike_Array = Point2D_Array | Sequence[Point2DLike]
Point3D = npt.NDArray[PointDType]
Point3DLike = Point3D | tuple[float, float, float]
Point3D_Array = npt.NDArray[PointDType]
Point3DLike_Array = Point3D_Array | Sequence[Point3DLike]
PointND = npt.NDArray[PointDType]
PointNDLike = PointND | Sequence[float]
PointND_Array = npt.NDArray[PointDType]
PointNDLike_Array = PointND_Array | Sequence[PointNDLike]
Vector2D = npt.NDArray[PointDType]
Vector2DLike = npt.NDArray[PointDType] | tuple[float, float]
Vector2D_Array = npt.NDArray[PointDType]
Vector2DLike_Array = Vector2D_Array | Sequence[Vector2DLike]
Vector3D = npt.NDArray[PointDType]
Vector3DLike = npt.NDArray[PointDType] | tuple[float, float, float]
Vector3D_Array = npt.NDArray[PointDType]
Vector3DLike_Array = npt.NDArray[PointDType] | Sequence[Vector3DLike]
VectorND = npt.NDArray[PointDType]
VectorNDLike = npt.NDArray[PointDType] | Sequence[float]
VectorND_Array = npt.NDArray[PointDType]
VectorNDLike_Array = npt.NDArray[PointDType] | Sequence[VectorNDLike]
RowVector = npt.NDArray[PointDType]
ColVector = npt.NDArray[PointDType]
MatrixMN = npt.NDArray[PointDType]
Zeros = MatrixMN
QuadraticBezierPoints = Point3D_Array
QuadraticBezierPointsLike = QuadraticBezierPoints | tuple[Point3DLike, Point3DLike, Point3DLike]
QuadraticBezierPoints_Array = npt.NDArray[PointDType]
QuadraticBezierPointsLike_Array = QuadraticBezierPoints_Array | Sequence[QuadraticBezierPointsLike]
QuadraticBezierPath = Point3D_Array
QuadraticBezierPathLike = Point3DLike_Array
QuadraticSpline = QuadraticBezierPath
QuadraticSplineLike = QuadraticBezierPathLike
CubicBezierPoints = Point3D_Array
CubicBezierPointsLike = CubicBezierPoints | tuple[Point3DLike, Point3DLike, Point3DLike, Point3DLike]
CubicBezierPoints_Array = npt.NDArray[PointDType]
CubicBezierPointsLike_Array = CubicBezierPoints_Array | Sequence[CubicBezierPointsLike]
CubicBezierPath = Point3D_Array
CubicBezierPathLike = Point3DLike_Array
CubicSpline = CubicBezierPath
CubicSplineLike = CubicBezierPathLike
BezierPoints = Point3D_Array
BezierPointsLike = Point3DLike_Array
BezierPoints_Array = npt.NDArray[PointDType]
BezierPointsLike_Array = BezierPoints_Array | Sequence[BezierPointsLike]
BezierPath = Point3D_Array
BezierPathLike = Point3DLike_Array
Spline = BezierPath
SplineLike = BezierPathLike
FlatBezierPoints = npt.NDArray[PointDType] | tuple[float, ...]
FunctionOverride = Callable
PathFuncType = Callable[[Point3DLike, Point3DLike, float], Point3DLike]
MappingFunction = Callable[[Point3D], Point3D]
MultiMappingFunction = Callable[[Point3D_Array], Point3D_Array]
ManimTextLabel = 'Text | MathTex | Typst'
PixelArray = npt.NDArray[ManimInt]
GrayscalePixelArray = PixelArray
RGBPixelArray = PixelArray
RGBAPixelArray = PixelArray
StrPath = str | PathLike[str]
StrOrBytesPath = str | bytes | PathLike[str] | PathLike[bytes]
