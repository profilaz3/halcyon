"""Offline tests with tiny lockfiles and vendor folders; never invoke Cargo."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "source_closure", Path(__file__).with_name("source_closure.py"))
closure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(closure)

ICED = "git+https://github.com/profilaz3/iced?rev=22c073b9e305d1d4738fd21547763cfcdf852b74"
LIBCOSMIC = "git+https://github.com/pop-os/libcosmic"
LOCK = f"""version = 4

[[package]]
name = "cosmic-ext-halcyon"
version = "0.6.1"

[[package]]
name = "demo"
version = "1.2.3"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "00"

[[package]]
name = "iced_core"
version = "0.14.0"
source = "{ICED}#22c073b9e305d1d4738fd21547763cfcdf852b74"

[[package]]
name = "libcosmic"
version = "1.0.0"
source = "{LIBCOSMIC}#dc11d6a11f601692fb8fd67fdc99775f0a9eb899"
"""
REPLACEMENTS = {
    "crates-io": '[source.crates-io]\nreplace-with = "vendored-sources"\n',
    LIBCOSMIC: f'[source."{LIBCOSMIC}"]\ngit = "https://github.com/pop-os/libcosmic"\n'
               'replace-with = "vendored-sources"\n',
    ICED: f'[source."{ICED}"]\ngit = "https://github.com/profilaz3/iced"\n'
          'rev = "22c073b9e305d1d4738fd21547763cfcdf852b74"\nreplace-with = "vendored-sources"\n',
}


class SourceClosureTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.tree = self.root / "main"
        (self.tree / ".cargo").mkdir(parents=True)
        (self.tree / "Cargo.lock").write_text(LOCK)
        self.dependencies = self.root / "dependencies/main"
        for name in ("demo-1.2.3", "iced_core-0.14.0", "libcosmic-1.0.0"):
            (self.dependencies / name).mkdir(parents=True)
            (self.dependencies / name / ".cargo-checksum.json").write_text("{}")

    def configure(self, *keys, directory="../dependencies/main"):
        text = "\n".join(REPLACEMENTS[key] for key in keys)
        text += f'\n[source.vendored-sources]\ndirectory = "{directory}"\n'
        (self.tree / ".cargo/config.toml").write_text(text)

    def test_complete_closure_passes(self):
        self.configure("crates-io", LIBCOSMIC, ICED)
        self.assertEqual(closure.check(self.tree), {"vendored": 3, "git_sources": 2})

    def test_config_from_a_release_before_the_iced_fork_is_rejected(self):
        self.configure("crates-io", LIBCOSMIC)
        with self.assertRaisesRegex(ValueError, "no replacement for .*profilaz3/iced"):
            closure.check(self.tree)

    def test_replacement_for_another_revision_does_not_count(self):
        self.configure("crates-io", LIBCOSMIC, ICED)
        config = self.tree / ".cargo/config.toml"
        config.write_text(config.read_text().replace('rev = "22c073b9', 'rev = "11c073b9'))
        with self.assertRaisesRegex(ValueError, "profilaz3/iced"):
            closure.check(self.tree)

    def test_missing_vendored_package_and_registry_replacement_are_named(self):
        self.configure(LIBCOSMIC, ICED)
        (self.dependencies / "iced_core-0.14.0/.cargo-checksum.json").unlink()
        with self.assertRaises(ValueError) as caught:
            closure.check(self.tree)
        self.assertIn("crates.io is not replaced", str(caught.exception))
        self.assertIn("not vendored: iced_core 0.14.0", str(caught.exception))

    def test_missing_config_is_explained(self):
        with self.assertRaisesRegex(ValueError, "vendor command"):
            closure.check(self.tree)

    def test_git_source_parsing(self):
        self.assertEqual(closure.git_source(ICED + "#22c073b"),
                         ("https://github.com/profilaz3/iced", "rev",
                          "22c073b9e305d1d4738fd21547763cfcdf852b74"))
        self.assertEqual(closure.git_source(LIBCOSMIC + "#dc11d6a"),
                         ("https://github.com/pop-os/libcosmic", None, None))
        self.assertEqual(closure.git_source("git+https://github.com/pop-os/winit.git?tag=cosmic-0.14#f737"),
                         ("https://github.com/pop-os/winit.git", "tag", "cosmic-0.14"))
        with self.assertRaises(ValueError):
            closure.git_source("git+https://example.test/x?rev=1&tag=2#3")

    def test_every_git_source_in_the_real_lockfile_parses(self):
        repo = Path(__file__).resolve().parent.parent
        sources = {source for _, _, source in closure.locked_packages(repo)
                   if source.startswith("git+")}
        self.assertTrue(sources)
        for source in sources:
            url, kind, _ = closure.git_source(source)
            self.assertTrue(url.startswith("https://"), source)
            self.assertIn(kind, (None, *closure.REFERENCES))

    def test_vendor_writes_relative_directory_and_checks(self):
        (self.tree / ".cargo").rmdir()
        printed = "\n".join(REPLACEMENTS.values()) + (
            f'\n[source.vendored-sources]\ndirectory = "{self.root}/new-dependencies"\n')
        destination = self.root / "new-dependencies"

        def cargo_vendor(command, **kwargs):
            self.assertEqual(command[:4], ["cargo", "vendor", "--locked", "--versioned-dirs"])
            self.assertEqual(kwargs["cwd"], self.tree.resolve())
            self.dependencies.rename(destination)
            return subprocess.CompletedProcess(command, 0, stdout=printed, stderr="")

        with patch.object(closure.subprocess, "run", side_effect=cargo_vendor):
            self.assertEqual(closure.vendor(self.tree, destination)["git_sources"], 2)
        self.assertIn('directory = "../new-dependencies"',
                      (self.tree / ".cargo/config.toml").read_text())

    def test_vendor_refuses_existing_config(self):
        self.configure("crates-io", LIBCOSMIC, ICED)
        with patch.object(closure.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "already exists"):
                closure.vendor(self.tree, self.root / "elsewhere")
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
