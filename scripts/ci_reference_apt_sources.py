"""Select official HTTPS Ubuntu mirrors without editing host APT configuration."""
import argparse
import os
from pathlib import Path
import re
import shlex
import subprocess


def official_sources(source, architecture):
    if architecture in {"amd64", "i386"}:
        archive = "https://archive.ubuntu.com/ubuntu/"
        security = "https://security.ubuntu.com/ubuntu/"
    elif architecture in {"arm64", "armhf", "ppc64el", "riscv64", "s390x"}:
        archive = security = "https://ports.ubuntu.com/ubuntu-ports/"
    else:
        raise ValueError("unsupported Ubuntu architecture: " + architecture)
    known = {
        "mirror+file:/etc/apt/apt-mirrors.txt": archive,
        "mirror+file:/etc/apt/apt-security-mirrors.txt": security,
        "http://azure.archive.ubuntu.com/ubuntu": archive,
        "https://azure.archive.ubuntu.com/ubuntu": archive,
        "http://archive.ubuntu.com/ubuntu": archive,
        "https://archive.ubuntu.com/ubuntu": archive,
        "http://security.ubuntu.com/ubuntu": security,
        "https://security.ubuntu.com/ubuntu": security,
        "http://ports.ubuntu.com/ubuntu-ports": archive,
        "https://ports.ubuntu.com/ubuntu-ports": archive,
    }
    result = []
    replacements = 0
    lines = iter(source.splitlines(keepends=True))
    pending = None
    while True:
        line = pending if pending is not None else next(lines, None)
        pending = None
        if line is None:
            break
        match = re.fullmatch(r"(?i:URIs):[ \t]*([^\r\n]*)(\r?\n)?", line)
        if match:
            values = [match[1]]
            for following in lines:
                if following[:1] in {" ", "\t"} and following.strip():
                    values.append(following.strip())
                else:
                    pending = following
                    break
            uris = " ".join(values).split()
            if not uris or any(uri.rstrip("/") not in known for uri in uris):
                raise ValueError("unrecognized Ubuntu source URI: " + match[1])
            resolved = dict.fromkeys(known[uri.rstrip("/")] for uri in uris)
            line = "URIs: " + " ".join(resolved) + (match[2] or "")
            replacements += 1
        result.append(line)
    if not replacements:
        raise ValueError("Ubuntu deb822 source has no URI field")
    return "".join(result)


def playwright_apt_config(source):
    """Preserve host settings, then override sources even after binary settings."""
    locations = dict(item.split("=", 1) for item in shlex.split(
        subprocess.check_output([
            "/usr/bin/apt-config", "shell", "parts", "Dir::Etc::parts/d",
            "main", "Dir::Etc::main/f", "root", "Dir", "etc", "Dir::Etc",
            "binary_root", "Binary::apt-get::Dir",
            "binary_etc", "Binary::apt-get::Dir::Etc"], text=True)))
    includes = [os.environ.get("APT_CONFIG"), locations.get("parts"), locations.get("main")]
    settings = {
        "Dir::Etc::parts": "", "Dir::Etc::main": "",
        "Dir::Etc::sourcelist": str(source.resolve()),
        "Dir::Etc::sourceparts": "-", "Acquire::Retries": "2",
        "Acquire::http::Timeout": "20", "Acquire::https::Timeout": "20",
        "DPkg::Lock::Timeout": "60", "APT::Update::Error-Mode": "any",
    }

    def quoted(value):
        if any(character in value for character in ('"', "\\", "\n", "\r")):
            raise ValueError("unsupported APT configuration path")
        return '"' + value + '"'

    lines = []
    for include in includes:
        if include and Path(include).exists():
            path = Path(include).resolve()
            lines.append("#include " + quoted(str(path) + ("/" if path.is_dir() else "")) + ";")
    # APT_CONFIG loads first. Include the existing fragments/main explicitly,
    # then prevent their automatic reload from undoing these final settings.
    # Promoting Binary::apt-get also replaces ancestor values. Preserve the
    # valued directory roots instead of creating empty Dir/Dir::Etc containers
    # that make APT's state, cache, preferences and keyring paths relative.
    for key, name in [("Dir", "root"), ("Dir::Etc", "etc")]:
        value = locations.get("binary_" + name) or locations[name]
        lines.append("Binary::apt-get::" + key + " " + quoted(value) + ";")
    for prefix in ("", "Binary::apt-get::"):
        lines.extend(prefix + key + " " + quoted(value) + ";"
                     for key, value in settings.items())
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--architecture")
    parser.add_argument("--playwright-config", action="store_true",
                        help="emit bounded Playwright APT configuration for this generated source")
    arguments = parser.parse_args()
    if arguments.playwright_config:
        print(playwright_apt_config(arguments.source), end="")
        return
    architecture = arguments.architecture or subprocess.check_output(
        ["/usr/bin/dpkg", "--print-architecture"], text=True).strip()
    print(official_sources(arguments.source.read_text(), architecture), end="")


if __name__ == "__main__":
    main()
