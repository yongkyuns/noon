"""Readback admission for #2094's image/vector probe, not an image rasterizer."""
from pathlib import Path

MIXED_IMAGE_TEST = (
    "gpu::presentation::prefix_restore_tests::mixed_images::mixed_image_prefix_sample_restore"
)
MIXED_IMAGE_MARKER = "CACHE_MIXED_IMAGE_PROBE_PASS"


def mixed_image_cases():
    """Fixed 138-case inventory; candidate logs cannot choose their own cases."""
    for profile in range(6):
        width, height = (96, 64) if profile % 2 == 0 else (127, 81)
        size = width * height * 4
        for step in range(4):
            for kind in ("ordinary", "fresh", "resolved", "samples"):
                yield f"i{profile}-s{step}-{kind}", size * (4 if kind == "samples" else 1), True
        yield f"i{profile}-preserved-samples", size * 4, True
        yield f"i{profile}-rebuilt-resolved", size, True
        yield f"i{profile}-rebuilt-samples", size * 4, True
        for defect in ("accumulated", "reordered", "missing-image", "stale-resource"):
            yield f"i{profile}-{defect}", size, False


def mixed_image_evidence_error(directory: Path) -> str | None:
    """Validate exact arrays, required defects and connected reference/history."""
    try:
        if not (directory / "PASS.txt").read_text().startswith(MIXED_IMAGE_MARKER + ":"):
            return "missing mixed-image completion marker"
        profile_text = (directory / "profile.txt").read_text()
        for expected in ("samples=4 transfer=Direct", "sampling=Nearest,Linear,Bicubic"):
            if expected not in profile_text:
                return "missing mixed-image sample/transfer/sampling profile"
        readbacks = {}
        for name, size, equal in mixed_image_cases():
            expected = (directory / f"{name}-expected.rgba").read_bytes()
            actual = (directory / f"{name}-actual.rgba").read_bytes()
            mask = (directory / f"{name}-diff.mask").read_bytes()
            if len(expected) != size or len(actual) != size or len(mask) != size // 4:
                return f"{name}: incomplete mixed-image readback or mask"
            if (expected == actual) != equal:
                return f"{name}: exact comparison or required defect failed"
            calculated = bytes(0 if expected[i:i+4] == actual[i:i+4] else 255
                               for i in range(0, size, 4))
            if calculated != mask:
                return f"{name}: incorrect pixel difference mask"
            readbacks[name] = (expected, actual)
        for profile in range(6):
            for step in range(4):
                base = f"i{profile}-s{step}"
                ordinary = readbacks[f"{base}-ordinary"][0]
                if len(set(ordinary[i:i+4] for i in range(0, len(ordinary), 4))) < 16:
                    return f"{base}: insufficient visible image/vector variation"
                for kind in ("fresh", "resolved"):
                    if readbacks[f"{base}-{kind}"][0] != ordinary:
                        return f"{base}: disconnected ordinary reference"
                samples = readbacks[f"{base}-samples"][0]
                if not any(samples[i:i+4] != samples[i+s:i+s+4]
                           for i in range(0, len(samples), 16) for s in (4, 8, 12)):
                    return f"{base}: no per-sample coverage variation"
            for kind in ("resolved", "samples"):
                first = readbacks[f"i{profile}-s0-{kind}"][1]
                for step in (2, 3):
                    if readbacks[f"i{profile}-s{step}-{kind}"][1] != first:
                        return f"i{profile}: repeated image/vector state has history drift"
                if readbacks[f"i{profile}-s1-{kind}"][1] == first:
                    return f"i{profile}: foreground change has no visible effect"
            ordinary = readbacks[f"i{profile}-s3-ordinary"][0]
            for defect in ("accumulated", "reordered", "missing-image"):
                if readbacks[f"i{profile}-{defect}"][0] != ordinary:
                    return f"i{profile}: disconnected defect reference"
            rebuilt = readbacks[f"i{profile}-rebuilt-resolved"][0]
            stale_expected, stale_actual = readbacks[f"i{profile}-stale-resource"]
            if stale_expected != rebuilt or rebuilt == ordinary or stale_actual != ordinary:
                return f"i{profile}: missing image replacement or stale-prefix witness"
    except (OSError, UnicodeError) as error:
        return f"missing or unreadable mixed-image evidence: {error}"
    return None
