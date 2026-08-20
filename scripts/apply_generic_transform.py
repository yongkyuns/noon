import runpy

runpy.run_path("scripts/fix_generic_transform_python_tests.py", run_name="__main__")
runpy.run_path("scripts/apply_generic_transform_review_fixes.py", run_name="__main__")
runpy.run_path("scripts/fix_render_geometry_fixtures.py", run_name="__main__")
