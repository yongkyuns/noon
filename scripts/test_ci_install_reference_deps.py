"""Exercise dependency setup without sudo, APT, network, or package changes."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest


class ReferenceDependencyInstallTests(unittest.TestCase):
    def invoke(self, packages, *, failure=None, status=100):
        script = Path(__file__).with_name("ci-install-reference-deps.sh")
        with tempfile.TemporaryDirectory(prefix="noon-reference-deps-test-") as directory:
            root = Path(directory)
            sudo = root / "sudo"
            sudo.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['NOON_DEPS_TEST_LOG'], 'a') as output:\n"
                "    output.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if os.environ.get('NOON_DEPS_TEST_FAILURE') in sys.argv[1:]:\n"
                "    sys.exit(int(os.environ['NOON_DEPS_TEST_STATUS']))\n"
            )
            sudo.chmod(0o755)
            log = root / "calls.jsonl"
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                       NOON_DEPS_TEST_LOG=str(log),
                       NOON_DEPS_TEST_FAILURE=failure or "no simulated failure",
                       NOON_DEPS_TEST_STATUS=str(status))
            result = subprocess.run(["bash", str(script), *packages], env=env,
                                    capture_output=True, text=True, timeout=10)
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return result, calls

    def test_update_precedes_install_and_preserves_packages(self):
        packages = ["ffmpeg", "libcairo2-dev", "libpango1.0-dev", "pkg-config",
                    "texlive-latex-extra", "texlive-fonts-recommended",
                    "fonts-dejavu-core", "dvisvgm"]
        result, calls = self.invoke(packages)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0][-2:], ["--error-on=any", "update"])
        self.assertEqual(calls[1][-len(packages)-3:],
                         ["install", "-y", "--no-install-recommends", *packages])
        for call, deadline in zip(calls, ["180s", "600s"]):
            self.assertEqual(call[:8], ["-n", "/usr/bin/timeout", "--signal=TERM",
                             "--kill-after=15s", deadline, "env",
                             "DEBIAN_FRONTEND=noninteractive", "/usr/bin/apt-get"])
            for option in ["Acquire::Retries=2", "Acquire::http::Timeout=20",
                           "Acquire::https::Timeout=20", "DPkg::Lock::Timeout=60"]:
                self.assertIn(option, call)

    def test_failed_update_never_attempts_install(self):
        result, calls = self.invoke(["ffmpeg"], failure="update")
        self.assertEqual(result.returncode, 100)
        self.assertEqual(len(calls), 1)

    def test_timeout_and_forced_kill_remain_failures(self):
        for status in [124, 137]:
            with self.subTest(status=status):
                result, calls = self.invoke(["ffmpeg"], failure="update", status=status)
                self.assertEqual(result.returncode, status)
                self.assertEqual(len(calls), 1)

    def test_failed_install_is_not_masked(self):
        result, calls = self.invoke(["mesa-vulkan-drivers"], failure="install")
        self.assertEqual(result.returncode, 100)
        self.assertEqual(len(calls), 2)

    def test_invalid_input_does_not_start_privileged_setup(self):
        for packages in [[], ["--allow-unauthenticated"], ["ffmpeg", "bad package"],
                         ["ffmpeg; false"], ["$(false)"]]:
            with self.subTest(packages=packages):
                result, calls = self.invoke(packages)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(calls, [])

    def test_workflows_select_and_bound_the_shared_installer(self):
        workflows = Path(__file__).resolve().parents[1] / ".github/workflows"
        expected = {
            "manim-raster-differential.yml": [
                "ffmpeg", "libcairo2-dev", "libpango1.0-dev", "pkg-config",
                "texlive-latex-extra", "texlive-fonts-recommended",
                "fonts-dejavu-core", "dvisvgm"],
            "image-raster-qualification.yml": [
                "ffmpeg", "libcairo2-dev", "libpango1.0-dev", "pkg-config",
                "mesa-vulkan-drivers"],
        }
        for name, packages in expected.items():
            with self.subTest(workflow=name):
                source = (workflows / name).read_text()
                for path in ["ci-install-reference-deps.sh", "test_ci_install_reference_deps.py"]:
                    self.assertIn('      - "scripts/' + path + '"', source)
                steps = source.split("      - name: ")
                install = next(step for step in steps if "bash scripts/ci-install-reference-deps.sh" in step)
                self.assertIn("        timeout-minutes: 15\n", install)
                self.assertIn("python3 -S scripts/test_ci_install_reference_deps.py", install)
                self.assertNotIn("apt-get", install)
                run = install.split("bash scripts/ci-install-reference-deps.sh", 1)[1]
                run = run.split("\n      - ", 1)[0].replace("\\\n", " ")
                self.assertEqual(shlex.split(run), packages)
                self.assertRegex(source, r"--constraint \.github/ci/manim-reference-constraints\.txt.*manim(?:\[typst\])?==0\.21\.0")


if __name__ == "__main__":
    unittest.main()
