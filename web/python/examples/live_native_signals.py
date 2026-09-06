"""Canonical native-input declarations over one shared Rust session.

The native host owns only normalized event delivery. Signal identity, input routing,
property bindings, and effective state remain in the SemanticStore and its single
ExecutionSession.
"""

from noon import Color, Scene, Square


class LiveNativeSignals(Scene):
    def construct(self):
        # Keep Rust's canonical square defaults (white screen-space miter/butt
        # stroke) and make only the same opaque blue fill edit as its example.
        square = Square(side_length=0.9)
        square.set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)
        self.add(square)

        pointer = self.pointer_position_signal()
        self.bind_position(square, pointer)

        opacity = self.control_signal("opacity", 1.0)
        self.bind_opacity(square, opacity)

        clicks = self.pointer_down_events(0)
        self.bind_rotation(square, clicks)

        visible = self.key_state_signal("Space", False)
        self.bind_presence(square, visible)

        # These normalized sources are valid unbound runtime no-ops.
        self.viewport_size_signal()
        self.wheel_delta_signal()
        self.wheel_events()
        self.control_commit_events("opacity")

        live = self.live_execution()
        live.evaluate(0.0)
        assert opacity.get_value() == 1.0
        assert clicks.get_value() == 0.0
