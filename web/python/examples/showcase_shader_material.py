"""The shared Rust shader_sweep lesson through ordinary Python authoring.

One immutable program per coordinate schema, named effect attachments, ordinary
parameter/motion animation, and explicit removal. No per-frame Python callback.
"""
from noon import *


SOURCE = """
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let stripes = 0.5 + 0.5 * cos(6.28318530718 * (input.position.y * p.frequency + p.phase));
    return vec4<f32>(input.color.rgb * mix(p.low.rgb, p.high.rgb, stripes), 1.0);
}
"""


def material(coordinates):
    return WgslEffectProgram(SOURCE, coordinates=coordinates, parameters=[
        ShaderParameter.scalar("phase", default=0, minimum=0, maximum=1),
        ShaderParameter.scalar("frequency", default=2, minimum=0.25, maximum=8),
        ShaderParameter.color("low", default=Color(0.09, 0.2, 0.42)),
        ShaderParameter.color("high", default=Color(0.48, 0.94, 0.83)),
    ])


class ShaderMaterial(Scene):
    def construct(self):
        self.add(Rectangle(width=16, height=9, fill=Color(0.035, 0.045, 0.09),
                           fill_opacity=1, stroke=None))
        panels = [Rectangle(width=4.8, height=2.5, fill=WHITE, fill_opacity=1,
                            stroke=None).move_to((x, 0, 0)) for x in (-3, 3)]
        self.add(*panels)
        for text, size, x, y in [
            ("A MATERIAL THAT MOVES", 36, 0, 3.1),
            ("Animate the controls. Keep the objects editable.", 20, 0, 2.5),
            ("LOCAL: follows the object", 18, -3, -2),
            ("SCENE: stays in the world", 18, 3, -2),
        ]:
            self.add(Text(text, font_size=size, color=Color(0.85, 0.9, 0.96))
                     .move_to((x, y, 0)))
        programs = [material("local"), material("scene")]
        phase = [program.parameter("phase") for program in programs]
        self.wait(0.5)
        for panel, program in zip(panels, programs):
            panel.add_effect(ShaderEffect(program), name="stripes")
        self.play(
            *(Rotate(panel, angle=0.35) for panel in panels),
            *(panel.animate.set_effect("stripes", parameters={key: 0.65})
              for panel, key in zip(panels, phase)),
            run_time=2, rate_func=smooth,
        )
        self.wait(0.5)
        self.play(
            *(Rotate(panel, angle=-0.35) for panel in panels),
            *(panel.animate.set_effect("stripes", parameters={key: 0})
              for panel, key in zip(panels, phase)),
            run_time=2, rate_func=smooth,
        )
        self.wait(0.5)
        for panel in panels:
            panel.remove_effect("stripes")
        self.wait(0.5)
