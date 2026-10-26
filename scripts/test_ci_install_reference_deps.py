"""Exercise dependency setup without sudo, network, or package changes."""
import json
import importlib.util
import shutil
import os
from pathlib import Path
import re
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
            "native-export-output.yml": ["ffmpeg", "mesa-vulkan-drivers", "libvulkan1"],
            "foreground-matching-qualification.yml": [
                "ffmpeg", "libcairo2-dev", "libpango1.0-dev", "pkg-config"],
        }
        for name, packages in expected.items():
            with self.subTest(workflow=name):
                source = (workflows / name).read_text()
                for path in ["ci-install-reference-deps.sh", "ci_reference_apt_sources.py", "test_ci_install_reference_deps.py"]:
                    self.assertRegex(source, r"(?m)^      - ['\"]scripts/" + re.escape(path) + r"['\"]$")
                steps = source.split("      - name: ")
                install = next(step for step in steps if "bash scripts/ci-install-reference-deps.sh" in step)
                self.assertIn("        timeout-minutes: 15\n", install)
                self.assertIn("python3 -S scripts/test_ci_install_reference_deps.py", install)
                self.assertNotIn("apt-get", install)
                run = next(line.strip() for line in install.replace("\\\n", " ").splitlines()
                           if line.strip().startswith("bash scripts/ci-install-reference-deps.sh "))
                self.assertEqual(shlex.split(run)[2:], packages)
                if name != "native-export-output.yml":
                    self.assertRegex(source, r"--constraint \.github/ci/manim-reference-constraints\.txt.*manim(?:\[typst\])?==0\.21\.0")

    def test_browser_workflows_reuse_the_shared_bounded_installer(self):
        workflows = Path(__file__).resolve().parents[1] / ".github/workflows"
        source = (workflows / "playground-cross-browser.yml").read_text()
        setup = [step for step in source.split("      - name: ")
                 if step.startswith("Install Playwright browser and system dependencies\n")]
        self.assertEqual(len(setup), 2)
        for step in setup:
            self.assertIn("        timeout-minutes: 15\n", step)
            self.assertIn('run: bash scripts/ci-install-playwright-browser.sh ${{ matrix.browser }}', step)
            self.assertNotIn("sudo", step)
        self.assertIn("python3 -S scripts/test_ci_install_reference_deps.py", source)
        for path in ["ci-install-reference-deps.sh", "ci_reference_apt_sources.py", "test_ci_install_reference_deps.py"]:
            self.assertEqual(source.count('      - "scripts/' + path + '"'), 2)
        image = (workflows / "image-raster-qualification.yml").read_text()
        self.assertIn("bash scripts/ci-install-reference-deps.sh --playwright chromium\n"
                      "          npx playwright install chromium", image)



class PlaywrightDependencyInstallTests(unittest.TestCase):
    def invoke(self, arguments, *, status=0, installed=True, hang=False):
        with tempfile.TemporaryDirectory(prefix="noon browser deps ") as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            source = root / "ubuntu.sources"
            source.write_text("Types: deb\nURIs: mirror+file:/etc/apt/apt-mirrors.txt\n"
                              "Suites: noble noble-updates\nComponents: main universe\n"
                              "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n")
            for name in ["ci-install-reference-deps.sh", "ci_reference_apt_sources.py"]:
                content = Path(__file__).with_name(name).read_text()
                content = content.replace("/etc/apt/sources.list.d/ubuntu.sources", str(source))
                # The production path has no spaces; quote the fixture path.
                if name.endswith(".sh"):
                    content = content.replace(str(source) + " >", shlex.quote(str(source)) + " >")
                    if hang:
                        content = content.replace("--kill-after=15s 780s", "--kill-after=1s 0.5s")
                (scripts / name).write_text(content)
            parts = root / "host.conf.d"
            parts.mkdir()
            fragment = parts / "99-host-sources"
            fragment.write_text('Dir::Etc::sourcelist "/host/wrong.sources";\n'
                                'NoonFixture::Fragment "preserved";\n')
            main = root / "host.conf"
            main.write_text('Binary::apt-get::Dir::Etc::sourcelist "/host/wrong-binary.sources";\n'
                            'Binary::apt-get::Acquire::Retries "99";\n'
                            'NoonFixture::Main "preserved";\n')
            bootstrap = root / "bootstrap.conf"
            bootstrap.write_text('Dir::Etc::parts "' + str(parts) + '";\n'
                                 'Dir::Etc::main "' + str(main) + '";\n'
                                 'NoonFixture::Bootstrap "preserved";\n')
            originals = {path: path.read_bytes() for path in [source, fragment, main, bootstrap]}
            result_path, calls_path = root / "result.json", root / "calls.json"
            # APT selects its Binary subtree from argv[0]. Exercise apt-get's
            # effective configuration, rather than the unpromoted parser tree.
            apt_get_config = root / "apt-get"
            apt_get_config.symlink_to("/usr/bin/apt-config")
            if installed:
                package = root / "node_modules/playwright"
                package.mkdir(parents=True)
                (package / "package.json").write_text(json.dumps({
                    "name": "playwright", "version": "1.62.1", "bin": {"playwright": "cli.js"},
                    "exports": {"./package.json": "./package.json"}}))
                (package / "cli.js").write_text(
                    'const fs = require("node:fs"), cp = require("node:child_process");\n'
                    'const query = cp.execFileSync(' + json.dumps(str(apt_get_config)) + ', ["shell",\n'
                    '"source", "Dir::Etc::sourcelist", "root", "Dir", "etc", "Dir::Etc",\n'
                    '"lists", "Dir::State::lists/d", "retries", "Acquire::Retries", "parts", "Dir::Etc::parts",\n'
                    '"main", "Dir::Etc::main", "fragment", "NoonFixture::Fragment",\n'
                    '"hostMain", "NoonFixture::Main", "bootstrap", "NoonFixture::Bootstrap"], {encoding:"utf8"});\n'
                    'const source = query.match(/^source=\x27(.*)\x27$/m)[1];\n'
                    'fs.writeFileSync(' + json.dumps(str(result_path)) + ', JSON.stringify({\n'
                    'args: process.argv.slice(2), aptConfig: process.env.APT_CONFIG, query,\n'
                    'configuration: fs.readFileSync(process.env.APT_CONFIG,"utf8"),\n'
                    'source, sources: fs.readFileSync(source,"utf8")}));\n'
                    + ('setInterval(() => {}, 1000);\n' if hang else
                     'process.exit(' + str(status) + ');\n'))
            sudo = root / "sudo"
            sudo.write_text(
                "#!/usr/bin/env python3\nimport json, subprocess, sys\n"
                "from pathlib import Path\n"
                "Path(" + repr(str(calls_path)) + ").write_text(json.dumps(sys.argv[1:]))\n"
                # Model sudo's environment reset, then execute the real timeout,
                # env, Node and fixture CLI. Only its APT calls are read-only.
                "result = subprocess.run(sys.argv[2:], env={'PATH':'/usr/bin:/bin'})\n"
                "sys.exit(result.returncode)\n")
            sudo.chmod(0o755)
            result = subprocess.run(["bash", str(scripts / "ci-install-reference-deps.sh"), *arguments],
                                    cwd=root, env=dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                                                       APT_CONFIG=str(bootstrap)),
                                    capture_output=True, text=True, timeout=10)
            calls = json.loads(calls_path.read_text()) if calls_path.exists() else []
            evidence = json.loads(result_path.read_text()) if result_path.exists() else None
            self.assertTrue(all(path.read_bytes() == content for path, content in originals.items()))
            if evidence:
                self.assertFalse(Path(evidence["aptConfig"]).exists())
                self.assertFalse(Path(evidence["source"]).exists())
            return result, calls, evidence

    def test_config_crosses_privilege_boundary_and_overrides_late_host_settings(self):
        result, calls, evidence = self.invoke(["--playwright", "firefox"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls[:6], ["-n", "/usr/bin/timeout", "--signal=TERM", "--kill-after=15s", "780s", "env"])
        self.assertIn("APT_CONFIG=" + evidence["aptConfig"], calls)
        self.assertIn("PATH=/usr/sbin:/usr/bin:/sbin:/bin", calls)
        self.assertTrue(Path(calls[-4]).is_absolute())
        self.assertTrue(calls[-3].endswith("/node_modules/playwright/cli.js"))
        self.assertEqual(evidence["args"], ["install-deps", "firefox"])
        settings = dict(item.split("=", 1) for item in shlex.split(evidence["query"]))
        self.assertEqual(settings["source"], evidence["source"])
        self.assertEqual(settings["root"], "/")
        self.assertEqual(settings["etc"], "etc/apt")
        self.assertEqual(settings["lists"], "/var/lib/apt/lists/")
        self.assertEqual(settings["retries"], "2")
        for key in ["fragment", "hostMain", "bootstrap"]:
            self.assertEqual(settings[key], "preserved")
        self.assertFalse(settings.get("parts"))
        self.assertFalse(settings.get("main"))
        self.assertIn("URIs: https://archive.ubuntu.com/ubuntu/\n", evidence["sources"])
        self.assertIn("Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n", evidence["sources"])
        self.assertIn("Suites: noble noble-updates\n", evidence["sources"])

    def test_playwright_failures_and_timeout_statuses_propagate_after_cleanup(self):
        for status in [100, 124, 137]:
            with self.subTest(status=status):
                result, calls, evidence = self.invoke(["--playwright", "chromium"], status=status)
                self.assertEqual(result.returncode, status, result.stderr)
                self.assertTrue(calls)
                self.assertIsNotNone(evidence)

    def test_missing_local_cli_or_invalid_browser_never_starts_privileged_setup(self):
        for arguments, installed in [(["--playwright", "firefox"], False),
                                     (["--playwright", "invalid"], True),
                                     (["--playwright"], True),
                                     (["--playwright", "firefox", "extra"], True)]:
            with self.subTest(arguments=arguments, installed=installed):
                result, calls, evidence = self.invoke(arguments, installed=installed)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])
                self.assertIsNone(evidence)

    def test_deadline_stops_the_child_and_cleans_its_configuration(self):
        result, calls, evidence = self.invoke(["--playwright", "webkit"], hang=True)
        self.assertEqual(result.returncode, 124, result.stderr)
        self.assertTrue(calls)
        self.assertIsNotNone(evidence)

    def test_apt_get_finds_local_indexes_and_preferences_after_binary_overrides(self):
        helper = Path(__file__).with_name("ci_reference_apt_sources.py").resolve()
        for binary_paths in [False, True]:
            with self.subTest(binary_paths=binary_paths), tempfile.TemporaryDirectory(prefix="noon-apt-paths-") as directory:
                root = Path(directory)
                apt_root, work = root / "apt-root", root / "work"
                for name in ["etc/apt/apt.conf.d", "etc/apt/preferences.d", "var/lib/apt/lists/partial", "var/lib/dpkg"]:
                    (apt_root / name).mkdir(parents=True, exist_ok=True)
                work.mkdir()
                status = apt_root / "var/lib/dpkg/status"
                status.write_text("")
                source = root / "ubuntu.sources"
                source.write_text("Types: deb\nURIs: https://archive.ubuntu.com/ubuntu/\n"
                                  "Suites: noble\nComponents: main\nArchitectures: amd64\n"
                                  "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n")
                packages = apt_root / "var/lib/apt/lists/archive.ubuntu.com_ubuntu_dists_noble_main_binary-amd64_Packages"
                packages.write_text("".join(
                    "Package: noon-ci-apt-fixture\nVersion: " + version + "\nArchitecture: amd64\n"
                    "Maintainer: Noon CI <ci@example.invalid>\nDescription: Local APT path fixture\n"
                    "Filename: pool/noon-ci-apt-fixture_" + version + "_amd64.deb\nSize: 1\n\n"
                    for version in ["1.0", "2.0"]))
                (apt_root / "etc/apt/preferences").write_text(
                    "Package: noon-ci-apt-fixture\nPin: version 1.0\nPin-Priority: 1001\n")
                bootstrap = root / "bootstrap.conf"
                bootstrap.write_text(
                    'Dir "' + str(root / "unused" if binary_paths else apt_root) + '";\n'
                    'Dir::Etc "' + ("unused-conf" if binary_paths else "etc/apt") + '";\n'
                    'Dir::Etc::parts "' + str(apt_root / "etc/apt/apt.conf.d") + '";\n'
                    'Dir::Etc::main "-";\nDir::State::status "' + str(status) + '";\n'
                    'Dir::Cache::pkgcache "";\nDir::Cache::srcpkgcache "";\n'
                    'APT::Architecture "amd64";\n' +
                    ('Binary::apt-get::Dir "' + str(apt_root) + '";\n'
                     'Binary::apt-get::Dir::Etc "etc/apt";\n' if binary_paths else ""))
                environment = dict(os.environ, APT_CONFIG=str(bootstrap))
                generated = subprocess.run(["python3", "-S", str(helper), "--playwright-config", str(source)],
                                           env=environment, capture_output=True, text=True, timeout=10)
                self.assertEqual(generated.returncode, 0, generated.stderr)
                configuration = root / "generated.conf"
                configuration.write_text(generated.stdout)
                originals = {path: path.read_bytes() for path in root.rglob("*") if path.is_file()}
                # Simulation reads this fixture's index and pinning. It neither
                # downloads archives nor installs packages or touches host APT.
                result = subprocess.run(["/usr/bin/apt-get", "--simulate", "install", "noon-ci-apt-fixture"],
                                        cwd=work, env=dict(environment, APT_CONFIG=str(configuration)),
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("Inst noon-ci-apt-fixture (1.0 ", result.stdout)
                self.assertNotIn("W:", result.stderr)
                self.assertEqual({path: path.read_bytes() for path in root.rglob("*") if path.is_file()}, originals)


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
