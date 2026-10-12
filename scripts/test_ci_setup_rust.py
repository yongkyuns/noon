"""Protect runner isolation and caller/future-step Rustup propagation."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class SetupRustTests(unittest.TestCase):
    def test_source_uses_fresh_home_and_preserves_repository_pin(self):
        script = Path(__file__).with_name("ci-setup-rust.sh").resolve()
        with tempfile.TemporaryDirectory(prefix="noon-ci-rust-test-") as directory:
            root = Path(directory)
            binaries = root / "bin"
            binaries.mkdir()
            for tool in ("rustup", "rustc"):
                fake = binaries / tool
                fake.write_text(
                    "#!/usr/bin/env python3\n"
                    "import json, os, sys\n"
                    "with open(os.environ['NOON_RUST_TEST_LOG'], 'a') as output:\n"
                    "    output.write(json.dumps([os.path.basename(sys.argv[0]), "
                    "sys.argv[1:], os.environ['RUSTUP_HOME']]) + '\\n')\n"
                )
                fake.chmod(0o755)
            log = root / "calls.jsonl"
            environment = root / "environment"
            inherited = root / "inherited-rustup"
            inherited.mkdir()
            marker = inherited / "corrupt-component"
            marker.write_text("preserve")
            env = dict(os.environ, RUNNER_TEMP=str(root), GITHUB_ENV=str(environment),
                       RUSTUP_HOME=str(inherited), NOON_RUST_TEST_LOG=str(log),
                       PATH=str(binaries) + os.pathsep + os.environ["PATH"])
            result = subprocess.run(
                ["bash", "-e", "-c", 'source "$1"; printf "%s" "$RUSTUP_HOME"', "setup-test", str(script)],
                env=env, check=True, capture_output=True, text=True,
            )
            fresh = Path(result.stdout)
            self.assertEqual(fresh.parent, root)
            self.assertNotEqual(fresh, inherited)
            self.assertTrue(fresh.is_dir())
            self.assertEqual(marker.read_text(), "preserve")
            self.assertEqual(environment.read_text(), f"RUSTUP_HOME={fresh}\n")
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            self.assertEqual([(tool, args) for tool, args, _ in calls], [
                ("rustup", ["install"]), ("rustup", ["show", "active-toolchain"]),
                ("rustc", ["--version", "--verbose"]),
            ])
            self.assertTrue(all(home == str(fresh) for _, _, home in calls))


if __name__ == "__main__":
    unittest.main()
