"""Actual Python/WASM contract; no mocked semantics or Python code layout."""
from pathlib import Path as FilePath
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
        self.add(block)
        self.wait(0.05)

        # Ordinary live family copying rebinds every Text handle and metadata
        # alias; it must not attempt to deepcopy the aggregate JS capability.
        line = block.code_lines[2]
        line.annotation = {"purpose": "selection example"}
        center = line.get_center()
        clone = block.copy().shift(RIGHT * 3).scale(0.7).set_opacity(0.5)
        assert clone.code_lines[2] is not line
        assert clone.code_lines[2].text == line.text
        assert clone.code_lines[2].annotation == line.annotation
        assert clone.code_lines[2].font == face.family
        assert (line.get_center() - center).length() < 1e-6
        self.add(clone)

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
        self.wait(0.05)
