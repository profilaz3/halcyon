"""Offline fixtures for candidate selection and retained pinned releases."""
import unittest
from unittest.mock import Mock

import smoke


class SmokeVersions(unittest.TestCase):
    def releases(self):
        return [{'release': version, 'assets': {'main.deb': {'kind': 'main-deb', 'sha256': digest}}}
                for version, digest in [('0.9.0', 'old'), ('0.10.0', 'new')]]

    def apt(self, candidate='0.10.0', old=True):
        def cmd(action, package, **kwargs):
            if action == 'policy':
                return f'  Candidate: {candidate}\n'
            version = package.split('=')[1]
            if version == '0.9.0' and not old:
                return ''
            return f"Version: {version}\nSHA256: {'old' if version == '0.9.0' else 'new'}\n"
        return Mock(cmd=Mock(side_effect=cmd))

    def test_only_newest_is_candidate_in_either_input_order(self):
        for releases in (self.releases(), list(reversed(self.releases()))):
            self.assertEqual(smoke.check_versions(self.apt(), releases), {'cosmic-ext-halcyon': 'new'})

    def test_old_pinned_release_must_remain_available(self):
        with self.assertRaisesRegex(ValueError, 'pinned_release'):
            smoke.check_versions(self.apt(old=False), self.releases())

    def test_candidate_must_match_exact_newest_version(self):
        for candidate in ('0.9.0', '0.10.01'):
            with self.assertRaisesRegex(ValueError, 'candidate'):
                smoke.check_versions(self.apt(candidate=candidate), self.releases())

    def test_archive_digest_must_match(self):
        releases = self.releases()
        releases[0]['assets']['main.deb']['sha256'] = 'wrong'
        with self.assertRaisesRegex(ValueError, 'pinned_release'):
            smoke.check_versions(self.apt(), releases)
