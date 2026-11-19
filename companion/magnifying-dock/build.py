#!/usr/bin/env python3
"""Build the opt-in app-list against its pinned COSMIC source and Cargo.lock."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
REVISION = "779f920c34d7a309acc12d9bba5bc0c53b0abc53"
URL = "https://github.com/pop-os/cosmic-applets.git"


def write_licenses(tree, output, env, offline):
    """Collect notices for the dock applet's dependency graph inside the upstream workspace."""
    manifest = tree / "cosmic-app-list/Cargo.toml"
    cargo_args = ["--manifest-path", str(manifest), "--locked"] + (["--offline"] if offline else [])
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", *cargo_args, "--format-version", "1",
         "--filter-platform", "x86_64-unknown-linux-gnu"], cwd=tree, env=env))
    graph = {node["id"]: node["dependencies"] for node in metadata["resolve"]["nodes"]}
    pending = [metadata["resolve"]["root"]]
    reachable = set()
    while pending:
        package = pending.pop()
        if package not in reachable:
            reachable.add(package)
            pending.extend(graph[package])
    metadata["packages"] = [p for p in metadata["packages"] if p["id"] in reachable]
    packaging = ROOT.parents[1] / "packaging"
    with tempfile.TemporaryDirectory() as scratch:
        work = Path(scratch)
        (work / "metadata.json").write_text(json.dumps(metadata))
        subprocess.run(["cargo", "about", "generate", *cargo_args, "--fail",
                        "--config", str(packaging / "licenses/about.toml"),
                        str(packaging / "licenses/about.hbs"), "--output-file", str(work / "about.txt")],
                       cwd=tree, env=env, check=True)
        subprocess.run([sys.executable, str(packaging / "collect-licenses.py"),
                        "--metadata", str(work / "metadata.json"), "--allow-missing",
                        "--out", str(work / "notices.txt")], env=env, check=True)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes((tree / "LICENSE").read_bytes() + b"\n"
                           + (work / "about.txt").read_bytes() + b"\n" + (work / "notices.txt").read_bytes())


def build(output, source, offline, prepare_source=None, license_output=None):
    env = os.environ.copy()
    if shutil.which("rustup") and not env.get("RUSTUP_TOOLCHAIN"):
        active = subprocess.check_output(["rustup", "show", "active-toolchain"], cwd=ROOT, text=True)
        env["RUSTUP_TOOLCHAIN"] = active.split()[0]
    with tempfile.TemporaryDirectory(prefix="halcyon-app-list-") as temp:
        tree = Path(temp) / "cosmic-applets"
        if offline and not source:
            raise ValueError("--offline requires a local --source checkout")
        subprocess.run(["git", "clone", "--quiet", "--no-checkout", str(source or URL), str(tree)], check=True)
        subprocess.run(["git", "-C", str(tree), "checkout", "--quiet", "--detach", REVISION], check=True)
        subprocess.run(["git", "-C", str(tree), "apply", str(ROOT / "app-list.patch")], check=True)
        shutil.copyfile(ROOT / "magnify.rs", tree / "cosmic-app-list/src/magnify.rs")
        if license_output:
            write_licenses(tree, license_output, env, offline)
        if prepare_source:
            shutil.copytree(tree, prepare_source, ignore=shutil.ignore_patterns(".git", "target"))
            return
        target = Path(env.get("CARGO_TARGET_DIR", tree / "target")).resolve()
        command = ["cargo", "build", "--locked", "--release", "-p", "cosmic-app-list", "--target-dir", str(target)]
        if offline:
            command.append("--offline")
        subprocess.run(command, cwd=tree, env=env, check=True)
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(target / "release/cosmic-app-list", output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT.parents[1] / "target/release/halcyon-magnifying-app-list")
    parser.add_argument("--source", type=Path)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--prepare-source", type=Path, metavar="DIR",
                        help="write the patched source tree for a corresponding-source archive")
    parser.add_argument("--license-output", type=Path, help="write the dock license bundle using cargo-about")
    args = parser.parse_args()
    build(args.output, args.source, args.offline, args.prepare_source, args.license_output)
