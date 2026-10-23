"""Exercise dependency setup without sudo, APT, network, or package changes."""
import json
import importlib.util
import shutil
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest


class ReferenceDependencyInstallTests(unittest.TestCase):
    def invoke(self, packages, *, failure=None, status=100, source_failure=False):
        script = Path(__file__).with_name("ci-install-reference-deps.sh")
        with tempfile.TemporaryDirectory(prefix="noon-reference-deps-test-") as directory:
            root = Path(directory)
            copied = root / "scripts"
            copied.mkdir()
            shutil.copy2(script, copied / script.name)
            script = copied / script.name
            (copied / "ci_reference_apt_sources.py").write_text(
                "import sys\n" + ("sys.exit(65)\n" if source_failure else
                "assert sys.argv[1] == '/etc/apt/sources.list.d/ubuntu.sources'\n"
                "print('Types: deb\\nURIs: https://archive.ubuntu.com/ubuntu/\\nSuites: noble noble-updates\\nComponents: main universe\\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg')\n"))
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
        source_options = [item for item in calls[0] if item.startswith("Dir::Etc::sourcelist=")]
        self.assertEqual(len(source_options), 1)
        self.assertIn(source_options[0], calls[1])
        self.assertFalse(Path(source_options[0].split("=", 1)[1]).exists())
        for call in calls:
            self.assertIn("Dir::Etc::sourceparts=-", call)
        for call, deadline in zip(calls, ["180s", "600s"]):
            self.assertEqual(call[:8], ["-n", "/usr/bin/timeout", "--signal=TERM",
                             "--kill-after=15s", deadline, "env",
                             "DEBIAN_FRONTEND=noninteractive", "/usr/bin/apt-get"])
            for option in ["Acquire::Retries=2", "Acquire::http::Timeout=20",
                           "Acquire::https::Timeout=20", "DPkg::Lock::Timeout=60"]:
                self.assertIn(option, call)

    def test_source_generation_failure_never_starts_apt(self):
        result, calls = self.invoke(["ffmpeg"], source_failure=True)
        self.assertEqual(result.returncode, 65)
        self.assertEqual(calls, [])

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
                for path in ["ci-install-reference-deps.sh", "ci_reference_apt_sources.py", "test_ci_install_reference_deps.py"]:
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



class OfficialUbuntuSourcesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = Path(__file__).with_name("ci_reference_apt_sources.py")
        spec = importlib.util.spec_from_file_location("ci_reference_apt_sources", path)
        cls.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.module)

    def source(self, uri="mirror+file:/etc/apt/apt-mirrors.txt"):
        return ("Types: deb\nURIs: " + uri + "\nSuites: noble noble-updates noble-backports\n"
                "Components: main restricted universe multiverse\n"
                "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n\n"
                "Types: deb\nURIs: mirror+file:/etc/apt/apt-security-mirrors.txt\n"
                "Suites: noble-security\nComponents: main restricted universe multiverse\n"
                "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n")

    def test_changes_only_uris_preserving_suites_components_and_signatures(self):
        source = self.source()
        result = self.module.official_sources(source, "amd64")
        expected = source.replace("mirror+file:/etc/apt/apt-mirrors.txt", "https://archive.ubuntu.com/ubuntu/")
        expected = expected.replace("mirror+file:/etc/apt/apt-security-mirrors.txt", "https://security.ubuntu.com/ubuntu/")
        self.assertEqual(result, expected)
        self.assertEqual(self.module.official_sources(result, "amd64"), result)

    def test_azure_and_duplicate_mirror_uris_use_one_official_endpoint(self):
        source = self.source("http://azure.archive.ubuntu.com/ubuntu/ https://archive.ubuntu.com/ubuntu/")
        self.assertEqual(self.module.official_sources(source, "amd64"),
                         self.module.official_sources(self.source(), "amd64"))

    def test_wrapped_uri_values_are_resolved_or_rejected_together(self):
        wrapped = self.source("http://azure.archive.ubuntu.com/ubuntu/\n https://archive.ubuntu.com/ubuntu/")
        self.assertEqual(self.module.official_sources(wrapped, "amd64"),
                         self.module.official_sources(self.source(), "amd64"))
        with self.assertRaises(ValueError):
            self.module.official_sources(self.source("http://azure.archive.ubuntu.com/ubuntu/\n https://untrusted.example/ubuntu/"), "amd64")

    def test_arm_uses_ports_for_archive_and_security(self):
        result = self.module.official_sources(self.source(), "arm64")
        self.assertEqual(result.count("URIs: https://ports.ubuntu.com/ubuntu-ports/"), 2)
        self.assertNotIn("https://archive.ubuntu.com", result)

    def test_missing_empty_unknown_or_unsupported_sources_fail_closed(self):
        for source, architecture in [("Types: deb\n", "amd64"), ("URIs: \n", "amd64"),
                                     (self.source("https://untrusted.example/ubuntu/"), "amd64"),
                                     (self.source(), "unknown")]:
            with self.subTest(source=source, architecture=architecture):
                with self.assertRaises(ValueError):
                    self.module.official_sources(source, architecture)

    def test_cli_reads_only_requested_source_without_mutating_it(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "ubuntu.sources"
            source.write_text(self.source())
            before = source.read_bytes()
            result = subprocess.run(["python3", "-S", str(Path(__file__).with_name("ci_reference_apt_sources.py")),
                                     str(source), "--architecture", "amd64"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, self.module.official_sources(self.source(), "amd64"))
            self.assertEqual(source.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
