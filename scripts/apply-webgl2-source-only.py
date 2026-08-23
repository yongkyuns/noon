from pathlib import Path

source = Path("scripts/apply-webgl2-fallback.py").read_text()
prefix, separator, _ = source.partition("\nfinal_ci = ")
if not separator:
    raise SystemExit("fallback patch script no longer contains the final_ci marker")
exec(compile(prefix, "scripts/apply-webgl2-fallback.py", "exec"))
