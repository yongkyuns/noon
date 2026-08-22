from noon import Color, FadeIn, FadeOut, Scene
from noon_layout import arrange

scene = Scene()

# The left circle is a visual reference. The right circle has semantic opacity
# 0.42; Fade only modulates the independent Appearance channel.
reference_slot, fading_slot = arrange(2, spacing=2.4)
COLOR = Color(0.26, 0.82, 0.92)
OUTLINE = Color(0.82, 0.96, 1.0)

scene.circle(
    0.65,
    position=reference_slot,
    fill=COLOR,
    stroke=OUTLINE,
    stroke_width=0.07,
    key="reference",
)
fading = scene.circle(
    0.65,
    position=fading_slot,
    fill=COLOR,
    stroke=OUTLINE,
    stroke_width=0.07,
    opacity=0.42,
    key="fading",
)

scene.play(
    FadeOut(fading, key="fading.out"),
    duration=0.8,
    start_time=0.55,
    easing="ease_in_out_cubic",
)
scene.play(
    FadeIn(fading, key="fading.in"),
    duration=0.8,
    start_time=2.0,
    easing="ease_in_out_cubic",
)

result = scene
