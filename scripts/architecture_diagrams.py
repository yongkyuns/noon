#!/usr/bin/env python3
"""Render/check architecture SVGs. D2 sources, not generated SVGs, are editable.

Install D2 0.9.0, then run:
  python3 scripts/architecture_diagrams.py
  python3 scripts/architecture_diagrams.py --check
Set D2 to an alternate executable path. No network access or installation occurs.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
VERSION = "v0.9.0"
RENDER_ARGS = (
    "--layout", "elk", "--theme", "0", "--dark-theme", "200", "--pad", "24",
    "--elk-nodeNodeBetweenLayers", "32", "--elk-edgeNodeBetweenLayers", "12",
    "--elk-padding", "[top=38,left=24,bottom=24,right=24]",
)


def tool_environment() -> dict[str, str]:
    # D2 supports environment overrides even for rendering defaults. Do not let
    # a caller's watch/sketch/font/scale settings change the checked-in assets.
    ignored = {"DEBUG", "IMG_CACHE", "OMIT_VERSION", "SCALE"}
    return {key: value for key, value in os.environ.items()
            if not key.startswith(("D2_", "ELK_")) and key not in ignored}


def run(command: list[str], root: Path) -> None:
    subprocess.run(command, cwd=root, env=tool_environment(), check=True)


def check_links(root: Path, stems: set[str]) -> None:
    """Keep every render discoverable; resolve diagram links from each document."""
    referenced: set[str] = set()
    for document in (root / "README.md", root / "docs/architecture.md"):
        text = document.read_text(encoding="utf-8")
        for match in re.finditer(r"!?\[[^\]]*\]\(([^)\s]+)\)", text):
            target = match.group(1).split("#", 1)[0]
            if "diagrams/" not in target or "://" in target:
                continue
            path = (document.parent / target).resolve()
            if not path.is_file():
                raise ValueError(f"broken diagram link in {document.relative_to(root)}: {target}")
            if path.suffix == ".svg":
                referenced.add(path.stem)
    missing = stems - referenced
    if missing:
        raise ValueError(f"unreferenced architecture SVGs: {', '.join(sorted(missing))}")


def render(root: Path, executable: str, check: bool) -> None:
    tool = shutil.which(executable)
    if tool is None:
        raise ValueError(f"D2 {VERSION} is required; install it or set D2=/path/to/d2")
    version = subprocess.check_output([tool, "version"], text=True, cwd=root, env=tool_environment()).strip()
    if version != VERSION:
        raise ValueError(f"expected D2 {VERSION}, got {version!r}")
    directory = root / "docs/diagrams"
    all_sources = sorted(directory.glob("*.d2"))
    sources = [path for path in all_sources if not path.name.startswith("_")]
    if not sources:
        raise ValueError("no architecture D2 sources found")
    stems = {path.stem for path in sources}
    orphans = {path.stem for path in directory.glob("*.svg")} - stems
    if orphans:
        raise ValueError(f"orphan architecture SVGs: {', '.join(sorted(orphans))}")
    # Formatting is checked, never silently rewritten, including imported styles.
    run([tool, "fmt", "--check", *map(str, all_sources)], root)
    # The pinned flags and sanitized environment make rendering reproducible.
    # Render ALL inputs before writing anything, so syntax errors cannot leave a
    # half-regenerated documentation set. --check never modifies checked-in files.
    with tempfile.TemporaryDirectory(prefix="noon-diagrams-") as temporary:
        generated: list[tuple[Path, Path]] = []
        for source in sources:
            output = Path(temporary) / f"{source.stem}.svg"
            run([tool, *RENDER_ARGS, str(source), str(output)], root)
            svg = ET.parse(output).getroot()
            if svg.tag != "{http://www.w3.org/2000/svg}svg":
                raise ValueError(f"D2 did not produce an SVG: {source.name}")
            generated.append((output, source.with_suffix(".svg")))
        stale = [destination.name for output, destination in generated
                 if not destination.exists() or output.read_bytes() != destination.read_bytes()]
        if check and stale:
            raise ValueError("stale/missing SVGs: " + ", ".join(stale)
                             + "; run python3 scripts/architecture_diagrams.py")
        if not check:
            for output, destination in generated:
                destination.write_bytes(output.read_bytes())
    check_links(root, stems)
    print(f"Architecture diagrams: {len(sources)} SVGs {'checked' if check else 'rendered'} "
          f"with D2 {VERSION}; links verified.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail on stale output; do not edit files")
    args = parser.parse_args()
    try:
        render(ROOT, os.environ.get("D2", "d2"), args.check)
    except (OSError, ValueError, ET.ParseError, subprocess.CalledProcessError) as error:
        print(f"architecture diagrams: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
