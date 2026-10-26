"""Check browser dependency setup without APT, sudo privileges or network."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class PlaywrightBrowserInstallTests(unittest.TestCase):
    def invoke(self, browsers, *, platform="Linux", dependency_status=0,
               install_status=0, installed=True):
        source = Path(__file__).with_name("ci-install-playwright-browser.sh")
        with tempfile.TemporaryDirectory(prefix="noon-playwright-setup-") as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            shutil.copy2(source, scripts / source.name)
            # The shared helper's suite exercises its actual APT configuration,
            # privilege boundary and deadline. This fixture observes the wrapper
            # handoff and proves that a failed setup never starts a download.
            (scripts / "ci-install-reference-deps.sh").write_text("""#!/usr/bin/env bash
python3 - "$@" <<'PY'
import json, os, sys
with open(os.environ['NOON_TEST_LOG'], 'a') as out:
    out.write(json.dumps({'kind': 'dependencies', 'args': sys.argv[1:]}) + '\\n')
sys.exit(int(os.environ['NOON_TEST_DEPENDENCY_STATUS']))
PY
""")
            package = root / "playwright"
            package.mkdir()
            (package / "package.json").write_text("{}")
            if installed:
                (package / "cli.js").write_text("// installed pinned CLI fixture")
            log = root / "calls.jsonl"
            node = root / "node"
            node.write_text("""#!/usr/bin/env python3
import json, os, pathlib, sys
if sys.argv[1] == '-p':
    print(str(pathlib.Path(__file__).resolve()) if sys.argv[2] == 'process.execPath'
          else os.environ['NOON_TEST_PACKAGE'])
    sys.exit(0)
args = sys.argv[1:]
entry = {'kind':'node', 'args':args, 'aptConfig':os.environ.get('APT_CONFIG')}
with open(os.environ['NOON_TEST_LOG'],'a') as out: out.write(json.dumps(entry)+'\\n')
sys.exit(int(os.environ['NOON_TEST_INSTALL_STATUS']))
""")
            uname = root / "uname"
            uname.write_text("#!/usr/bin/env python3\nimport os\nprint(os.environ['NOON_TEST_PLATFORM'])\n")
            for executable in [node, uname]:
                executable.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                       NOON_TEST_PACKAGE=str(package / "package.json"), NOON_TEST_LOG=str(log),
                       NOON_TEST_PLATFORM=platform,
                       NOON_TEST_DEPENDENCY_STATUS=str(dependency_status),
                       NOON_TEST_INSTALL_STATUS=str(install_status))
            env.pop("APT_CONFIG", None)
            result = subprocess.run(["bash", str(scripts / source.name), *browsers],
                                    env=env, capture_output=True, text=True, timeout=15)
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return result, calls

    def test_linux_delegates_dependencies_before_the_user_browser_install(self):
        for browser in ["chromium", "firefox", "webkit"]:
            with self.subTest(browser=browser):
                result, calls = self.invoke([browser])
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual([c["kind"] for c in calls], ["dependencies", "node"])
                deps, install = calls
                self.assertEqual(deps["args"], ["--playwright", browser])
                self.assertEqual(install["args"][-2:], ["install", browser])
                self.assertTrue(Path(install["args"][0]).is_absolute())
                self.assertTrue(install["args"][0].endswith("/playwright/cli.js"))
                self.assertIsNone(install["aptConfig"])

    def test_non_linux_retains_the_original_combined_install(self):
        result, calls = self.invoke(["webkit"], platform="Darwin")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([c["kind"] for c in calls], ["node"])
        self.assertEqual(calls[0]["args"][-3:], ["install", "--with-deps", "webkit"])
        self.assertIsNone(calls[0]["aptConfig"])

    def test_shared_setup_failures_and_deadlines_never_download(self):
        for status in [51, 65, 100, 124, 137]:
            with self.subTest(status=status):
                result, calls = self.invoke(["firefox"], dependency_status=status)
                self.assertEqual(result.returncode, status)
                self.assertEqual([c["kind"] for c in calls], ["dependencies"])

    def test_browser_install_failure_is_not_masked(self):
        result, calls = self.invoke(["firefox"], install_status=100)
        self.assertEqual(result.returncode, 100)
        self.assertEqual([c["kind"] for c in calls], ["dependencies", "node"])

    def test_missing_local_cli_never_starts_setup_or_downloads(self):
        for platform in ["Linux", "Darwin"]:
            with self.subTest(platform=platform):
                result, calls = self.invoke(["firefox"], platform=platform, installed=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])

    def test_invalid_inputs_never_start_setup(self):
        for browsers in [[], ["--with-deps"], ["firefox", "chromium"], ["firefox; false"]]:
            with self.subTest(browsers=browsers):
                result, calls = self.invoke(browsers)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(calls, [])

    def test_workflow_routes_both_jobs_without_changing_existing_deadlines_or_pins(self):
        p = Path(__file__).resolve().parents[1] / ".github/workflows/playground-cross-browser.yml"
        source = p.read_text()
        self.assertEqual(source.count("run: bash scripts/ci-install-playwright-browser.sh ${{ matrix.browser }}"), 2)
        for step in source.split("      - name: "):
            if "bash scripts/ci-install-playwright-browser.sh" in step:
                self.assertIn("timeout-minutes: 15", step)
        self.assertIn("playwright@1.62.1", source)
        self.assertIn("python3 -S scripts/test_ci_install_playwright_browser.py", source)
        self.assertIn("node --test --test-concurrency=2 scripts/gallery-shards.test.mjs scripts/playground-stress-observation.test.mjs", source)
        self.assertIn("timeout-minutes: 20", source)
        self.assertIn('NOON_GALLERY_SHARD_COUNT: "3"', source)
        for name in ["ci-install-playwright-browser.sh", "ci_reference_apt_sources.py", "test_ci_install_playwright_browser.py"]:
            self.assertEqual(source.count('      - "scripts/' + name + '"'), 2)


if __name__ == "__main__":
    unittest.main()
