"""Isolated R4 qualification transport; excluded from the product PR.

Apply the five-line, hash-guarded accessor edit before qualification. After all
checks pass, retain only the four explicitly selected source blobs in GitHub's
object database. This does not create or update a branch, PR, or approval.
"""
import hashlib
import json
import os
from pathlib import Path
import sys
import urllib.request

CORE = Path("crates/noon-core/src/semantic_store.rs")
PATHS = [str(CORE), "crates/noon-compile/src/semantic_lowering/root_order.rs",
         "crates/noon/tests/root_order_acceptance.rs",
         "crates/noon/tests/root_order_transaction_regressions.rs"]


def blob_sha(data: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def patch() -> None:
    data = CORE.read_bytes()
    assert blob_sha(data) == "5d4ab8a97c53795ee880f6550bb52e9205557a2b", "core source moved; recheck master"
    text = data.decode()
    anchor = "    /// Direct successor of `member`, resolved without scanning siblings.\n"
    addition = """    /// Return the last member without traversing or allocating sibling order.
    pub fn last_member(&self) -> Option<SemanticNodeId> {
        self.members.tail
    }

"""
    assert text.count(anchor) == 1
    CORE.write_text(text.replace(anchor, addition + anchor))


def retain_blobs() -> None:
    token = os.environ["GH_TOKEN"]
    entries = []
    for path in PATHS:
        content = Path(path).read_bytes()
        request = urllib.request.Request(
            "https://api.github.com/repos/yongkyuns/noon/git/blobs",
            data=json.dumps({"content": content.decode(), "encoding": "utf-8"}).encode(),
            headers={"Authorization": "Bearer " + token,
                     "Accept": "application/vnd.github+json",
                     "Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            result = json.load(response)
        assert result["sha"] == blob_sha(content), "uploaded bytes differ"
        entries.append({"path": path, "mode": "100644", "type": "blob", "sha": result["sha"]})
    Path("evidence/qualified-blobs.json").write_text(json.dumps(entries, indent=2) + "\n")
    print(json.dumps(entries, indent=2))


if __name__ == "__main__":
    if sys.argv[1:] == ["patch"]:
        patch()
    elif sys.argv[1:] == ["retain"]:
        retain_blobs()
    else:
        raise SystemExit("expected patch or retain")
