"""Canonical authored shader time through an ordinary wait, without callbacks."""
from noon import *


class ShaderTime(Scene):
    def construct(self):
        program = WgslEffectProgram(
            "fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> { return vec4<f32>(input.color.rgb, 0.2 + 0.25 * input.time); }",
            authored_time=True,
        )
        panel = Rectangle(width=4, height=2, fill=Color(0.5, 0.75, 1),
                          fill_opacity=0.5, stroke=None)
        panel.add_effect(ShaderEffect(program), name="clock")
        self.add(panel)
        self.wait(2)
