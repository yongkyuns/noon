"""Native CPython entry point for Noon's finite, sampled authoring profile.

The scene source is shared with Pyodide. Execution-only builds are renderer-free;
--output uses the optional native export feature, never a realtime window. Optional text/resources,
spatial/family construction and callback structural/content producers are outside
this binding's initial profile; they are not silently replaced or emulated.
"""
from __future__ import annotations

from _noon_native_host import run_scene, run_source, close_scene, export_scene, export_source


def main():
    # One CLI parser/resolver, not a native-only set of render option meanings.
    from _noon_render_cli import main as render_main
    return render_main()


if __name__ == "__main__":
    raise SystemExit(main())
