from pathlib import Path
import re

AUTO_JOIN = "stroke_join: noon_core::StrokeJoin::Round,"
AUTO_CAP = "stroke_cap: noon_core::StrokeCap::Round,"


def strip_bad_auto_insertions(path: Path) -> None:
    text = path.read_text()
    if path.as_posix() != "crates/noon-core/src/lib.rs":
        text = re.sub(r"\n\s*" + re.escape(AUTO_JOIN), "", text)
        text = re.sub(r"\n\s*" + re.escape(AUTO_CAP), "", text)
        # The first migration appended a comma before its injected fields.
        text = text.replace(",,", ",")
    path.write_text(text)


def matching_brace(text: str, open_brace: int) -> int:
    depth = 0
    for pos in range(open_brace, len(text)):
        if text[pos] == "{":
            depth += 1
        elif text[pos] == "}":
            depth -= 1
            if depth == 0:
                return pos
    raise RuntimeError("unbalanced braces")


def add_defaults_to_style_literals(path: Path) -> None:
    if path.as_posix() == "crates/noon-core/src/lib.rs":
        return
    text = path.read_text()
    cursor = 0
    while True:
        index = text.find("Style {", cursor)
        if index < 0:
            break
        # Require an identifier boundary so SetStyle/PackedStyle are ignored.
        if index > 0 and (text[index - 1].isalnum() or text[index - 1] == "_"):
            cursor = index + len("Style {")
            continue
        prefix = text[max(0, index - 32):index]
        if "struct " in prefix:
            cursor = index + len("Style {")
            continue
        open_brace = index + len("Style ")
        end = matching_brace(text, open_brace)
        block = text[index:end + 1]
        if "stroke_join:" in block:
            cursor = end + 1
            continue

        closing_line_start = text.rfind("\n", index, end) + 1
        closing_indent = text[closing_line_start:end]
        if closing_indent.strip():
            closing_indent = ""
        field_indent = closing_indent + "    "
        fields = (
            f"{field_indent}{AUTO_JOIN}\n"
            f"{field_indent}{AUTO_CAP}\n"
        )

        inner = block[len("Style {"):-1]
        # A struct update (`..base`) must remain the final entry.
        update_match = re.search(r"(?m)^(\s*)\.\.", inner)
        if update_match:
            insert_at = update_match.start()
            before = inner[:insert_at]
            after = inner[insert_at:]
            if before and not before.rstrip().endswith(","):
                before = before.rstrip() + ",\n"
            elif before and not before.endswith("\n"):
                before += "\n"
            new_inner = before + fields + after
        else:
            before = inner.rstrip()
            if before and not before.endswith(","):
                before += ","
            new_inner = before + "\n" + fields + closing_indent

        new_block = "Style {" + new_inner + "}"
        text = text[:index] + new_block + text[end + 1:]
        cursor = index + len(new_block)
    path.write_text(text)


for rust_file in Path("crates").rglob("*.rs"):
    strip_bad_auto_insertions(rust_file)
for rust_file in Path("crates").rglob("*.rs"):
    add_defaults_to_style_literals(rust_file)
