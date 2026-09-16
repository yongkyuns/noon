"""Small image-only oracle, executed with ManimCE 0.21.0, not a mock."""
import json
import sys
from pathlib import Path
import manim
import numpy as np
from PIL import Image
from manim import Camera, ImageMobject, Square, tempconfig

assert manim.__version__ == "0.21.0", manim.__version__
output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=True)
pixels = np.array([[[255, 0, 0, 255], [0, 255, 0, 128]],
                   [[0, 0, 255, 0], [255, 255, 255, 255]]], dtype=np.uint8)
Image.fromarray(pixels).save(output / "fixture.png")
Image.fromarray(pixels[:, :, :3]).save(output / "fixture.jpg", quality=95)
report = {"manim": manim.__version__, "samples": []}
with tempconfig({"pixel_width": 256, "pixel_height": 256, "frame_width": 8.0,
                 "frame_height": 8.0, "background_color": "#14283c"}):
    intrinsic = ImageMobject(pixels)
    report["intrinsic"] = {"width": float(intrinsic.width), "height": float(intrinsic.height)}
    for sampler in ["nearest", "bilinear", "bicubic"]:
        for opacity in [1.0, 0.5]:
            camera = Camera()
            image = ImageMobject(pixels).set(height=4)
            image.set_resampling_algorithm(getattr(Image.Resampling, sampler.upper()))
            image.set_opacity(opacity)
            camera.capture_mobjects([image])
            name = f"{sampler}-{opacity}.png"
            camera.get_image().save(output / name)
            report["samples"].append({"name": name, "sampler": sampler, "opacity": opacity})
    for time in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5]:
        camera = Camera()
        image = ImageMobject(pixels).set(height=2)
        image.set_resampling_algorithm(Image.Resampling.NEAREST)
        image.shift([-2, 0, 0])
        objects = []
        if time < 1:
            image.set_opacity(time)
        elif time < 2:
            alpha = time - 1
            image.move_to([-2 + 2 * alpha, 0, 0])
            image.rotate(np.pi / 4 * alpha).scale(1 - 0.25 * alpha)
            image.set_opacity(1 - 0.4 * alpha)
        else:
            image.move_to([0, 0, 0]).rotate(np.pi / 4).scale(0.75)
            image.set_opacity(0.6 * max(0, 3 - time))
        if time < 3:
            objects.append(image)
        if time >= 1:
            second = ImageMobject(pixels).set(height=1)
            second.set_resampling_algorithm(Image.Resampling.NEAREST)
            second.shift([2, 0, 0])
            objects.append(second)
        camera.capture_mobjects(objects)
        camera.get_image().save(output / f"lifecycle-{time}.png")
(output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
