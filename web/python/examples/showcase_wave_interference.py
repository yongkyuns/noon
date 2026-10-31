"""An analytic wave-field lesson: three persistent surfaces, one shared material.

Constant-amplitude circular waves illustrate superposition, not propagation
physics. One material evaluates the fields per pixel; ordinary tracks own phase
without a Python updater or a grid of animated cells.
"""
from noon import *


FIELD = """
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let a = cos(44.0 * length(input.position - vec2<f32>(0.28, 0.5)) - p.phase);
    let b = cos(44.0 * length(input.position - vec2<f32>(0.72, 0.5)) - p.phase + p.offset);
    let value = p.weight_a * a + p.weight_b * b;
    let neutral = vec3<f32>(0.93, 0.93, 0.9);
    let negative = vec3<f32>(0.08, 0.42, 0.62);
    let positive = vec3<f32>(0.98, 0.57, 0.12);
    let color = mix(neutral, select(negative, positive, value >= 0.0), abs(value) * 0.5);
    return vec4<f32>(color, 1.0);
}
"""


class WaveInterference(Scene):
    def construct(self):
        ink, paper, muted = color_from_hex("#101c30"), color_from_hex("#eeeae0"), color_from_hex("#9baec4")
        gold = color_from_hex("#f9b541")
        self.add(Rectangle(width=16, height=9, fill=ink, fill_opacity=1, stroke=None))
        self.add(Text("INTERFERENCE IS ADDITION", font_size=34, color=paper).move_to((0, 3.3, 0)))
        self.add(Text("Two point sources. One signed sum.", font_size=20, color=muted).move_to((0, 2.72, 0)))
        program = WgslEffectProgram(FIELD, parameters=[
            ShaderParameter.scalar("phase", default=0, minimum=0, maximum=4 * PI),
            ShaderParameter.scalar("offset", default=0, minimum=0, maximum=PI),
            ShaderParameter.scalar("weight_a", default=1, minimum=0, maximum=1),
            ShaderParameter.scalar("weight_b", default=1, minimum=0, maximum=1),
        ])
        phase, offset = program.parameter("phase"), program.parameter("offset")
        wa, wb = program.parameter("weight_a"), program.parameter("weight_b")
        panels = []
        for x, name, weights in [(-4.8, "SOURCE A", (1, 0)), (0, "SOURCE B", (0, 1)), (4.8, "A + B", (1, 1))]:
            panel = Rectangle(width=3.6, height=3.6, fill=WHITE, fill_opacity=1, stroke=None).move_to((x, 0.15, 0))
            panel.add_effect(ShaderEffect(program, parameters={wa: weights[0], wb: weights[1]}), name="field")
            panels.append(panel)
            self.add(panel, Text(name, font_size=17, color=paper).move_to((x, 2.25, 0)))
        for x, symbol in [(-2.4, "+"), (2.4, "=")]:
            self.add(Text(symbol, font_size=30, color=muted).move_to((x, 0.15, 0)))
        self.add(Text("Same scale in every panel: blue is negative, gold is positive.", font_size=17, color=muted).move_to((0, -2.05, 0)))
        self.add(Text("Opposite signs cancel to zero; matching signs reinforce.", font_size=17, color=muted).move_to((0, -2.43, 0)))
        same = Text("SAME PHASE", font_size=18, color=gold).move_to((-2.15, -3.0, 0))
        opposite = Text("HALF-CYCLE OFFSET", font_size=18, color=muted).move_to((2.15, -3.0, 0))
        legend = Rectangle(width=5.0, height=0.16, stroke=None).set_fill(LinearGradient([
            (0, Color(0.08, 0.42, 0.62)), (0.5, Color(0.93, 0.93, 0.9)), (1, Color(0.98, 0.57, 0.12)),
        ]), opacity=1).move_to((0, -3.5, 0))
        self.add(same, opposite, legend)
        for x, label in [(-2.5, "-2"), (0, "0"), (2.5, "+2")]:
            self.add(Text(label, font_size=14, color=muted).move_to((x, -3.82, 0)))
        self.wait(0.8)
        self.play(*(p.animate.set_effect("field", parameters={phase: 2 * PI}) for p in panels), run_time=2.4, rate_func=linear)
        self.wait(0.6)
        self.play(*(p.animate.set_effect("field", parameters={offset: PI}) for p in panels),
                  same.animate.set_color(muted), opposite.animate.set_color(gold), run_time=1.2, rate_func=smooth)
        self.wait(0.8)
        self.play(*(p.animate.set_effect("field", parameters={phase: 4 * PI}) for p in panels), run_time=2.4, rate_func=linear)
        self.wait(0.6)
        self.play(*(p.animate.set_effect("field", parameters={offset: 0}) for p in panels),
                  same.animate.set_color(gold), opposite.animate.set_color(muted), run_time=1.2, rate_func=smooth)
        self.wait(1)
