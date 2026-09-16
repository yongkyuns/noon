"""Dispatch/option ownership only; Rust and actual Pyodide test the semantics."""
from types import SimpleNamespace
from unittest import TestCase, main
from unittest.mock import Mock, patch
import _manim_plotting as plotting

class LiveCoordinateAdapterTests(TestCase):
    def setUp(self):
        self.addCleanup(patch.stopall)
        self.options=Mock()
        self.cold=Mock(return_value=Mock())
        self.live=Mock(return_value=Mock())
        self.context=SimpleNamespace(liveCreateCoordinates=self.live)
        self.resolve=patch.object(plotting._shared,"_live_constructor_context",return_value=self.context).start()
        self.guard=patch.object(plotting,"_outside_callback").start()
        patch.object(plotting,"_to_js",side_effect=lambda x:x).start()
        patch.object(plotting,"_create_coordinates",self.cold).start()
        patch.object(plotting,"_coordinate_options",SimpleNamespace(
            numberLine=Mock(return_value=self.options),axes=Mock(return_value=self.options))).start()
        patch.object(plotting,"engine_call",side_effect=lambda fn,*args:fn(*args)).start()
        patch.object(plotting,"_coordinate_style").start()
        patch.object(plotting,"_attach_number_line",side_effect=lambda wrapper,handle:wrapper).start()
        patch.object(plotting,"_family",side_effect=lambda wrapper,handle,members:wrapper).start()

    def construct(self, axes=False):
        return plotting.Axes((0,2,1),(0,2,1),x_length=2,y_length=2) if axes else plotting.NumberLine((0,2,1))

    def test_live_number_line_captures_owner_once_and_consumes_options(self):
        self.construct()
        self.resolve.assert_called_once_with("coordinates")
        self.live.assert_called_once_with(self.options)
        self.cold.assert_not_called(); self.options.free.assert_not_called()

    def test_live_axes_use_the_same_dispatch_without_a_cold_fallback(self):
        self.construct(True)
        self.resolve.assert_called_once_with("coordinates")
        self.live.assert_called_once_with(self.options)
        self.cold.assert_not_called(); self.options.free.assert_not_called()

    def test_cold_constructor_still_uses_existing_host(self):
        self.resolve.return_value=None
        self.construct()
        self.cold.assert_called_once_with(self.options); self.live.assert_not_called()

    def test_consuming_live_failure_never_retries_cold_or_double_frees(self):
        error=ValueError("stale owner")
        self.live.side_effect=error
        with self.assertRaises(ValueError) as caught:self.construct()
        self.assertIs(caught.exception,error)
        self.cold.assert_not_called(); self.options.free.assert_not_called()

    def test_option_failure_frees_unconsumed_candidate_and_never_publishes(self):
        error=ValueError("bad ticks");self.options.setTicks.side_effect=error
        with self.assertRaises(ValueError) as caught:self.construct()
        self.assertIs(caught.exception,error)
        self.options.free.assert_called_once_with(); self.live.assert_not_called();self.cold.assert_not_called()

    def test_callback_guard_runs_before_context_or_options(self):
        self.guard.side_effect=NotImplementedError("unpinned callback")
        with self.assertRaises(NotImplementedError):self.construct()
        self.resolve.assert_not_called(); plotting._coordinate_options.numberLine.assert_not_called()

    def test_missing_host_does_not_allocate(self):
        plotting._create_coordinates=None
        with self.assertRaises(RuntimeError):self.construct()
        self.resolve.assert_not_called();plotting._coordinate_options.numberLine.assert_not_called()

    def test_unsupported_tips_do_not_allocate_options(self):
        with self.assertRaises(NotImplementedError):
            plotting.NumberLine((0,2,1), include_tip=True)
        with self.assertRaises(NotImplementedError):
            plotting.Axes((0,2,1),(0,2,1),x_length=2,y_length=2,tips=True)
        plotting._coordinate_options.numberLine.assert_not_called()
        plotting._coordinate_options.axes.assert_not_called()
        self.live.assert_not_called(); self.cold.assert_not_called()

    def test_late_queries_require_binding_but_never_choose_authored_fallback(self):
        shaft=object()
        with patch.object(plotting._shared, "_live_mutation_context", return_value=None), \
             patch.object(plotting._shared, "_is_bound", return_value=False) as bound:
            with self.assertRaisesRegex(NotImplementedError, "add live coordinates"):
                plotting._coordinate_context([shaft])
            bound.return_value=True
            self.assertIs(plotting._coordinate_context([shaft]), self.context)
            self.resolve.return_value=None
            bound.return_value=False
            self.assertIsNone(plotting._coordinate_context([shaft]))

if __name__=="__main__":main()
