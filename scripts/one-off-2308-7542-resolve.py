#!/usr/bin/env python3
"""Local, fail-closed reconciliation of the three independently inventoried conflicts.

For read-only CI qualification ONLY. Never writes remote refs, edits master,
or publishes the pending #2308 source.
"""
import json
import os
import re
import subprocess
from pathlib import Path

NEW_MASTER = "7542dfafb8ce50cbdf7718766a67d98f547ca84e"
CONFLICTS = {
    "crates/noon-compile/src/lib.rs": 2,
    "crates/noon-compile/src/semantic_lowering/compiled_scene.rs": 1,
    "crates/noon-render-wgpu/src/gpu/retained_text.rs": 1,
}
ROOT = Path.cwd()
CONFLICT_PATTERN = re.compile(rf"(?ms)^<<<<<<< HEAD\n(.*?)^=======\n(.*?)^>>>>>>> {NEW_MASTER}\n")


def git(*args):
    p = subprocess.run(["git", *args], check=True, text=True, capture_output=True)
    return p.stdout.strip()


def resolve(filename, source):
    count = 0

    def one(match):
        nonlocal count
        count += 1
        ours, theirs = match.groups()
        if filename.endswith("/lib.rs"):
            if count == 1:
                assert ours.strip() == "InvalidClickIndicate(ObjectId),", ours
                assert theirs.strip() == "InvalidComposedSource(CompiledComposedSourceError),", theirs
                return theirs + ours
            assert count == 2
            assert "Self::InvalidClickIndicate(id) => write!(" in ours
            assert '"object {} requires a valid click Indicate binding on analytic circle or rectangle content"' in ours
            assert theirs.strip() == "Self::InvalidComposedSource(error) => error.fmt(formatter),", theirs
            return theirs + ours
        if filename.endswith("/compiled_scene.rs"):
            assert count == 1
            assert "lower_content(object.semantic_id, object.content, store, &mut resources)?;" in ours
            assert "crate::validate_click_indicate_binding" in ours
            assert "return Err(SemanticCompiledSceneError::InvalidClickIndicate" in ours
            assert "super::SemanticExecutionContent::Object(content)" in theirs
            assert "super::SemanticExecutionContent::Composed(source)" in theirs
            key = "        if (object.click_indicate.is_some() && !object.signal_bindings.is_empty())"
            assert ours.count(key) == 1
            return theirs + key + ours.split(key, 1)[1]
        assert filename.endswith("/retained_text.rs") and count == 1
        assert "let reset_changes = rows.reset.then(FrameChanges::all);" in ours
        assert "let reset_changes = rows.reset.then(FrameChanges::all);" in theirs
        assert "self.stage_image_draw_eligibility(frame, eligibility_changes)" in ours
        assert "self.stage_image_draw_eligibility(frame, reset_changes.as_ref().unwrap_or(changes))" in theirs
        assert "// Cold or failed image preparation rebuilds residency" in ours
        assert "// A new/recovering consumer can receive" in theirs
        # The same source-reset proof is preserved on both sides. Keep the
        # explicit cold/warm sparse staging formulation already regression-tested.
        return ours

    result = CONFLICT_PATTERN.sub(one, source)
    assert count == CONFLICTS[filename], (filename, count, CONFLICTS[filename])
    assert "<<<<<<<" not in result and "=======" not in result and ">>>>>>>" not in result
    if filename.endswith("/lib.rs"):
        assert result.count("InvalidComposedSource(CompiledComposedSourceError)") >= 2
    if filename.endswith("/compiled_scene.rs"):
        assert "crate::validate_click_indicate_binding(" in result
        assert "(ObjectContentRef::Composed(source.clone()), None)" in result
    return result, count



def apply_clean_merge_repairs():
    """Address independently proven compiler mismatches from cleanly merged files.

    These edits apply only to the local integration candidate, never the remote
    feature or master ref. Fail on any changed source, rather than guessing.
    """
    repairs = {
        "crates/noon-compile/src/semantic_lowering/composed.rs": (
            "            effects: self.effects,\n"
            "            base_transform: Transform2D::IDENTITY,",
            "            effects: self.effects,\n"
            "            click_indicate: None,\n"
            "            base_transform: Transform2D::IDENTITY,",
        ),
        "crates/noon-compile/src/replay_revision/prefix/resources.rs": (
            "                    ObjectContentRef::Geometry(_) => {}",
            "                    // A composed scope refers to member object IDs, not\n"
            "                    // another raw resource. Member rows carry resource\n"
            "                    // dependencies through the existing closure.\n"
            "                    ObjectContentRef::Geometry(_) | ObjectContentRef::Composed(_) => {}",
        ),
    }
    for filename, (original, replacement) in repairs.items():
        path = ROOT / filename
        source = path.read_text()
        assert source.count(original) == 1, (
            "unexpected clean-merge compiler source; refusing to amend", filename
        )
        path.write_text(source.replace(original, replacement, 1))
        git("add", "--", filename)
    return sorted(repairs)

def main():
    unmerged = git("diff", "--name-only", "--diff-filter=U").splitlines()
    assert set(unmerged) == set(CONFLICTS), ("unexpected merge conflicts", unmerged)
    assert git("rev-parse", "MERGE_HEAD") == NEW_MASTER
    assert git("merge-base", "HEAD", NEW_MASTER) == os.environ["EXPECTED_OLD_MASTER"]
    report = {"conflicted_paths": unmerged, "resolved_blocks": {}, "new_master": NEW_MASTER}
    for filename in sorted(unmerged):
        source = ROOT / filename
        result, count = resolve(filename, source.read_text())
        source.write_text(result)
        git("add", "--", filename)
        report["resolved_blocks"][filename] = count
    assert not git("diff", "--name-only", "--diff-filter=U"), "unresolved Git paths"
    assert not git("ls-files", "-u"), "leftover unmerged Git index"
    report["clean_merge_repairs"] = apply_clean_merge_repairs()
    report["staged_tree"] = git("write-tree")
    Path(os.environ.get("REPORT_PATH", "read-only-resolution.json")).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
