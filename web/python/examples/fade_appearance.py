from noon import BLUE, WHITE, RIGHT, Circle, FadeIn, FadeOut, Scene, VGroup

scene = Scene()

# The left circle is a visual reference. The right circle has semantic opacity
# 0.42; Fade only modulates the independent Appearance channel.
reference = Circle(0.65, color=BLUE).set_stroke(WHITE, 0.07)
fading = Circle(0.65, color=BLUE).set_stroke(WHITE, 0.07).set_opacity(0.42)
VGroup(reference, fading).arrange(RIGHT, buff=1.1)

scene.add(reference, key="reference")
scene.add(fading, key="fading")

scene.wait(0.4)
scene.play(FadeOut(fading, key="fading.out"), run_time=0.8, easing="ease_in_out_cubic")
scene.wait(0.5)
scene.play(FadeIn(fading, key="fading.in"), run_time=0.8, easing="ease_in_out_cubic")

result = scene
