from noon import Scene, Square, resolve_render_options, NoonValueError, NoonUnsupportedError
import json

class RenderOptions(Scene):
    async def construct(self):
        observations = []
        for quality, width, height, rate in [
            ("l", 854, 480, (15, 1)), ("m", 1280, 720, (30, 1)),
            ("h", 1920, 1080, (60, 1)), ("p", 2560, 1440, (60, 1)),
            ("k", 3840, 2160, (60, 1)),
        ]:
            value = resolve_render_options(quality=quality)
            if (value.width, value.height, value.fps) != (width, height, rate):
                raise RuntimeError("quality resolver differs from the pinned Manim profile")
            observations.append([value.width, value.height, *value.fps])
        for rate, expected in [(60, (60, 1)), (29.97, (2997, 100)),
                               ("60000/1001", (60000, 1001))]:
            value = resolve_render_options(quality="l", resolution="65,33", frame_rate=rate, format="png")
            if (value.width, value.height, value.fps, value.format) != (65, 33, expected, "png"):
                raise RuntimeError("explicit output settings did not override the quality defaults")
            observations.append([value.width, value.height, *value.fps])
        for options, error_type, code in [
            ({"frame_rate": "nan"}, NoonValueError, "render.frame_rate"),
            ({"format": "gif"}, NoonUnsupportedError, "render.unsupported_format"),
        ]:
            try:
                resolve_render_options(**options)
            except error_type as error:
                if error.code != code:
                    raise RuntimeError("render option error identity changed")
            else:
                raise RuntimeError("invalid configuration silently accepted")
        self.add(Square(1).set_fill("#0000ff", opacity=1).set_stroke(width=0))
        await self.wait(1)
        print("NOON_HOST_REPORT " + json.dumps({"case": "render_options", "observations": observations}))
