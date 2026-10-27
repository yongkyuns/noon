#!/usr/bin/env python3
"""Merge qualified #2308 candidate into pinned current master without remote writes.

Conflict policy: preserve existing click-Indicate declarations/admission;
choose master's newer paint/effect ownership in each overlap. Explicit
per-hunk whitelist; any unexpected conflict is a hard error.
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import tarfile
from pathlib import Path

root = Path(os.environ["RUNNER_TEMP"])
out = root / "noon2308-current-master-evidence"
out.mkdir(parents=True, exist_ok=True)
repo = Path.cwd()
source = root / "noon2308-source" / "candidate-source.tar.gz"
receipt = root / "noon2308-source" / "manifest.json"
old_pr = os.environ["PIN_PR2308"]
old_master = os.environ["PIN_OLD_MASTER"]
new_master = os.environ["PIN_CURRENT_MASTER"]

def git(*args, cwd=repo, check=True, input=None):
    result = subprocess.run(["git", *args], cwd=cwd, input=input, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, check=False)
    if check and result.returncode:
        raise RuntimeError(f"git {args} rc={result.returncode}: {result.stderr.decode(errors='replace')[-1500:]}")
    return result

def output(*args, cwd=repo):
    return git(*args, cwd=cwd).stdout.decode().strip()

assert hashlib.sha256(source.read_bytes()).hexdigest() == os.environ["ARCHIVE_SHA256"]
evidence = json.loads(receipt.read_text())
assert evidence["tested_candidate"]["tree"] == os.environ["CANDIDATE_TREE"]
assert evidence["tested_candidate"]["parents"] == [old_pr, old_master]
assert evidence["tracked_source_unchanged"]
assert all(evidence["command_exit_codes"][k] == 0 for k in
           ("fast", "wasm", "gradient", "pointer-tests", "prefix-bindings",
            "interactions-tests", "prepared-input", "web-preflight"))

refs = [old_pr, old_master, new_master]
fetch = git("fetch", "--no-tags", "--depth=256", "origin", *refs)
(out / "fetch.log").write_bytes(fetch.stdout + fetch.stderr)
assert all(output("rev-parse", sha+"^{commit}") == sha for sha in refs)
assert output("merge-base", old_pr, new_master) != ""
work = root / "noon2308-compiler-reconciled"
git("worktree", "add", "--detach", str(work), old_pr)
git("config", "user.name", "Noon isolated integration qualification", cwd=work)
git("config", "user.email", "qualification@example.invalid", cwd=work)
# Replace old source fully, including deletions; do not silently retain stale files.
for entry in work.iterdir():
    if entry.name == ".git":
        continue
    if entry.is_dir() and not entry.is_symlink():
        shutil.rmtree(entry)
    else:
        entry.unlink()
with tarfile.open(source, "r:gz") as tar:
    items = tar.getmembers()
    assert all(not (item.name.startswith("/") or ".." in Path(item.name).parts
                    or item.issym() or item.islnk()) for item in items)
    tar.extractall(work, filter="data")
git("add", "--all", cwd=work)
assert output("write-tree", cwd=work) == os.environ["CANDIDATE_TREE"]
candidate = git("commit-tree", os.environ["CANDIDATE_TREE"],
                "-p", old_pr, "-p", old_master,
                cwd=work, input=b"Source-qualified isolated #2308 candidate\n").stdout.decode().strip()
assert output("rev-parse", candidate+"^{tree}", cwd=work) == os.environ["CANDIDATE_TREE"]
git("checkout", "--detach", "--force", candidate, cwd=work)
merge = git("merge", "--no-commit", "--no-ff", new_master, cwd=work, check=False)
(out / "original-master-merge.log").write_bytes(merge.stdout + merge.stderr)
paths = output("diff", "--name-only", "--diff-filter=U", cwd=work).splitlines()
expected = [
    "crates/noon-compile/src/lib.rs",
    "crates/noon-compile/src/semantic_lowering/compiled_scene.rs",
    "crates/noon-compile/src/semantic_lowering/projection.rs",
    "crates/noon-compile/src/transaction_preflight.rs",
]
assert sorted(paths) == sorted(expected), ("unexpected merge paths", paths)
assert merge.returncode == 1
(out / "original-conflicts.diff").write_bytes(git("diff", "--cc", cwd=work).stdout)
(out / "original-unmerged-stages.txt").write_text(output("ls-files", "-u", cwd=work)+"\n")
marker = re.compile(r"(?ms)^<<<<<<< HEAD\n(.*?)^=======\n(.*?)^>>>>>>> [^\n]+\n")
counts = [3, 2, 4, 5]
decisions = []

def resolve(path, ours, theirs):
    if path.endswith("/src/lib.rs"):
        if "InvalidClickIndicate(ObjectId)" in ours and "InvalidEffectAttachmentChange" in theirs:
            return "    InvalidClickIndicate(ObjectId),\n" + theirs
        if "validate_click_indicate_binding(" in ours and "fill_gradient" in ours:
            return ours.replace("fill_gradient", "fill_paint")
    elif path.endswith("semantic_lowering/compiled_scene.rs"):
        if "InvalidClickIndicate" in ours and "capture_paint_from_arena" in theirs:
            return ours + "        }\n" + theirs
        if "click_indicate:" in ours and "effect:" in theirs:
            retained = "".join(line for line in ours.splitlines(keepends=True) if "glow:" not in line)
            return retained + theirs
    elif path.endswith("semantic_lowering/projection.rs"):
        if "click_indicate" in ours and ("effect:" in theirs or "effect," in theirs):
            retained = "".join(line for line in ours.splitlines(keepends=True)
                               if "glow:" not in line and "glow," not in line
                               and "fill_gradient:" not in line)
            # 'click_indicate' values are layered onto master's paint/effect.
            assert "click_indicate" in retained
            return theirs + retained
    elif path.endswith("/transaction_preflight.rs"):
        if "click_indicate" in ours and "effect" in theirs:
            retained = "".join(line for line in ours.splitlines(keepends=True)
                               if "glow:" not in line and "glow," not in line)
            assert "click_indicate" in retained
            return theirs + retained
    raise AssertionError(f"Unreviewed conflict at {path}: ours={ours[:200]!r}, theirs={theirs[:200]!r}")

for path, expected_count in zip(expected, counts):
    file = work / path
    raw = file.read_text()
    found = list(marker.finditer(raw))
    assert len(found) == expected_count, (path, "expected", expected_count, "found", len(found))
    def replace(match):
        ours, theirs = match.group(1), match.group(2)
        replacement = resolve(path, ours, theirs)
        decisions.append({"file":path, "ours_sha256":hashlib.sha256(ours.encode()).hexdigest(),
                          "theirs_sha256":hashlib.sha256(theirs.encode()).hexdigest(),
                          "resolution_sha256":hashlib.sha256(replacement.encode()).hexdigest()})
        return replacement
    new = marker.sub(replace, raw)
    assert "<<<<<<<" not in new and ">>>>>>>" not in new
    assert new.count("click_indicate") >= raw.count("click_indicate") - 4, path
    file.write_text(new)
    git("add", path, cwd=work)
assert not output("ls-files", "-u", cwd=work)
git("diff", "--cached", "--check", cwd=work)
resolved_tree = output("write-tree", cwd=work)
(out / "master-resolution.patch").write_bytes(git("diff", "--cached", "--binary", candidate, cwd=work).stdout)
git("commit", "-m", "Integrate prepared historical click bindings with current paint/effect compiler", cwd=work)
merged = output("rev-parse", "HEAD", cwd=work)
assert output("show", "-s", "--format=%P", "HEAD", cwd=work).split() == [candidate,new_master]
assert output("rev-parse", "HEAD^{tree}", cwd=work) == resolved_tree
(out / "resolutions.json").write_text(json.dumps(decisions, indent=2)+"\n")
record = {"scope":"Isolated master reconciliation; no feature/master refs modified",
          "inputs":evidence["inputs"],"candidate_source_archive_sha256":os.environ["ARCHIVE_SHA256"],
          "candidate_commit_local_only":candidate,"candidate_tree":os.environ["CANDIDATE_TREE"],
          "pinned_current_master":new_master,"compiler_conflicts":paths,
          "resolution_count":len(decisions),"resolved_master_tree":resolved_tree,
          "merged_commit_local_only":merged,"tracked_source_unchanged":True}
(out / "manifest.json").write_text(json.dumps(record,indent=2)+"\n")
print("Resolved pinned-master compiler conflicts:",len(decisions),flush=True)
print(json.dumps(record,indent=2),flush=True)
