"""Move the selected MP4 using the same tracker as the native Rust example."""

from noon import RIGHT, Scene, VideoSource, linear

result = Scene()
clip = await VideoSource.from_upload()
video = result.video(clip, height=6)
translation = result.value_tracker(0)
result.bind_position(video, translation, direction=RIGHT)
result.declare_value(translation, 1.5, run_time=2, rate_func=linear)
