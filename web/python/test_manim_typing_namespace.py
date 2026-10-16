import subprocess
import sys
from pathlib import Path
import unittest

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_typing as aliases
import noon


class ManimTypingNamespace(unittest.TestCase):
    def test_cold_public_alias_and_star_import_do_not_load_numpy(self):
        source = "import sys; import noon; from noon import *; assert noon.typing is typing; assert 'numpy' not in sys.modules; assert 'Point3DLike' in dir(typing); assert 'Point3DLike' not in noon.__all__"
        subprocess.run([sys.executable, "-S", "-c", source], cwd=Path(__file__).parent, check=True)

    def test_host_discovers_numpy_for_direct_aliased_and_module_type_access(self):
        for source in ["from manim import typing as mt\nvalue: mt.Point3DLike", "from noon import typing\nvalue: typing.Point3DLike", "import noon\nvalue: noon.typing.Point3DLike"]:
            self.assertEqual(namespace.required_packages(source), ("numpy",))
        self.assertEqual(namespace.required_packages("message = 'typing.Point3DLike'"), ())
        self.assertEqual(namespace.required_packages("from noon import *\nCircle()"), ())

    def test_unknown_names_and_export_boundary(self):
        self.assertIs(noon.typing, aliases)
        self.assertIn("typing", noon.__all__)
        self.assertEqual(len(aliases.__all__), 87)
        with self.assertRaises(AttributeError):
            aliases.NotAManimType

    @unittest.skipIf(np is None, "NumPy alias contracts run in the pinned utility lane")
    def test_alias_identity_dtype_and_path_protocols(self):
        from os import PathLike
        self.assertIs(aliases.ManimFloat, np.float64)
        self.assertIs(aliases.ManimInt, np.int64)
        self.assertIs(aliases.PointDType, aliases.ManimFloat)
        self.assertIs(aliases.Zeros, aliases.MatrixMN)
        self.assertIs(aliases.CubicBezierPoints, aliases.Point3D_Array)
        self.assertIs(aliases.RGBAPixelArray, aliases.PixelArray)
        import numpy.typing as npt
        self.assertEqual(aliases.Point3D, npt.NDArray[np.float64])
        self.assertEqual(aliases.StrPath, str | PathLike[str])
        self.assertEqual(aliases.ManimTextLabel, "Text | MathTex | Typst")
        self.assertIs(aliases.Point3D, aliases.Point3D)


if __name__ == "__main__":
    unittest.main()
