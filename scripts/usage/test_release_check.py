import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch, MagicMock
import io
import contextlib
import json

spec = importlib.util.spec_from_file_location("release_check", Path(__file__).with_name("release-check.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ReleaseEndpointTests(unittest.TestCase):
    def test_requires_a_production_google_web_app(self):
        good = "https://script.google.com/macros/s/AKfycb-example_123/exec"
        self.assertEqual(module.validate_endpoint(good + "\n"), good)
        for bad in ["", good.replace("https:", "http:"), good.replace("/exec", "/dev"),
                    good.replace("script.google.com", "example.com"), good + "?secret=123",
                    good + "#fragment", good.replace("AKfycb-example_123", ""),
                    good.replace("script.google.com", "script.google.com:444"),
                    good.replace("script.google.com", "user@script.google.com")]:
            with self.subTest(endpoint=bad), self.assertRaises(ValueError):
                module.validate_endpoint(bad)

    def test_live_gate_requires_installation_metadata_support(self):
        for schema in (1, 2, 3, 4, 5):
            response = MagicMock()
            response.__enter__.return_value = response
            response.status = 200
            response.read.return_value = json.dumps({"service": "halcyon-activity", "schema": schema}).encode()
            opener = MagicMock()
            opener.open.return_value = response
            with patch("sys.argv", ["release-check", "--check-live"]), \
                    patch.object(module.urllib.request, "build_opener", return_value=opener), \
                    contextlib.redirect_stdout(io.StringIO()):
                if schema < 5:
                    with self.assertRaises(SystemExit):
                        module.main()
                else:
                    module.main()


if __name__ == "__main__":
    unittest.main()
