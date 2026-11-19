#!/usr/bin/env python3
"""Build the opt-in app-list against its pinned COSMIC source and Cargo.lock."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
REVISION = "779f920c34d7a309acc12d9bba5bc0c53b0abc53"
URL = "https://github.com/pop-os/cosmic-applets.git"


def build(output, source, offline, prepare_source=None):
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
    args = parser.parse_args()
    build(args.output, args.source, args.offline, args.prepare_source)
