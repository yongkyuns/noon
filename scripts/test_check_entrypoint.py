#!/usr/bin/env python3
"""Orchestration tests only; real guard/Cargo fixtures live in the shell suite."""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
GUARDS = ["layer-dependency-ratchet.sh", "noon-core-module-ownership-ratchet.sh",
          "renderer-host-boundary-ratchet.sh", "transient-presentation-ratchet.sh",
          "active-perf-frontend-ratchet.sh", "architecture-ratchet.sh"]


REFERENCE_CALL = "python reference"


class EntrypointTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="noon entrypoint ")
        self.addCleanup(temp.cleanup)
        self.outer = Path(temp.name)
        self.root = self.outer / "repo"
        (self.root / "scripts").mkdir(parents=True)
        (self.root / "crates/noon-web/src").mkdir(parents=True)
        (self.root / "crates/noon-web/src/lib.rs").write_text("")
        for name in ("check.sh", "check-architecture.sh"):
            shutil.copyfile(ROOT / "scripts" / name, self.root / "scripts" / name)
        for name in GUARDS:
            (self.root / "scripts" / name).write_text(
                '#!/usr/bin/env bash\nset -euo pipefail\n'
                'name="$(basename "$0")"\nprintf "%s\\n" "$name" >> "$CALLS"\n'
                'printf "%s" "$GIT_INDEX_FILE" > "$SEEN_INDEX"\n'
                'git ls-files --error-unmatch untracked.rs >/dev/null\n'
                'if [[ "${FAIL_GUARD:-}" == "$name" ]]; then exit 1; fi\n')
        (self.root / "scripts/build-web-demo.sh").write_text('echo web >> "$CALLS"\n')
        reference = self.root / "tests/visual-effects/test_reference.py"
        reference.parent.mkdir(parents=True)
        # Observe orchestration without importing the real operator reference.
        reference.write_text(
            'import os\n'
            'with open(os.environ["CALLS"], "a") as log: log.write("python reference\\n")\n'
            'raise SystemExit(int(os.environ.get("FAIL_REFERENCE", "0")))\n'
        )
        (self.outer / "bin").mkdir()
        fake = self.outer / "bin/cargo"
        fake.write_text(
            '#!/usr/bin/env bash\n'
            'printf "cargo %s\\n" "$*" >> "$CALLS"\n'
            'if [[ "${FAIL_CARGO_ARGS:-}" == "$*" ]]; then exit 17; fi\n'
        )
        fake.chmod(0o755)
        self.env = dict(os.environ, GIT_OPTIONAL_LOCKS="0", CALLS=str(self.outer / "calls"),
                        SEEN_INDEX=str(self.outer / "seen-index"),
                        PATH=str(self.outer / "bin") + os.pathsep + os.environ["PATH"])
        for args in (("init", "-q", "-b", "master"), ("config", "user.name", "Fixture"),
                     ("config", "user.email", "fixture@example.invalid"), ("add", "."),
                     ("-c", "commit.gpgSign=false", "-c", "core.hooksPath=/dev/null", "commit", "-qm", "fixture")):
            self.git(*args)
        self.git("update-ref", "refs/remotes/origin/master", "HEAD")
        (self.root / "untracked.rs").write_text("// untracked\n")
        (self.root / "staged.rs").write_text("// staged\n")
        self.git("add", "staged.rs")
        (self.root / "staged.rs").write_text("// unstaged edit after staging\n")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, env=self.env, text=True).strip()

    def invoke(self, *args, failure="", cargo_failure="", reference_failure=0):
        index = (self.root / ".git/index").read_bytes()
        status = self.git("status", "--porcelain=v1")
        log = self.outer / "calls"
        log.write_text("")
        result = subprocess.run(["bash", str(self.root / "scripts/check.sh"), *args],
                                cwd=self.outer, env=dict(self.env, FAIL_GUARD=failure,
                                                         FAIL_CARGO_ARGS=cargo_failure,
                                                         FAIL_REFERENCE=str(reference_failure)),
                                text=True, capture_output=True)
        self.assertEqual((self.root / ".git/index").read_bytes(), index)
        self.assertEqual(self.git("status", "--porcelain=v1"), status)
        seen = self.outer / "seen-index"
        if seen.exists():
            self.assertFalse(Path(seen.read_text()).exists(), "temporary index leaked")
        return result, log.read_text().splitlines()

    def test_all_modes_gate_before_compilation(self):
        lint = ["cargo fmt --all -- --check", "cargo check --workspace --all-targets --all-features",
                "cargo clippy --workspace --all-targets --all-features -- -D warnings"]
        tests = [
            # Cargo --tests also selects explicitly testable example targets.
            "cargo test --workspace --all-features --lib --tests --bins --no-fail-fast",
            "cargo test --workspace --all-features --doc --no-fail-fast",
        ]
        expected = {"architecture": [], "fmt-lint": lint,
                    "fast": lint + [REFERENCE_CALL, "cargo test --workspace --all-features --lib --no-fail-fast"],
                    "rust": lint + [REFERENCE_CALL] + tests,
                    "full": lint + [REFERENCE_CALL] + tests + ["web"],
                    "test": [REFERENCE_CALL] + tests, "web": ["web"]}
        for mode, commands in expected.items():
            with self.subTest(mode=mode):
                result, calls = self.invoke(mode, "HEAD")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(calls, GUARDS + commands)

    def test_test_failure_stops_following_commands_including_web_after_docs(self):
        test_command, doc_command = (
            "cargo test --workspace --all-features --lib --tests --bins --no-fail-fast",
            "cargo test --workspace --all-features --doc --no-fail-fast",
        )
        lint = ["cargo fmt --all -- --check",
                "cargo check --workspace --all-targets --all-features",
                "cargo clippy --workspace --all-targets --all-features -- -D warnings"]
        for failing, expected_tail in (
            (test_command, [f"cargo {test_command.removeprefix('cargo ')}"]),
            (doc_command, [f"cargo {test_command.removeprefix('cargo ')}",
                           f"cargo {doc_command.removeprefix('cargo ')}"]),
        ):
            with self.subTest(failing=failing):
                result, calls = self.invoke("full", "HEAD", cargo_failure=failing.removeprefix("cargo "))
                self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
                self.assertEqual(calls, GUARDS + lint + [REFERENCE_CALL] + expected_tail)

    def test_reference_failure_stops_rust_tests_and_web(self):
        for mode in ("fast", "rust", "full", "test"):
            with self.subTest(mode=mode):
                result, calls = self.invoke(mode, "HEAD", reference_failure=23)
                self.assertEqual(result.returncode, 23, result.stdout + result.stderr)
                self.assertEqual(calls[:len(GUARDS)], GUARDS)
                self.assertEqual(calls[-1], REFERENCE_CALL)
                self.assertFalse(any(call.startswith("cargo test") for call in calls))
                self.assertNotIn("web", calls)

    def test_missing_reference_is_not_silently_skipped(self):
        (self.root / "tests/visual-effects/test_reference.py").unlink()
        result, calls = self.invoke("test", "HEAD")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("test_reference.py", result.stderr)
        self.assertEqual(calls, GUARDS)

    def test_each_guard_stops_compilation_and_cleans_index(self):
        for position, guard in enumerate(GUARDS):
            with self.subTest(guard=guard):
                result, calls = self.invoke("fast", "HEAD", failure=guard)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertEqual(calls, GUARDS[:position + 1])

    def test_unavailable_base_stops_before_guards(self):
        result, calls = self.invoke("fast", "not-present")
        self.assertEqual(result.returncode, 2)
        self.assertIn("comparison base is unavailable", result.stderr)
        self.assertEqual(calls, [])

    def test_missing_guard_is_not_skipped(self):
        (self.root / "scripts" / GUARDS[2]).unlink()
        result, calls = self.invoke("fast", "HEAD")
        self.assertEqual(result.returncode, 2)
        self.assertIn("missing or unreadable guard", result.stderr)
        self.assertEqual(calls, GUARDS[:2])

    def test_help_does_not_run_guards(self):
        result, calls = self.invoke("--help")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
