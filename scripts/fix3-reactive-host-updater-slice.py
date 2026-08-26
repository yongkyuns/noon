from pathlib import Path

path = Path("crates/noon-web/src/host_player.rs")
text = path.read_text()
old_candidates = [
    '        let slots = format!(r#"[{{\\"id\\":0,\\"objects\\":[{}]}}]"#, object.get());\n',
    '        let slots = format!(r#\\"[{{\\\\\\"id\\\\\\":0,\\\\\\"objects\\\\\\":[{}]}}]\\"#, object.get());\n',
]
new = '        let slots = format!(r#"[{{"id":0,"objects":[{}]}}]"#, object.get());\n'
for old in old_candidates:
    if old in text:
        path.write_text(text.replace(old, new, 1))
        break
else:
    for line in text.splitlines():
        if "let slots = format!" in line and "object.get()" in line:
            raise SystemExit(f"unexpected slots fixture form: {line!r}")
    raise SystemExit("timed host callback slots fixture not found")
