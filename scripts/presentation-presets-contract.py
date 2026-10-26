"""Actual Python/WASM presentation contract, executed by the shared authoring gate."""
import copy
import json

from noon import Theme, Text, NativeFontFace, RED
from _noon_errors import NoonIndexError, NoonValueError


dark, light = Theme.dark(), Theme.light()
plain_before = Text("Unstyled")
title = dark.text("Meaning", role="title")
assert isinstance(title, Text)
title_size = title.font_size
title_color = title.get_color()
face = NativeFontFace.bundled("DejaVu Sans Mono")
override = dark.text("Meaning", role="title", font=face, font_size=17, color=RED)
ordinary_override = Text("Meaning", font=face, font_size=17, color=RED)
assert override.font_size == 17 and override.get_color() == ordinary_override.get_color()
assert override.font == face.family
assert abs(override.width - ordinary_override.width) < 1e-9
assert abs(override.height - ordinary_override.height) < 1e-9
assert title.font_size == title_size and title.get_color() == title_color
assert title_color == dark.color("foreground")
assert dark.color("foreground") != light.color("foreground")
assert dark.text("Muted", role="caption").get_color() == dark.color("muted")
assert light.text("Muted", role="caption").get_color() == light.color("muted")
assert copy.copy(dark) is dark and copy.deepcopy(dark) is dark
assert Text("Unstyled").font_size == plain_before.font_size
assert Text("Unstyled").get_color() == plain_before.get_color()
assert dark.spacing("small") < dark.spacing("normal") < dark.spacing("large")
assert len({dark.categorical(i) for i in range(dark.categorical_count)}) == dark.categorical_count

for operation in (lambda: Theme("missing"), lambda: dark.color("missing"),
                  lambda: dark.spacing("missing"), lambda: dark.text("source", role="missing")):
    try:
        operation()
    except NoonValueError as error:
        assert error.code == "presentation.unknown_name"
    else:
        raise AssertionError("unknown presentation name was accepted")
try:
    dark.categorical(dark.categorical_count)
except NoonIndexError as error:
    assert error.code == "presentation.category_out_of_range"
else:
    raise AssertionError("categorical colors silently cycled")
try:
    dark.categorical(-1)
except NoonValueError as error:
    assert error.code == "presentation.invalid_category_index"
else:
    raise AssertionError("negative category index was accepted")
_presentation_values_json = json.dumps({
    "categories": dark.categorical_count, "titleSize": title_size,
    "overrideSize": override.font_size, "ordinaryTextSize": plain_before.font_size,
})
