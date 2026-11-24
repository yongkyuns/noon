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

        self.live_execution()
        self.remove(toggled)
        self.add(toggled)
        self.add(appended)
        self.set_translation(appended, 2.0, -1.0)

        assert self.effective_translation(anchor) == (0.0, 0.0)
        assert self.effective_translation(appended) == (2.0, -1.0)
