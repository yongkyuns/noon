"""Paired Python proof for the typed live property/query path.

Run in the Noon browser/Pyodide authoring environment.  This calls the real
canonical Rust/WASM context and its live execution player; it does not encode a
scene document or synchronize a Python state copy.
"""

from noon import Circle, Scene


class LiveSemanticScene(Scene):
    def construct(self):
        circle = Circle(1.0)
        self.add(circle)
        live = self.live_execution()
        live.set_translation(circle, 2.0, -1.0)
        live.set_scale(circle, 1.5, 0.5)
        assert live.effective_center(circle) == (2.0, -1.0)
