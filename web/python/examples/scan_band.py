"""External ScanBand: ordinary typed material parameters and a seekable animation."""
from noon import *

SOURCE = r"""
// Exact box coverage of a vertical band in affine local coordinates.
fn scan_cdf(t: f32, gradient: vec2<f32>) -> f32 {
    let a = max(abs(gradient.x), abs(gradient.y));
    let b = min(abs(gradient.x), abs(gradient.y));
    if a == 0.0 { return select(0.0, 1.0, t >= 0.0); }
    if b == 0.0 { return clamp(t / a + 0.5, 0.0, 1.0); }
    let s = clamp(t + 0.5 * (a + b), 0.0, a + b);
    if s < b { return 0.5 * s * s / (a * b); }
    if s <= a { return (s - 0.5 * b) / a; }
    let tail = a + b - s;
    return 1.0 - 0.5 * tail * tail / (a * b);
}
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let gradient = vec2<f32>(dpdx(input.position.x), dpdy(input.position.x));
    let offset = p.phase - input.position.x;
    let coverage = scan_cdf(offset + 0.5 * p.width, gradient)
                 - scan_cdf(offset - 0.5 * p.width, gradient);
    return vec4<f32>(1.0, 1.0, 0.0, clamp(coverage, 0.0, 1.0));
}
"""


class ScanBand(Scene):
    def construct(self):
        program = WgslEffectProgram(SOURCE, parameters=[
            ShaderParameter.scalar("width", default=0.2, minimum=2**-149,
                                   maximum=1, unit="normalized"),
            ShaderParameter.scalar("phase", default=0, minimum=0,
                                   maximum=1, unit="normalized"),
        ])
        panel = Rectangle(width=4, height=2, fill=WHITE,
                          fill_opacity=0.5, stroke=None).rotate(0.37)
        panel.add_effect(ShaderEffect(program), name="scan")
        self.add(panel)
        self.play(panel.animate.set_effect("scan", parameters={program.parameter("phase"): 1}),
                  run_time=2, rate_func=linear)
