#!/usr/bin/env python3
"""Pins what the voice training needs, for setup.sh and for the app.

    uv run --python 3.12 tools/voice-training/pin.py

For each computer the app trains on (Windows and Linux, with an NVIDIA card or the processor)
it resolves requirements.txt, PyTorch and FFmpeg at the versions in requirements.lock, and
writes two files to environments/:

- <computer>.lock, the requirements with their SHA-256, which setup.sh installs from;
- <computer>.json, every wheel of that computer with its address, size and SHA-256, which the
  app downloads one by one, so it can show the progress and resume a download that stopped.

Run it again after changing requirements.txt, and commit what changes.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
import tomllib
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE / "environments"
PYTORCH = "2.7.1"
# Decodes the recordings for RVC on the processor, which calls the ffmpeg program.
FFMPEG = "imageio-ffmpeg==0.6.0"
PYTHON = "3.12"
# The build of Python the app installs, through uv, which checks it against its own hashes.
PYTHON_BUILD = "3.12.13"

WINDOWS = ("win_amd64",)
LINUX = tuple(f"manylinux_2_{minor}_x86_64" for minor in range(5, 29)) + (
    "manylinux1_x86_64",
    "manylinux2010_x86_64",
    "manylinux2014_x86_64",
    "linux_x86_64",
)

COMPUTERS = {
    # name: (uv's platform, platform tags of the wheels it takes, PyTorch index)
    "windows-cuda": ("x86_64-pc-windows-msvc", WINDOWS, "cu128"),
    "windows-cpu": ("x86_64-pc-windows-msvc", WINDOWS, "cpu"),
    "linux-cuda": ("x86_64-manylinux_2_28", LINUX, "cu128"),
    "linux-cpu": ("x86_64-manylinux_2_28", LINUX, "cpu"),
}


def compile_lock(platform: str, index: str, output: Path, *options: str) -> None:
    # Relative paths, run from here: uv cuts a path with a space in a requirements file.
    with tempfile.TemporaryDirectory(dir=HERE) as folder:
        extra = Path(folder) / "extra.txt"
        extra.write_text(
            f"torch=={PYTORCH}\ntorchaudio=={PYTORCH}\n{FFMPEG}\n", encoding="utf-8"
        )
        subprocess.run(
            [
                shutil.which("uv") or "uv",
                "pip",
                "compile",
                "requirements.txt",
                str(extra.relative_to(HERE)),
                "--constraint",
                "requirements.lock",
                "--python-version",
                PYTHON,
                "--python-platform",
                platform,
                "--no-header",
                "--index-url",
                "https://pypi.org/simple",
                "--extra-index-url",
                f"https://download.pytorch.org/whl/{index}",
                "--index-strategy",
                "unsafe-best-match",
                "--output-file",
                str(output),
                *options,
            ],
            cwd=HERE,
            check=True,
        )


def takes(wheel: str, platforms: tuple[str, ...]) -> bool:
    """Whether a wheel installs on CPython 3.12 of the computer."""
    python, abi, platform = wheel.rsplit("/", 1)[-1].removesuffix(".whl").split("-")[-3:]
    pythons = python.split(".")
    fits_python = any(p in ("cp312", "py3") for p in pythons) or (
        abi == "abi3" and any(p.startswith("cp3") for p in pythons)
    )
    fits_platform = platform == "any" or any(
        part in platforms for part in platform.split(".")
    )
    return fits_python and fits_platform


def size_of(url: str) -> int:
    request = urllib.request.Request(url, method="HEAD")
    with urllib.request.urlopen(request, timeout=60) as response:
        return int(response.headers["Content-Length"])


def from_pypi(name: str, version: str, url: str) -> dict:
    """The same file on PyPI. PyTorch's index mirrors a few packages of PyPI without their
    hashes."""
    file = url.rsplit("/", 1)[-1]
    address = f"https://pypi.org/pypi/{name}/{version}/json"
    with urllib.request.urlopen(address, timeout=60) as response:
        release = json.load(response)
    for found in release["urls"]:
        if found["filename"] == file:
            return {
                "url": found["url"],
                "size": found["size"],
                "hashes": {"sha256": found["digests"]["sha256"]},
            }
    sys.exit(f"{file} is not on PyPI")


def manifest(pylock: Path, tags: tuple[str, ...]) -> list[dict]:
    wheels = []
    for package in tomllib.loads(pylock.read_text(encoding="utf-8"))["packages"]:
        found = [w for w in package.get("wheels", []) if takes(w["url"], tags)]
        if not found:
            sys.exit(f"no wheel of {package['name']} {package['version']} for {tags}")
        # The most specific one: a wheel for the platform over a pure one.
        wheel = sorted(found, key=lambda w: w["url"].endswith("-any.whl"))[0]
        if "sha256" not in wheel["hashes"]:
            wheel = from_pypi(package["name"], package["version"], wheel["url"])
        wheels.append(
            {
                "name": package["name"],
                "version": package["version"],
                "url": wheel["url"],
                "size": wheel.get("size") or size_of(wheel["url"]),
                "sha256": wheel["hashes"]["sha256"],
            }
        )
    return wheels


def main() -> None:
    OUT.mkdir(exist_ok=True)
    for name, (platform, tags, index) in COMPUTERS.items():
        compile_lock(platform, index, OUT / f"{name}.lock", "--generate-hashes")
        with tempfile.TemporaryDirectory() as folder:
            pylock = Path(folder) / "pylock.toml"
            compile_lock(platform, index, pylock)
            wheels = manifest(pylock, tags)
        (OUT / f"{name}.json").write_text(
            json.dumps({"python": PYTHON_BUILD, "wheels": wheels}, indent=1) + "\n",
            encoding="utf-8",
        )
        total = sum(wheel["size"] for wheel in wheels)
        print(f"{name}: {len(wheels)} wheels, {total / 1e6:.0f} MB")


if __name__ == "__main__":
    main()
