"""Host filesystem helpers for user paths, independent of scene execution."""
from __future__ import annotations

from pathlib import Path
import os
import shutil
import time
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from manim.typing import StrPath


def ensure_executable(path_to_exe: Path) -> bool:
    """Check the host executable search path or an explicit user path."""
    if path_to_exe.parent == Path("."):
        executable = shutil.which(path_to_exe.stem)
        if executable is None:
            return False
    else:
        executable = path_to_exe
    return os.access(executable, os.X_OK)


def modify_atime(file_path: str) -> None:
    """Set a user file's access time while retaining its modification time."""
    os.utime(file_path, times=(time.time(), Path(file_path).stat().st_mtime))


def add_extension_if_not_present(file_name: Path, extension: str) -> Path:
    """Append the extension to the existing suffix, preserving matching paths."""
    if file_name.suffix != extension:
        return file_name.with_suffix(file_name.suffix + extension)
    return file_name


def guarantee_existence(path: Path) -> Path:
    """Create parents and return the strictly resolved directory."""
    path.mkdir(parents=True, exist_ok=True)
    return path.resolve(strict=True)


def guarantee_empty_existence(path: Path) -> Path:
    """Replace an existing directory and return its strictly resolved path."""
    if path.exists():
        shutil.rmtree(str(path))
    path.mkdir(parents=True, exist_ok=True)
    return path.resolve(strict=True)


def seek_full_path_from_defaults(
    file_name: StrPath, default_dir: Path, extensions: list[str]
) -> Path:
    """Search the explicit user path before ordered default-directory suffixes."""
    possible_paths = [Path(file_name).expanduser()]
    possible_paths += [
        Path(default_dir) / f"{file_name}{extension}" for extension in ["", *extensions]
    ]
    for path in possible_paths:
        if path.exists():
            return path
    raise OSError(
        f"From: {Path.cwd()}, could not find {file_name} at either "
        f"of these locations: {list(map(str, possible_paths))}"
    )
