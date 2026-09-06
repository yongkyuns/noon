"""Paired Python proof for typed live membership and property publication.

Run in the Noon browser/Pyodide authoring environment.  This calls the real
canonical Rust/WASM context and its live execution player; it does not encode a
scene document or synchronize a Python state copy.
"""

from noon import Circle, Scene, Square


class LiveSemanticScene(Scene):
    def construct(self):
        anchor = Circle(0.5)
        toggled = Circle(1.0)
        appended = Square(1.5)
        self.add(anchor)
        self.add(toggled)

        live = self.live_execution()
        live.remove(toggled)
        live.add(toggled)
        live.add(appended)
        live.set_translation(appended, 2.0, -1.0)

        assert live.effective_center(anchor) == (0.0, 0.0)
        assert live.effective_center(appended) == (2.0, -1.0)
