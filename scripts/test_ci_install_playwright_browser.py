"""Check browser dependency setup without APT, sudo privileges or network."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class PlaywrightBrowserInstallTests(unittest.TestCase):
    def invoke(self, browsers, *, platform="Linux", failure="", source_failure=False):
        source = Path(__file__).with_name("ci-install-playwright-browser.sh")
        with tempfile.TemporaryDirectory(prefix="noon-playwright-setup-") as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            shutil.copy2(source, scripts / source.name)
            (scripts / "ci_reference_apt_sources.py").write_text(
                "import sys\n" + ("sys.exit(65)\n" if source_failure else
                "assert sys.argv[1] == '/etc/apt/sources.list.d/ubuntu.sources'\n"
                "print('Types: deb\\nURIs: https://archive.ubuntu.com/ubuntu/\\nSuites: noble noble-updates\\nComponents: main universe\\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg')\n"))
            package = root / "playwright"
            package.mkdir()
            (package / "package.json").write_text("{}")
            (package / "cli.js").write_text("// installed pinned CLI fixture")
            log = root / "calls.jsonl"
            node = root / "node"
            node.write_text("""#!/usr/bin/env python3
import json, os, pathlib, sys
if sys.argv[1] == '-p':
    print(os.environ['NOON_TEST_PACKAGE'])
    sys.exit(0)
args = sys.argv[1:]
entry = {'kind':'node', 'args':args, 'aptConfig':os.environ.get('APT_CONFIG')}
if entry['aptConfig']:
    config = pathlib.Path(entry['aptConfig'])
    entry['config'] = config.read_text()
    sources = config.parent / 'ubuntu.sources'
    entry['sources'] = sources.read_text()
    entry['modes'] = [oct(p.stat().st_mode & 0o777) for p in [config.parent, config, sources]]
with open(os.environ['NOON_TEST_LOG'],'a') as out: out.write(json.dumps(entry)+'\\n')
if os.environ['NOON_TEST_FAILURE'] == args[1]: sys.exit(100)
""")
            sudo = root / "sudo"
            sudo.write_text("""#!/usr/bin/env python3
import json, os, subprocess, sys
with open(os.environ['NOON_TEST_LOG'],'a') as out: out.write(json.dumps({'kind':'sudo','args':sys.argv[1:]})+'\\n')
if os.environ['NOON_TEST_FAILURE'] == 'sudo': sys.exit(51)
assert sys.argv[1] == '-n'
sys.exit(subprocess.call(sys.argv[2:]))
""")
            uname = root / "uname"
            uname.write_text("#!/usr/bin/env python3\nimport os\nprint(os.environ['NOON_TEST_PLATFORM'])\n")
            for executable in [node, sudo, uname]:
                executable.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                       NOON_TEST_PACKAGE=str(package / "package.json"), NOON_TEST_LOG=str(log),
                       NOON_TEST_PLATFORM=platform, NOON_TEST_FAILURE=failure)
            env.pop("APT_CONFIG", None)
            result = subprocess.run(["bash", str(scripts / source.name), *browsers],
                                    env=env, capture_output=True, text=True, timeout=15)
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            for call in calls:
                if call.get("aptConfig"):
                    self.assertFalse(Path(call["aptConfig"]).exists(), "temporary config must be cleaned")
            return result, calls

    def test_linux_roots_only_system_dependencies_and_retains_user_browser_install(self):
        for browser in ["chromium", "firefox", "webkit"]:
            with self.subTest(browser=browser):
                result, calls = self.invoke([browser])
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual([c["kind"] for c in calls], ["sudo", "node", "node"])
                self.assertEqual(calls[0]["args"][:2], ["-n", "env"])
                self.assertTrue(calls[0]["args"][2].startswith("APT_CONFIG="))
                deps, install = calls[1:]
                self.assertEqual(deps["args"][-2:], ["install-deps", browser])
                self.assertEqual(install["args"][-2:], ["install", browser])
                self.assertIsNone(install["aptConfig"])
                self.assertIn('Dir::Etc::sourceparts "-";', deps["config"])
                self.assertIn("URIs: https://archive.ubuntu.com/ubuntu/", deps["sources"])
                self.assertIn("Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg", deps["sources"])
                self.assertEqual(deps["modes"], ["0o755", "0o644", "0o644"])

    def test_non_linux_retains_the_original_combined_install(self):
        result, calls = self.invoke(["webkit"], platform="Darwin")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([c["kind"] for c in calls], ["node"])
        self.assertEqual(calls[0]["args"][-3:], ["install", "--with-deps", "webkit"])
        self.assertIsNone(calls[0]["aptConfig"])

    def test_source_failure_never_elevates_or_downloads(self):
        result, calls = self.invoke(["firefox"], source_failure=True)
        self.assertEqual(result.returncode, 65)
        self.assertEqual(calls, [])

    def test_privilege_failure_never_downloads(self):
        result, calls = self.invoke(["firefox"], failure="sudo")
        self.assertEqual(result.returncode, 51)
        self.assertEqual([c["kind"] for c in calls], ["sudo"])

    def test_dependency_failure_never_downloads(self):
        result, calls = self.invoke(["firefox"], failure="install-deps")
        self.assertEqual(result.returncode, 100)
        self.assertEqual([c["kind"] for c in calls], ["sudo", "node"])

    def test_browser_install_failure_is_not_masked(self):
        result, calls = self.invoke(["firefox"], failure="install")
        self.assertEqual(result.returncode, 100)
        self.assertEqual([c["kind"] for c in calls], ["sudo", "node", "node"])

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
