"""Qualify bounded point-free Mobject constructors against complete pinned Manim."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import sys

def observations(api):
    def color_hex(color):
        if hasattr(color,'to_hex'):return color.to_hex().upper()
        return '#' + ''.join(f'{round(channel*255):02X}' for channel in (color.red,color.green,color.blue))
    rows=[]
    for options in ({},{'color':'#FC6255','name':'named','z_index':3},{'dim':2}):
        value=api.Mobject(**options)
        for stage in ('initial','shift','scale'):
            if stage=='shift':value.shift(api.RIGHT)
            elif stage=='scale':value.scale(2)
            rows.append([options,stage,type(value).__name__,value.name,value.dim,value.target is None,
                         [float(x) for x in value.get_center()][:2],float(value.width),float(value.height),
                         float(value.z_index),color_hex(value.get_color())])
    return rows


def qualify(source_root, *, native_package=None):
    import manim
    from manim.mobject import mobject
    source = source_root / "manim/mobject/mobject.py"
    if Path(mobject.__file__).read_bytes() != source.read_bytes():
        raise ValueError("installed complete Mobject module differs from pinned source")
    expected = observations(manim)
    result = {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
              "complete_original_observations": len(expected), "native_profiles": [], "errors": []}
    if native_package is None:
        return result
    sys.path.insert(0, str(native_package.resolve()))
    import asyncio
    from noon_native import close_scene, run_source
    candidate_source = inspect.getsource(observations) + "\nfrom noon import *\nimport noon\nclass Probe(Scene):\n    def construct(self):\n        self.observations=observations(noon)\n        self.add(Square())\n        self.wait(.2)\n"
    for portable in (False, True):
        scene = asyncio.run(run_source(candidate_source, portable=portable, sample_hz=5))
        try:
            if scene.observations != expected:
                result["errors"].append(f"native constructor observations differ: portable={portable}")
            result["native_profiles"].append({"portable": portable, "observations": len(scene.observations),
                                               "time": scene.time})
        finally:
            close_scene(scene)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("--native-package", type=Path)
    args = parser.parse_args()
    result = qualify(args.source_root, native_package=args.native_package)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
