"""Independent admission for #2094's low-level mixed image/glyph/vector probe.

Raw RGBA arrays are actual driver outputs. This module checks their complete
inventory, exact comparisons, seeded defects and cross-state relationships; it
is not a rasterizer or an oracle for image filtering and glyph coverage.
"""
from pathlib import Path

MIXED_TEXT_TEST = (
    "gpu::presentation::prefix_restore_tests::mixed::mixed_image_glyph_prefix_sample_restore"
)
MIXED_TEXT_MARKER = "CACHE_MIXED_PROBE_PASS"
MARKER = MIXED_TEXT_MARKER + ":"
PROFILE = "profiles=Nearest/96x64,Linear/127x81,Bicubic/127x81"


def cases():
    """Fixed inventory: 72 exact comparisons and 18 deliberate differences."""
    for profile, (width, height) in enumerate(((96, 64), (127, 81), (127, 81))):
        for step in range(6):
            for kind in ("ordinary", "resolved", "fresh", "samples"):
                size = width * height * (16 if kind == "samples" else 4)
                yield f"p{profile}-s{step}-{kind}", size, True
        for defect in ("no-image", "no-text", "reordered", "accumulated", "stale-image", "stale-text"):
            yield f"p{profile}-{defect}", width * height * 4, False


def evidence_error(directory: Path) -> str | None:
    try:
        if not (directory / "PASS.txt").read_text().startswith(MARKER):
            return "missing mixed-probe completion marker"
        profile_text = (directory / "profile.txt").read_text()
        if "samples=4" not in profile_text or PROFILE not in profile_text:
            return "missing mixed-probe sampling profile"
        readbacks = {}
        for name, size, equal in cases():
            expected = (directory / f"{name}-expected.rgba").read_bytes()
            actual = (directory / f"{name}-actual.rgba").read_bytes()
            mask = (directory / f"{name}-diff.mask").read_bytes()
            if len(expected) != size or len(actual) != size or len(mask) != size // 4:
                return f"{name}: incomplete readback or mask"
            if (expected == actual) != equal:
                return f"{name}: exact comparison or ineffective negative control"
            recomputed = bytes(0 if expected[i:i + 4] == actual[i:i + 4] else 255
                               for i in range(0, size, 4))
            if mask != recomputed:
                return f"{name}: incorrect differing-pixel mask"
            readbacks[name] = expected, actual
        for profile in range(3):
            def pair(step, kind):
                return readbacks[f"p{profile}-s{step}-{kind}"]
            for step in range(6):
                ordinary = pair(step, "ordinary")[1]
                for kind in ("resolved", "fresh"):
                    if pair(step, kind)[0] != ordinary:
                        return f"p{profile}-s{step}: different ordinary reference for {kind}"
                if ordinary == ordinary[:4] * (len(ordinary) // 4):
                    return f"p{profile}-s{step}: blank mixed reference"
                samples = pair(step, "samples")[1]
                if not any(samples[i:i + 4] != samples[i + 4:i + 8]
                           or samples[i:i + 4] != samples[i + 8:i + 12]
                           or samples[i:i + 4] != samples[i + 12:i + 16]
                           for i in range(0, len(samples), 16)):
                    return f"p{profile}-s{step}: no unequal MSAA witness"
            for kind in ("resolved", "samples"):
                baseline = pair(0, kind)[1]
                if pair(1, kind)[1] == baseline:
                    return f"p{profile}: moving foreground did not change {kind}"
                for step in (2, 3):
                    if pair(step, kind)[1] != baseline:
                        return f"p{profile}-s{step}: history-dependent {kind}"
            for defect in ("no-image", "no-text", "reordered", "accumulated"):
                if readbacks[f"p{profile}-{defect}"][0] != pair(3, "ordinary")[1]:
                    return f"p{profile}-{defect}: different ordinary reference"
            for step, defect in ((4, "stale-image"), (5, "stale-text")):
                expected, actual = readbacks[f"p{profile}-{defect}"]
                if expected != pair(step, "ordinary")[1]:
                    return f"p{profile}-{defect}: different edited reference"
                if actual != pair(step - 1, "ordinary")[1]:
                    return f"p{profile}-{defect}: stale prefix is not prior state"
    except (OSError, UnicodeError) as error:
        return f"missing or unreadable mixed probe evidence: {error}"
    return None
