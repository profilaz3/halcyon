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
    def test_requires_the_cloudflare_collector(self):
        good = "https://halcyon-reports.profilaz3.workers.dev/v1/report"
        self.assertEqual(module.validate_endpoint(good + "\n"), good)
        for bad in ["", good.replace("https:", "http:"), good + "/", good.replace("/v1/report", "/api/summary.json"),
                    good.replace("halcyon-reports.profilaz3.workers.dev", "example.com"), good + "?secret=123",
                    good + "#fragment", good.replace(".workers.dev", ".workers.dev.example.com"),
                    good.replace(".workers.dev", ".workers.dev:444"),
                    good.replace("https://", "https://user@"),
                    "https://script.google.com/macros/s/AKfycb-example_123/exec"]:
            with self.subTest(endpoint=bad), self.assertRaises(ValueError):
                module.validate_endpoint(bad)

    def test_shipped_endpoint_is_valid(self):
        path = Path(__file__).resolve().parents[2] / "assets/usage-endpoint.txt"
        module.validate_endpoint(path.read_text())

    def test_live_gate_requires_feature_snapshot_support(self):
        for schema in (1, 2, 3, 4, 5, 6):
            response = MagicMock()
            response.__enter__.return_value = response
            response.status = 200
            response.read.return_value = json.dumps({"service": "halcyon-activity", "schema": schema}).encode()
            opener = MagicMock()
            opener.open.return_value = response
            with patch("sys.argv", ["release-check", "--check-live"]), \
                    patch.object(module.urllib.request, "build_opener", return_value=opener), \
                    contextlib.redirect_stdout(io.StringIO()):
                if schema < 6:
                    with self.assertRaises(SystemExit):
                        module.main()
                else:
                    module.main()


if __name__ == "__main__":
    unittest.main()
