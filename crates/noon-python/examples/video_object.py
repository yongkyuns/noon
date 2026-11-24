"""Place and animate a local video on Noon's finite scene timeline."""

import os
from pathlib import Path

from noon import RIGHT, Scene, linear

result = Scene()
video = result.video(Path(os.environ["NOON_VIDEO_SOURCE"]), height=6.0)
translation = result.value_tracker(0)
result.bind_position(video, translation, direction=RIGHT)
result.declare_value(translation, 1.5, run_time=2.0, rate_func=linear)
