"""Actual shared-Code source editing through the thin Python/WASM facade."""
from math import dist
from noon import *


class CodeSourceAuthoringContract(Scene):
    def construct(self):
        source = "\n".join((
            "__shared__ float tile[TILE_DIM][TILE_DIM];",
            "int x = threadIdx.x;",
            "tile[x][0] = in[x];",
            "",
            "__syncthreads();",
            "out[x] = tile[x][0]; // in tile",
        ))
        padded_line = "__shared__ float tile[TILE_DIM][TILE_DIM + 1];"
        reserved = source.replace(source.split("\n")[0], padded_line, 1)
        face = NativeFontFace.bundled("DejaVu Sans Mono")
        listing = Code(
            code_string=source, reserved_source=reserved, language="cpp",
            paragraph_config={"font": face, "font_size": 16},
            syntax_colors={"foreground": WHITE, "keyword": PURPLE,
                           "comment": GRAY, "number": ORANGE},
            token_colors={"in": BLUE, "tile": BLUE, "out": GREEN,
                          "__syncthreads": RED, "TILE_DIM": ORANGE,
                          "threadIdx.x": ORANGE},
        )
        previous = listing.code_lines[0]
        previous.annotation = "preserve the old source alias"
        untouched = list(listing.code_lines)[1:]
        number_aliases = list(listing.line_numbers)
        positions = [tuple(row.get_center()) for row in untouched]
        panel_size = (listing.background.width, listing.background.height)
        row_slot = listing.get_highlighted_lines(0, padding=0)

        # Before Scene.add, shared store authoring swaps one row and returns
        # ordinary Text handles. A wider reserved variant cannot resize the page.
        old, current = listing.replace_line(0, padded_line)
        assert old is previous and old.text == source.split("\n")[0]
        assert old.annotation == "preserve the old source alias"
        assert isinstance(current, Text) and listing.code_lines[0] is current
        assert current is not old and current.text == padded_line
        assert current.font == face.family and current.font_size == 16
        assert listing.code_string == reserved
        assert list(listing.code_lines)[1:] == untouched
        assert list(listing.line_numbers) == number_aliases
        assert (listing.background.width, listing.background.height) == panel_size
        assert current.width <= row_slot.width + 1e-6
        assert all(dist(row.get_center(), position) < 1e-6
                   for row, position in zip(untouched, positions))
        assert len(self.mobjects) == 0

        self.add(listing)
        self.wait(0.05)
        clone = listing.copy().scale(0.7).rotate(0.08).shift(DOWN * 2)
        self.add(clone)
        unchanged_ids = [row.id for row in clone.code_lines][1:]
        unchanged_positions = [tuple(row.get_center()) for row in clone.code_lines][1:]
        cloned_numbers = list(clone.line_numbers)
        clone_panel = (clone.background.width, clone.background.height,
                       tuple(clone.background.get_center()))
        clone_row = clone.get_highlighted_lines(0, padding=0)
        clone_previous = clone.code_lines[0]
        clone_previous_id = clone_previous.id
        clone_previous.annotation = "live replacement"

        # The clone uses its copied Rust recipe and current transformed frame.
        # The source object and every unaffected row/gutter alias stay stable.
        old, current = clone.replace_line(0, source.split("\n")[0])
        assert old is clone_previous and old.annotation == "live replacement"
        assert old.id == clone_previous_id
        assert (old._semantic_handle.semanticSlot, old._semantic_handle.semanticGeneration) != (
            current._semantic_handle.semanticSlot, current._semantic_handle.semanticGeneration)
        assert clone.code_lines[0] is current
        assert old.text == padded_line and current.text == source.split("\n")[0]
        assert clone.code_string == source and listing.code_string == reserved
        assert [row.id for row in clone.code_lines][1:] == unchanged_ids
        assert list(clone.line_numbers) == cloned_numbers
        assert (clone.background.width, clone.background.height,
                tuple(clone.background.get_center())) == clone_panel
        assert all(dist(row.get_center(), position) < 1e-6
                   for row, position in zip(list(clone.code_lines)[1:], unchanged_positions))
        after_slot = clone.get_highlighted_lines(0, padding=0)
        assert dist(clone_row.get_center(), after_slot.get_center()) < 1e-6
        assert abs(clone_row.width - after_slot.width) < 1e-6
        assert abs(clone_row.height - after_slot.height) < 1e-6

        # Invalid indices, multiline rows and overflow leave both source and
        # wrapper observations untouched, including after a successful edit.
        for index, text in [(-1, "x"), (6, "x"), (2 ** 32, "x"),
                            (0, "first\nsecond"), (0, "very_long_name" * 100)]:
            try:
                clone.replace_line(index, text)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid source replacement must fail atomically")
        assert clone.code_string == source and clone.code_lines[0] is current
        assert [row.id for row in clone.code_lines][1:] == unchanged_ids
        assert len(self.mobjects) == 2

        # The previous row is explicitly a temporary scene root. The current
        # row remains a child; ordinary animations own the 0.75-second transition.
        self.add(old)
        assert old.id == clone_previous_id
        self.play(FadeOut(old), Write(current), run_time=0.75, rate_func=linear)
        assert len(self.mobjects) == 2
        assert all(root is not old and root is not current for root in self.mobjects)
        assert clone.code_lines[0] is current
        assert clone.code_string == source
        assert [row.id for row in clone.code_lines][1:] == unchanged_ids

        for kwargs in [
            {"reserved_source": "wrong row count"},
            {"syntax_colors": {"not_a_category": RED}},
            {"token_colors": {"in[0]": BLUE}},
            {"token_colors": {"": BLUE}},
        ]:
            try:
                Code(code_string=source, language="cpp", **kwargs)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid shared Code options must fail")
        assert len(self.mobjects) == 2
        self.source_contract_observed = {
            "rows": len(listing.code_lines),
            "unchanged_rows": len(unchanged_ids),
            "source": clone.code_string,
            "replacement_duration": 0.75,
        }
