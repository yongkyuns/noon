"""Synchronous JSPI continuation for canonical FadeIn/FadeOut lifecycle."""

from noon import Circle, Color, FadeIn, FadeOut, Scene, linear


class OrdinaryFadeSynchronousContinuation(Scene):
    # Explicit while Pyodide JSPI/run_sync remains experimental. Ordinary
    # synchronous constructs retain endpoint-only behavior by default.
    realtime_continuation = True

    def construct(self):
        circle = Circle(radius=0.4).set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)

        self.play(FadeIn(circle), run_time=1.0, rate_func=linear)
        first_id = circle.id
        assert self.time == 1.0
        assert circle._scene is self

        self.play(FadeOut(circle), run_time=1.0, rate_func=linear)
        assert self.time == 2.0
        assert circle._scene is None

        # Keep the same detached semantic handle absent for a coherent frame.
        # This wait is driven by the retained Rust session, not a Python clock.
        self.wait(0.25)
        assert self.time == 2.25
        assert circle._scene is None

        # Re-enter the exact handle through Rust LiveSession.add. The wrapper's
        # ObjectId is derived bookkeeping and must remain stable.
        self.add(circle)
        assert circle.id == first_id
        assert circle._scene is self

        # A zero-duration canonical wait reattaches the existing renderer lease
        # and presents the re-entry without adding authored time.
        self.wait(0.0)
        assert self.time == 2.25
