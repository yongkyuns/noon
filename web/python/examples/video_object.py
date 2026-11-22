"""Move the selected MP4 using the same tracker as the native Rust example."""

from noon import RIGHT, Scene, VideoSource, linear


class VideoObject(Scene):
    async def construct(self):
        video = self.video(await VideoSource.from_upload(), height=6)
        translation = self.value_tracker(0)
        self.bind_position(video, translation, direction=RIGHT)
        self.declare_value(translation, 1.5, run_time=2, rate_func=linear)
