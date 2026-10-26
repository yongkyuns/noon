"""Actual Python/WASM contract; no mocked semantics or Python code layout."""
from pathlib import Path as FilePath
from math import dist
from noon import *


class CodeAuthoringContract(Scene):
    def construct(self):
        face = NativeFontFace.bundled("DejaVu Sans Mono")
        source = "\nif ready:\n\tanswer = 42\n\n"
        block = Code(code_string=source, language="python", line_numbers_from=9,
                     paragraph_config={"font": face, "font_size": 20})
        assert len(block.code_lines) == 5
        assert [line.text for line in block.code_lines] == ["", "if ready:", "    answer = 42", "", ""]
        assert [line.text for line in block.line_numbers] == ["9", "10", "11", "12", "13"]
        assert block.code_string == source.expandtabs(4)
        assert len(block.code_lines[2].source_parts_for("answer")) == 1
        assert block.code_lines[2].font == face.family
        assert "vim" in Code.get_styles_list()
        assert "Python" in Code.get_languages_list()

        # Blank rows have native allocated regions even though they have no
        # glyph ink. Cold construction returns an ordinary detached target or
        # an attached member of Code's dedicated painter layer.
        assert len(block.highlights) == 0
        detached = block.get_highlighted_lines(0)
        assert isinstance(detached, Rectangle)
        assert detached.width > 0 and detached.height > 0
        assert abs(detached.get_fill_opacity() - 0.2) < 1e-7
        assert detached.get_stroke_width() == 0
        assert len(block.highlights) == 0
        with_numbers = block.get_highlighted_lines(0, include_line_numbers=True)
        assert with_numbers.width > detached.width
        focus = block.highlight_lines(0, padding=0.03, color=BLUE, fill_opacity=0.3)
        focus.annotation = {"purpose": "retained line region"}
        assert block.highlights[0] is focus
        self.add(block)
        focus_id = focus.id
        self.wait(0.05)

        # Ordinary live family copying rebinds every Text handle and metadata
        # alias; it must not attempt to deepcopy the aggregate JS capability.
        line = block.code_lines[2]
        line.annotation = {"purpose": "selection example"}
        center = line.get_center()
        clone = block.copy().shift(RIGHT * 3).scale(0.7).set_opacity(0.5)
        self.add(clone)
        assert clone.code_lines[2] is not line
        assert clone.code_lines[2].text == line.text
        assert clone.code_lines[2].annotation == line.annotation
        assert clone.code_lines[2].font == face.family
        assert dist(line.get_center(), center) < 1e-6
        assert len(clone.highlights) == 1
        copied_focus = clone.highlights[0]
        assert isinstance(copied_focus, Rectangle)
        assert copied_focus is not focus and copied_focus.id != focus.id
        assert copied_focus.annotation == focus.annotation
        # Opacity writes use ordinary family style semantics, including the
        # region's fill channel; the original family's style stays independent.
        assert abs(copied_focus.get_fill_opacity() - 0.5) < 1e-7
        assert abs(focus.get_fill_opacity() - 0.3) < 1e-7
        copied_region = clone.get_highlighted_lines(0, padding=0.03)
        assert dist(copied_region.get_center(), copied_focus.get_center()) < 1e-6
        assert abs(copied_region.width - copied_focus.width) < 1e-6
        assert abs(copied_region.height - copied_focus.height) < 1e-6
        extra = clone.highlight_lines(3, 4, include_line_numbers=True)
        assert len(clone.highlights) == 2 and clone.highlights[1] is extra
        assert len(block.highlights) == 1 and block.highlights[0] is focus

        # A row region follows the Code panel's frame, not an individually moved
        # line's current ink bounds. These calls run through the active session.
        slot_before = block.get_highlighted_lines(2)
        line.shift(RIGHT * 0.25)
        slot_after = block.get_highlighted_lines(2)
        assert dist(slot_before.get_center(), slot_after.get_center()) < 1e-6
        assert abs(slot_before.width - slot_after.width) < 1e-6
        assert block.code_string == source.expandtabs(4)
        for start, stop in [(-1, 1), (0, 0), (3, 2), (0, 6), (2 ** 32, None)]:
            try:
                block.highlight_lines(start, stop)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid line region must fail atomically")
        for options in [{"padding": -1}, {"padding": float("nan")},
                        {"fill_opacity": 1.1}, {"fill_opacity": float("inf")}]:
            try:
                block.highlight_lines(0, **options)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid region styling must fail atomically")
        assert len(block.highlights) == 1 and block.highlights[0] is focus

        # File IO is performed in the authoring host; the Rust API receives UTF-8
        # and the path hint and performs the same code normalization/layout.
        path = FilePath("/tmp/noon-code-contract.py")
        path.write_text("π = 3\n\tprint(π)\n", encoding="utf-8")
        try:
            new = Code(code_file=path, code_string="ignored", add_line_numbers=False,
                       paragraph_config={"font": face, "font_size": 18}).shift(DOWN * 2)
        finally:
            path.unlink()
        assert not hasattr(new, "line_numbers")
        assert new.code_string == "π = 3\n    print(π)\n"
        assert new.code_lines[1].text == "    print(π)"
        self.add(new)
        try:
            Code(code_string="unchanged", formatter_style="not-a-native-theme")
        except NotImplementedError:
            pass
        else:
            raise AssertionError("unsupported theme must fail explicitly")
        # Ordinary Transform moves the same attached rectangle to a detached
        # two-row target; neither the target nor the child becomes a scene root.
        target = block.get_highlighted_lines(2, 4, padding=0.03, color=BLUE, fill_opacity=0.3)
        assert len(self.mobjects) == 3
        assert all(root is not target for root in self.mobjects)
        self.play(Transform(focus, target), run_time=0.05, rate_func=linear)
        assert focus.id == focus_id and block.highlights[0] is focus
        assert dist(focus.get_center(), target.get_center()) < 1e-6
        assert abs(focus.width - target.width) < 1e-6
        assert abs(focus.height - target.height) < 1e-6
        assert len(self.mobjects) == 3
        assert all(root is not target and root is not focus for root in self.mobjects)
